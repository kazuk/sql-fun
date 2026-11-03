use crate::payloads::{CollectionHandle, CollectionPage, CollectionReference, CollectionTraversal};

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

impl<T: CollectionTraversal + ::serde::de::DeserializeOwned + 'static>
    crate::payloads::CollectionTraversal for Collection<T>
{
    fn collections(&self) -> Vec<&dyn CollectionReference> {
        vec![self]
    }
    fn collections_mut(&mut self) -> Vec<&mut dyn CollectionReference> {
        vec![self]
    }
}
impl<T: CollectionTraversal + ::serde::de::DeserializeOwned + 'static>
    crate::payloads::CollectionReference for Collection<T>
{
    fn page(&self) -> Option<CollectionPage> {
        Some(CollectionPage::new(
            &self.handle,
            self.total_length,
            self.offset,
            self.require_release,
            self.items.len(),
        ))
    }
    fn insert(
        &mut self,
        offset: usize,
        values: Vec<::serde_json::Value>,
    ) -> Result<(), ::serde_json::Error> {
        let page = self;
        let index = offset.saturating_sub(page.offset);
        if index > page.items.len() {
            return Err(<::serde_json::Error as ::serde::de::Error>::custom(
                "collection insertion exceeds decoded storage",
            ));
        }
        for (inserted, value) in values.into_iter().enumerate() {
            let item = ::serde_json::from_value(value)?;
            page.items.insert(index + inserted, item);
            // Preserve successfully decoded prefix items even if a later item fails.
            page.offset = page.offset.min(offset);
        }
        Ok(())
    }
    fn children(&self) -> Vec<&dyn CollectionReference> {
        CollectionTraversal::collections(&self.items)
    }
    fn children_mut(&mut self) -> Vec<&mut dyn CollectionReference> {
        CollectionTraversal::collections_mut(&mut self.items)
    }
    fn finish(&mut self) {
        // Server pages retain their wire representation; only clients own completion state.
    }
}
