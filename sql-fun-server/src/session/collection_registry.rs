use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::atomic::{AtomicU64, Ordering},
};

use ::serde_json::Value;
use ::sql_fun_server_api::Collection;
use sql_fun_server_api::payloads::CollectionHandle;

static HANDLE_IDS: HandleIdAllocator = HandleIdAllocator::new();

/// Connection-local reusable sources, owned by the handler rather than the wrapper.
///
/// Sources may borrow schemas retained by the wrapper's append-only context
/// registry. They live until explicit release or handler teardown. There is no
/// expiry, byte accounting, collection-count cap, or automatic eviction.
pub(super) struct CollectionRegistry<'ctx> {
    sources: HashMap<CollectionHandle, Box<dyn CollectionSource + Send + 'ctx>>,
    page_limit: NonZeroUsize,
}

impl<'ctx> CollectionRegistry<'ctx> {
    pub(super) fn new(page_limit: NonZeroUsize) -> Self {
        Self {
            sources: HashMap::new(),
            page_limit,
        }
    }

    /// Register a reusable source without iterating or serializing its items.
    ///
    /// Borrowing the source must implement `IntoIterator` for every read lifetime.
    /// The caller guarantees immutable contents, deterministic ordering, and an
    /// exact, stable length. The mapping preserves the original API item shape.
    /// Ownership is transferred so both owned and schema-borrowing sources can
    /// remain registered after the registering request returns.
    pub(super) fn register<S, L, M>(
        &mut self,
        source: S,
        length: L,
        map: M,
    ) -> Result<CollectionHandle, CollectionRegistryError>
    where
        S: Send + 'ctx,
        L: Fn(&S) -> usize + Send + 'ctx,
        M: Send + 'ctx,
        for<'read> &'read S: IntoIterator,
        for<'read> M: Fn(<&'read S as IntoIterator>::Item) -> Value,
    {
        self.register_source(
            Box::new(MappedSource {
                source,
                length,
                map,
            }),
            &HANDLE_IDS,
        )
    }

    fn register_source(
        &mut self,
        source: Box<dyn CollectionSource + Send + 'ctx>,
        ids: &HandleIdAllocator,
    ) -> Result<CollectionHandle, CollectionRegistryError> {
        let handle = ids.issue()?;
        self.sources.insert(handle.clone(), source);
        Ok(handle)
    }

    /// Current number of unreleased collections, in O(1).
    ///
    /// API handlers can use this through the later Session facade to decide
    /// whether to require releases before accepting more work. No threshold is
    /// imposed by the registry itself.
    pub(super) fn len(&self) -> usize {
        self.sources.len()
    }

    /// Read a bounded page using a fresh iterator, without advancing stored state.
    pub(super) fn read(
        &self,
        handle: &CollectionHandle,
        start_offset: usize,
        count: Option<usize>,
    ) -> Result<Collection<Value>, CollectionRegistryError> {
        let source = self
            .sources
            .get(handle)
            .ok_or_else(CollectionRegistryError::invalid_handle)?;
        let total_length = source.len();
        let count = count
            .unwrap_or(self.page_limit.get())
            .min(self.page_limit.get())
            .min(total_length.saturating_sub(start_offset));
        let items = source.iter().skip(start_offset).take(count).collect();
        Ok(Collection::new(
            handle,
            total_length,
            start_offset,
            true,
            items,
        ))
    }

    /// Drop a registered source immediately; repeated release is an error.
    pub(super) fn release(
        &mut self,
        handle: &CollectionHandle,
    ) -> Result<(), CollectionRegistryError> {
        self.sources
            .remove(handle)
            .ok_or_else(CollectionRegistryError::invalid_handle)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;

/// Type erasure keeps concrete storage and JSON mappings out of the registry.
trait CollectionSource {
    fn len(&self) -> usize;
    fn iter(&self) -> Box<dyn Iterator<Item = Value> + '_>;
}

struct MappedSource<S, L, M> {
    source: S,
    length: L,
    map: M,
}

impl<S, L, M> CollectionSource for MappedSource<S, L, M>
where
    L: Fn(&S) -> usize,
    for<'read> &'read S: IntoIterator,
    for<'read> M: Fn(<&'read S as IntoIterator>::Item) -> Value,
{
    fn len(&self) -> usize {
        (self.length)(&self.source)
    }

    fn iter(&self) -> Box<dyn Iterator<Item = Value> + '_> {
        Box::new((&self.source).into_iter().map(&self.map))
    }
}

struct HandleIdAllocator(AtomicU64);

impl HandleIdAllocator {
    const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    fn issue(&self) -> Result<CollectionHandle, CollectionRegistryError> {
        #[expect(
            deprecated,
            reason = "try_update requires Rust 1.95; the workspace MSRV is 1.93"
        )]
        let id = self
            .0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| CollectionRegistryError::handle_ids_exhausted())?;
        Ok(CollectionHandle::from(format!("collection-{id}")))
    }
}

#[cfg(test)]
mod test_handle_id_allocator {
    use super::HandleIdAllocator;
    use std::{collections::HashSet, sync::atomic::AtomicU64};

    #[test]
    fn exhaustion_never_wraps() {
        let ids = HandleIdAllocator(AtomicU64::new(u64::MAX - 1));
        let last = ids.issue().unwrap();
        assert!(ids.issue().is_err());
        assert!(ids.issue().is_err());
        assert_ne!(last, HandleIdAllocator::new().issue().unwrap());
    }

    #[test]
    fn concurrent_issuance_is_unique() {
        let ids = HandleIdAllocator::new();
        let handles = std::thread::scope(|scope| {
            let first = scope.spawn(|| (0..64).map(|_| ids.issue().unwrap()).collect::<Vec<_>>());
            let second = scope.spawn(|| (0..64).map(|_| ids.issue().unwrap()).collect::<Vec<_>>());
            let mut handles = first.join().unwrap();
            handles.extend(second.join().unwrap());
            handles
        });
        assert_eq!(handles.into_iter().collect::<HashSet<_>>().len(), 128);
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum CollectionRegistryError {
    #[error("invalid collection handle")]
    InvalidHandle,
    #[error("collection handle IDs exhausted")]
    HandleIdsExhausted,
}

impl CollectionRegistryError {
    const fn invalid_handle() -> Self {
        Self::InvalidHandle
    }

    const fn handle_ids_exhausted() -> Self {
        Self::HandleIdsExhausted
    }
}

impl From<CollectionRegistryError> for ::sql_fun_server_api::SqlFunServerApiError {
    fn from(error: CollectionRegistryError) -> Self {
        match error {
            CollectionRegistryError::InvalidHandle => Self::invalid_collection_handle(),
            CollectionRegistryError::HandleIdsExhausted => Self::new_unexpected(&error.to_string()),
        }
    }
}
