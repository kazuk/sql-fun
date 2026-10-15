use super::CollectionHandle;

/// Enumerates collections using the response type's authoritative field layout.
/// JSON values are opaque leaves, even when their contents resemble a page.
pub trait CollectionTraversal: Send {
    /// Collections directly contained in this response, excluding their items.
    fn collections(&self) -> Vec<&dyn CollectionReference> {
        Vec::new()
    }
    /// Mutable references to the same directly contained collections.
    fn collections_mut(&mut self) -> Vec<&mut dyn CollectionReference> {
        Vec::new()
    }
}

/// Type-erased access to a concrete collection's storage and nested items.
pub trait CollectionReference: Send {
    /// Metadata for an unfinished collection; completed collections return None.
    fn page(&self) -> Option<CollectionPage>;
    /// Decode and insert a page at its absolute offset.
    ///
    /// # Errors
    /// Returns an item decoding error, retaining previously decoded items.
    fn insert(
        &mut self,
        offset: usize,
        values: Vec<::serde_json::Value>,
    ) -> Result<(), ::serde_json::Error>;
    /// Enumerate collections in the currently decoded items.
    fn children(&self) -> Vec<&dyn CollectionReference>;
    /// Enumerate mutable collections in the currently decoded items.
    fn children_mut(&mut self) -> Vec<&mut dyn CollectionReference>;
    /// Mark successful materialization after all required releases have succeeded.
    fn finish(&mut self);
}

/// Snapshot of page metadata, independent of its item type.
#[derive(Clone, derive_getters::Getters)]
pub struct CollectionPage {
    handle: CollectionHandle,
    total_length: usize,
    offset: usize,
    require_release: bool,
    length: usize,
}
impl CollectionPage {
    /// Construct metadata for a decoded page.
    #[must_use]
    pub fn new(
        handle: &CollectionHandle,
        total_length: usize,
        offset: usize,
        require_release: bool,
        length: usize,
    ) -> Self {
        Self {
            handle: handle.clone(),
            total_length,
            offset,
            require_release,
            length,
        }
    }
}

impl<T: CollectionTraversal> CollectionTraversal for Vec<T> {
    fn collections(&self) -> Vec<&dyn CollectionReference> {
        self.iter()
            .flat_map(CollectionTraversal::collections)
            .collect()
    }
    fn collections_mut(&mut self) -> Vec<&mut dyn CollectionReference> {
        self.iter_mut()
            .flat_map(CollectionTraversal::collections_mut)
            .collect()
    }
}
impl<T: CollectionTraversal> CollectionTraversal for Option<T> {
    fn collections(&self) -> Vec<&dyn CollectionReference> {
        match self {
            Some(value) => value.collections(),
            None => Vec::new(),
        }
    }
    fn collections_mut(&mut self) -> Vec<&mut dyn CollectionReference> {
        match self {
            Some(value) => value.collections_mut(),
            None => Vec::new(),
        }
    }
}
impl<T: CollectionTraversal> CollectionTraversal for Box<T> {
    fn collections(&self) -> Vec<&dyn CollectionReference> {
        self.as_ref().collections()
    }
    fn collections_mut(&mut self) -> Vec<&mut dyn CollectionReference> {
        self.as_mut().collections_mut()
    }
}
macro_rules! leaf {
    ($($ty:ty),* $(,)?) => { $(impl CollectionTraversal for $ty {})* };
}
leaf!(
    (),
    String,
    bool,
    u8,
    u16,
    u32,
    u64,
    usize,
    i8,
    i16,
    i32,
    i64,
    isize,
    f32,
    f64,
    ::serde_json::Value
);
