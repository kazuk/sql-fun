//! Session-owned state reserved for query-analysis caches.

/// Cache state owned by an analysis caller and reused across its requests.
///
/// This context is currently empty. It supplements the shared parse context
/// without providing catalog, configuration, or diagnostic services.
pub struct CacheContext {}

impl CacheContext {
    /// Create an empty cache context for an analysis session.
    #[allow(
        clippy::new_without_default,
        reason = "Only new() is exposed to cache owners"
    )]
    pub const fn new() -> Self {
        Self {}
    }
}
