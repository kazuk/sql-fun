use std::{
    hash::RandomState,
    marker::PhantomData,
    time::{Duration, Instant},
};

use indexmap::IndexMap;
use tokio::sync::RwLock;

use crate::PendingFut;

pub struct Loader<K, V, E, F, FutureResult>
where
    K: std::hash::Hash + std::cmp::Eq,
    V: Clone,
    E: Clone,
    F: Fn(K) -> FutureResult,
    FutureResult: Future<Output = Result<V, E>> + Send + 'static,
{
    func: Box<F>,
    _phantom: PhantomData<K>,
}

impl<K, V, E, F, FutureResult> Loader<K, V, E, F, FutureResult>
where
    K: std::hash::Hash + std::cmp::Eq,
    V: Clone,
    E: Clone,
    F: Fn(K) -> FutureResult,
    FutureResult: Future<Output = Result<V, E>> + Send + 'static,
{
    pub fn new(fun: F) -> Self {
        Self {
            func: Box::new(fun),
            #[allow(clippy::default_constructed_unit_structs)]
            _phantom: PhantomData::default(),
        }
    }

    pub fn load(&self, key: K) -> PendingFut<V, E> {
        PendingFut::new((self.func)(key))
    }
}

#[derive(Clone, Debug)]
enum CacheEntry<Value, Error>
where
    Value: Clone + std::fmt::Debug,
    Error: Clone + std::fmt::Debug,
{
    Starting,
    Pending(PendingFut<Value, Error>),
    Occupied(Value, Instant),
    Error(Error, Instant),
}

impl<Value, Error> CacheEntry<Value, Error>
where
    Value: Clone + std::fmt::Debug,
    Error: Clone + std::fmt::Debug,
{
    pub fn pending(fut: &PendingFut<Value, Error>) -> Self {
        Self::Pending(fut.clone())
    }

    pub fn to_result(&self) -> Result<Value, Error> {
        #[allow(clippy::match_wildcard_for_single_variants)]
        match self {
            Self::Occupied(v, _ttl) => Ok(v.clone()),
            Self::Error(e, _ttl) => Err(e.clone()),
            _ => panic!("can not to convert result {self:?}"),
        }
    }

    pub fn update_with_result(&mut self, value: &Result<Value, Error>) {
        match value {
            Ok(v) => *self = Self::Occupied(v.clone(), Instant::now()),
            Err(e) => *self = Self::Error(e.clone(), Instant::now()),
        }
    }

    pub fn update_last_used(&mut self) {
        #[allow(clippy::match_wildcard_for_single_variants)]
        match self {
            Self::Occupied(_, last_used) | Self::Error(_, last_used) => {
                *last_used = Instant::now();
            }
            _ => {}
        }
    }

    pub const fn last_used(&self) -> Option<Instant> {
        match self {
            Self::Pending(_) | Self::Starting => None,
            Self::Occupied(_, lu) | Self::Error(_, lu) => Some(*lu),
        }
    }
}

pub struct Cache<Key, Value, Error, LoaderFun, FutureResult, S = RandomState>
where
    Key: std::hash::Hash + std::cmp::Eq + Clone + Copy,
    Value: Clone + std::fmt::Debug,
    Error: Clone + std::fmt::Debug,
    LoaderFun: Fn(Key) -> FutureResult,
    FutureResult: Future<Output = Result<Value, Error>> + Send + 'static,
{
    load_fn: Loader<Key, Value, Error, LoaderFun, FutureResult>,
    entries: RwLock<IndexMap<Key, CacheEntry<Value, Error>, S>>,
    expilation_timeout: Duration,
}

impl<Key, Value, Error, LoaderFun, FutureResult> Cache<Key, Value, Error, LoaderFun, FutureResult>
where
    Key: std::hash::Hash + std::cmp::Eq + Clone + Copy,
    Value: Clone + std::fmt::Debug,
    Error: Clone + std::fmt::Debug,
    LoaderFun: Fn(Key) -> FutureResult,
    FutureResult: Future<Output = Result<Value, Error>> + Send + 'static,
{
    pub fn new(loader: LoaderFun, expilation_timeout: Duration) -> Self {
        Self {
            load_fn: Loader::new(loader),
            entries: RwLock::new(IndexMap::new()),
            expilation_timeout,
        }
    }

    async fn get_entry_or_insert_starting(&self, key: &Key) -> Option<CacheEntry<Value, Error>> {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(key) {
            entry.update_last_used();
            Some(entry.clone())
        } else {
            entries.insert(*key, CacheEntry::Starting);
            None
        }
    }

    async fn get_entry_expect_pending(&self, key: &Key) -> Option<CacheEntry<Value, Error>> {
        let entry = loop {
            let entry = {
                let entries = self.entries.read().await;
                entries.get(key).cloned()
            };
            if matches!(entry, Some(CacheEntry::Starting)) {
                tokio::task::yield_now().await;
                continue;
            }
            break entry;
        };
        if matches!(entry, Some(CacheEntry::Pending(_))) {
            entry
        } else {
            None
        }
    }

    async fn insert_entry(&self, key: &Key, entry: &CacheEntry<Value, Error>) {
        let mut entries = self.entries.write().await;
        entries.insert(*key, entry.clone());
    }

    async fn update_entry_with_value(
        &self,
        key: &Key,
        value: Result<Value, Error>,
    ) -> Result<Value, Error> {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(key) {
            if matches!(entry, CacheEntry::Pending(_)) {
                entry.update_with_result(&value);
            }
            entry.to_result()
        } else {
            value
        }
    }

    /// get cached value
    ///
    /// # Errors
    ///
    /// when loader function returns error
    ///
    /// # Remarks:
    ///
    /// This method is not cancellation-safe. Callers must not cancel/drop the
    /// future before it completes; doing so may leave the cache in a
    /// Starting/Pending state. Panics inside the loader can also leave a Starting
    /// entry behind.
    ///
    pub async fn get(&self, key: Key) -> Result<Value, Error> {
        if let Some(entry) = self.get_entry_or_insert_starting(&key).await {
            match entry {
                CacheEntry::Pending(cell) => {
                    let result = cell.await;
                    self.update_entry_with_value(&key, result).await
                }
                CacheEntry::Occupied(value, _ttl) => Ok(value),
                CacheEntry::Error(e, _ttl) => Err(e),
                CacheEntry::Starting => {
                    if let Some(CacheEntry::Pending(cell)) =
                        self.get_entry_expect_pending(&key).await
                    {
                        let result = cell.await;
                        self.update_entry_with_value(&key, result).await
                    } else {
                        // can not to catch pending state
                        // may be loader is immediately completed and current state is Occupied / Error.
                        //
                        // then recursive call to this function results correct value
                        self.get(key).await
                    }
                }
            }
        } else {
            let pending_fut = self.load_fn.load(key);
            self.insert_entry(&key, &CacheEntry::pending(&pending_fut))
                .await;
            let result = pending_fut.await;
            self.update_entry_with_value(&key, result).await
        }
    }

    pub async fn evict_entries(&self) {
        let mut entries = self.entries.write().await;

        #[allow(clippy::unchecked_time_subtraction)]
        let expire_before = Instant::now() - self.expilation_timeout;

        entries.retain(|_key, entry| match entry.last_used() {
            None => true,
            Some(last_used) => last_used >= expire_before,
        });
    }
}
