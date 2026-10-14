/// collection handle
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct CollectionHandle(String);

impl From<String> for CollectionHandle {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// argument for [`crate::SqlFunServerApi::release_collection`]
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct ReleaseCollectionArgs {
    /// releasing collection handle
    collection_handle: CollectionHandle,
}

/// argument for [`crate::SqlFunServerApi::read_collection`]
///
/// The item type returned by `read_collection` is identical to the element type
/// of the original `Collection<T>` that produced the handle, and never changes.
/// This method only reads the continuation of that collection; no type conversion
/// is performed.
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct ReadCollectionArgs {
    /// collection
    collection_handle: CollectionHandle,

    /// start offset
    start_offset: usize,

    /// read count
    ///
    /// when None, client requests read to end, but server implementation has a hard limit
    count: Option<usize>,
}

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

#[cfg(test)]
mod test_collection {
    use super::{Collection, CollectionHandle};

    #[test]
    fn serializes_page_without_expiration_and_preserves_item_type() {
        let handle = CollectionHandle::from("collection-7".to_owned());
        let page = Collection::new(&handle, 3, 1, true, vec![42_u32]);
        let json = ::serde_json::to_value(&page).unwrap();
        assert_eq!(
            json,
            ::serde_json::json!({
                "handle": "collection-7", "total_length": 3, "offset": 1,
                "require_release": true, "items": [42]
            })
        );
        let decoded: Collection<u32> = ::serde_json::from_value(json).unwrap();
        assert_eq!(decoded.handle(), &handle);
        assert_eq!(decoded.items(), &[42]);
    }

    #[cfg(feature = "openrpc-schema")]
    #[test]
    fn generated_schema_has_no_expiration_property_or_requirement() {
        let schema = ::schemars::schema_for!(Collection<u32>);
        let json = ::serde_json::to_value(schema).unwrap();
        assert!(json["properties"].get("expire_at").is_none());
        assert_eq!(json["properties"].as_object().unwrap().len(), 5);
        let required = json["required"].as_array().unwrap();
        assert_eq!(required.len(), 5);
        assert!(!required.contains(&::serde_json::json!("expire_at")));
    }
}
