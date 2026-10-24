use crate::InitializedSchema;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use ::elsa::sync::FrozenMap;
use sql_fun_server_api::payloads::ContextHandle;

// Only issuance identity is process-wide; registrations and schemas belong to wrappers.
static HANDLE_IDS: HandleIdAllocator = HandleIdAllocator::new();

/// Connection-owned, append-only strong schema registrations.
///
/// Borrowed schemas remain valid across insertion and storage growth. There is
/// deliberately no removal, replacement, or mutable storage access: later
/// handler-local cursors may borrow schemas until the registry itself is dropped.
/// The synchronized map permits the connection future to move between workers;
/// schema reads return ordinary borrows, without retaining a lock guard.
pub(super) struct ContextHandleRegistry {
    schemas: FrozenMap<ContextHandle, Box<RegisteredContext>>,
}

// Query output policy belongs to a registration, independently of shared catalog layers.
struct RegisteredContext {
    schema: Arc<InitializedSchema>,
    rewrite_options: ::sql_fun_core::RewriteOptions,
}

impl ContextHandleRegistry {
    pub(super) fn new() -> Self {
        Self {
            schemas: FrozenMap::new(),
        }
    }

    /// Retain the shared schema under a fresh handle, even for repeat registration.
    pub(super) fn register(
        &self,
        schema: &Arc<InitializedSchema>,
    ) -> Result<ContextHandle, HandleIssuanceError> {
        self.register_with_options(schema, &::sql_fun_core::RewriteOptions::default())
    }

    pub(super) fn register_with_options(
        &self,
        schema: &Arc<InitializedSchema>,
        rewrite_options: &::sql_fun_core::RewriteOptions,
    ) -> Result<ContextHandle, HandleIssuanceError> {
        // Exhaustion must be detected before any registration is inserted.
        let handle = HANDLE_IDS.issue()?;
        self.schemas.insert(
            handle.clone(),
            Box::new(RegisteredContext {
                schema: Arc::clone(schema),
                rewrite_options: *rewrite_options,
            }),
        );
        Ok(handle)
    }

    /// Resolve only registrations belonging to this connection.
    ///
    /// The result borrows the registry, not a request, facade, or lock guard.
    pub(super) fn resolve(&self, handle: &ContextHandle) -> Option<&InitializedSchema> {
        self.schemas
            .get(handle)
            .map(|context| context.schema.as_ref())
    }

    pub(super) fn rewrite_options(
        &self,
        handle: &ContextHandle,
    ) -> Option<&::sql_fun_core::RewriteOptions> {
        self.schemas
            .get(handle)
            .map(|context| &context.rewrite_options)
    }
}

#[cfg(test)]
mod test_context_handle_registry;

/// Monotonic process-local issuance, independent of schema/cache identities.
struct HandleIdAllocator(AtomicU64);

impl HandleIdAllocator {
    const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    fn issue(&self) -> Result<ContextHandle, HandleIssuanceError> {
        // Ordering carries no schema publication: uniqueness alone is required.
        #[expect(
            deprecated,
            reason = "try_update requires Rust 1.95; the workspace MSRV is 1.93"
        )]
        let id = self
            .0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| HandleIssuanceError)?;
        Ok(ContextHandle::from(format!("context-{id}")))
    }
}

#[cfg(test)]
mod test_handle_id_allocator {
    use super::HandleIdAllocator;
    use std::{collections::HashSet, sync::atomic::AtomicU64};

    #[test]
    fn exhaustion_never_wraps_or_reuses_an_id() {
        let ids = HandleIdAllocator(AtomicU64::new(u64::MAX - 1));
        let last = ids.issue().unwrap();
        assert!(ids.issue().is_err());
        assert!(ids.issue().is_err());
        assert!(last != HandleIdAllocator::new().issue().unwrap());
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
        let unique: HashSet<_> = handles.into_iter().collect();
        assert_eq!(unique.len(), 128);
    }
}

#[derive(Debug, thiserror::Error)]
#[error("context handle IDs exhausted")]
pub(super) struct HandleIssuanceError;
