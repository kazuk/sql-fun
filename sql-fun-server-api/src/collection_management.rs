use crate::payloads::CollectionHandle;

/// collection
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct Collection<T> {
    /// handle for collecttion
    handle: CollectionHandle,

    /// length of collection
    total_length: usize,

    /// offset of items
    offset: usize,

    /// When true, client must be call `SqlFunServerApi::release_collection`
    require_release: bool,

    /// collection items
    items: Vec<T>,
}

impl<T> Collection<T> {
    /// Construct a page without converting its items.
    ///
    /// Retained collections remain readable until explicit release or connection
    /// teardown. Reaching the last page does not release a retained collection.
    /// This constructor takes ownership of the page's items to avoid cloning them.
    #[must_use]
    pub fn new(
        handle: &CollectionHandle,
        total_length: usize,
        offset: usize,
        require_release: bool,
        items: Vec<T>,
    ) -> Self {
        Self {
            handle: handle.clone(),
            total_length,
            offset,
            require_release,
            items,
        }
    }
}
