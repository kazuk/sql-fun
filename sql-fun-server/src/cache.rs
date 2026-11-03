use ::tokio::sync::oneshot;
use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, Mutex},
};

/// Concurrent cache with server-owned loads and one receiver per request.
///
/// Successful results can be retained for the cache lifetime. Errors are always
/// retryable; represent a deterministic negative result in the value type.
pub struct Cache<K, V, E> {
    state: Arc<Mutex<State<K, V, E>>>,
    retain: bool,
}

struct State<K, V, E> {
    completed: HashMap<K, V>,
    pending: HashMap<K, Vec<oneshot::Sender<Result<V, E>>>>,
}

impl<K, V, E> Cache<K, V, E>
where
    K: Hash + Eq + Clone + Send + 'static,
    V: Clone + Send + 'static,
    E: Clone + Send + 'static,
{
    /// Create a cache with explicit successful-result retention.
    #[must_use]
    pub fn new(retain: bool) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                completed: HashMap::new(),
                pending: HashMap::new(),
            })),
            retain,
        }
    }

    /// Look up a completed value without waiting for a load.
    ///
    /// # Panics
    /// Panics if the cache state mutex was poisoned.
    pub fn lookup(&self, key: &K) -> Option<V> {
        self.state
            .lock()
            .expect("cache state")
            .completed
            .get(key)
            .cloned()
    }

    #[cfg(test)]
    pub(crate) fn active_waiters(&self, key: &K) -> usize {
        self.state
            .lock()
            .expect("cache state")
            .pending
            .get(key)
            .map_or(0, |waiters| {
                waiters.iter().filter(|waiter| !waiter.is_closed()).count()
            })
    }

    /// Publish once, returning the existing value if another load completed first.
    ///
    /// # Panics
    /// Panics if the cache state mutex was poisoned.
    pub fn publish(&self, key: &K, value: &V) -> V {
        self.state
            .lock()
            .expect("cache state")
            .completed
            .entry(key.clone())
            .or_insert_with(|| value.clone())
            .clone()
    }

    /// Share a load, retaining only successful values when configured to do so.
    ///
    /// Dropping any or all waiting requests does not cancel the spawned task.
    /// No cache lock is held while loading. Panics follow the server's fail-fast
    /// process policy; this cache does not recover panicking tasks.
    ///
    /// # Panics
    /// Panics if state is poisoned or a loader terminates without publication.
    /// The server executable aborts on loader panics.
    ///
    /// # Errors
    /// Returns the loader's error; the next request retries.
    pub async fn load<F, Fut>(&self, key: &K, loader: F) -> Result<V, E>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<V, E>> + Send + 'static,
    {
        let receiver = {
            let mut state = self.state.lock().expect("cache state");
            if let Some(value) = state.completed.get(key) {
                return Ok(value.clone());
            }
            let (sender, receiver) = oneshot::channel();
            if let Some(waiters) = state.pending.get_mut(key) {
                waiters.push(sender);
            } else {
                state.pending.insert(key.clone(), vec![sender]);
                let owned = Arc::clone(&self.state);
                let key = key.clone();
                let retain = self.retain;
                // Spawn before releasing the lock, with no await/cancellation gap.
                ::tokio::spawn(async move {
                    let result = loader().await;
                    let waiters = {
                        let mut state = owned.lock().expect("cache state");
                        if retain && let Ok(value) = &result {
                            state
                                .completed
                                .entry(key.clone())
                                .or_insert_with(|| value.clone());
                        }
                        state.pending.remove(&key).expect("registered load")
                    };
                    for waiter in waiters {
                        let _ = waiter.send(result.clone());
                    }
                });
            }
            receiver
        };
        receiver.await.expect("server-owned loader must complete")
    }
}

#[cfg(test)]
mod tests {
    use crate::cache::Cache;
    use ::tokio::sync::oneshot;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[tokio::test]
    async fn canceled_initiator_does_not_cancel_population() {
        let cache = Arc::new(Cache::<String, usize, ()>::new(true));
        let key = "schema".to_owned();
        let (started, start_rx) = oneshot::channel();
        let (release, release_rx) = oneshot::channel();
        let owner = Arc::clone(&cache);
        let task_key = key.clone();
        let task = ::tokio::spawn(async move {
            owner
                .load(&task_key, || async move {
                    started.send(()).unwrap();
                    release_rx.await.unwrap();
                    Ok(42)
                })
                .await
        });
        start_rx.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let follower = cache.load(&key, || async { panic!("duplicate load") });
        release.send(()).unwrap();
        assert_eq!(follower.await.unwrap(), 42);
        assert_eq!(cache.lookup(&key), Some(42));
    }

    #[tokio::test]
    async fn all_waiters_can_disconnect_while_one_load_populates() {
        let cache = Arc::new(Cache::<String, usize, ()>::new(true));
        let key = "shared".to_owned();
        let (started, start_rx) = oneshot::channel();
        let (release, release_rx) = oneshot::channel();
        let owner = Arc::clone(&cache);
        let task_key = key.clone();
        let mut tasks = vec![::tokio::spawn(async move {
            owner
                .load(&task_key, || async move {
                    started.send(()).unwrap();
                    release_rx.await.unwrap();
                    Ok(99)
                })
                .await
        })];
        start_rx.await.unwrap();
        for _ in 0..15 {
            let owner = Arc::clone(&cache);
            let task_key = key.clone();
            tasks.push(::tokio::spawn(async move {
                owner
                    .load(&task_key, || async { panic!("duplicate in-flight load") })
                    .await
            }));
        }
        ::tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let count = cache.state.lock().unwrap().pending.get(&key).unwrap().len();
                if count == 16 {
                    break;
                }
                ::tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        for task in tasks {
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
        }
        release.send(()).unwrap();
        ::tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while cache.lookup(&key).is_none() {
                ::tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(cache.lookup(&key), Some(99));
    }

    #[tokio::test]
    async fn concurrent_requests_share_load_and_errors_retry() {
        let cache = Arc::new(Cache::<String, usize, usize>::new(true));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..16 {
            let cache = Arc::clone(&cache);
            let calls = Arc::clone(&calls);
            tasks.push(::tokio::spawn(async move {
                cache
                    .load(&"root".to_owned(), move || async move {
                        calls.fetch_add(1, Ordering::Relaxed);
                        Ok(7)
                    })
                    .await
                    .unwrap()
            }));
        }
        for task in tasks {
            assert_eq!(task.await.unwrap(), 7);
        }
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            cache.load(&"bad".to_owned(), || async { Err(1) }).await,
            Err(1)
        );
        assert_eq!(
            cache.load(&"bad".to_owned(), || async { Ok(2) }).await,
            Ok(2)
        );
    }
}
