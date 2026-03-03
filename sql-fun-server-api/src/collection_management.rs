use time::UtcDateTime;

/// collection handle
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(transparent)]
pub struct CollectionHandle(String);

/// argument for [`SqlFunServerApi::release_collection`]
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct ReleaseCollectionArgs {
    /// releasing collection handle
    collection_handle: CollectionHandle,
}

/// argument for [`SqlFunServerApi::read_collection`]
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

    /// expiration time
    #[cfg_attr(
        feature = "openrpc-schema",
        schemars(schema_with = "self::schema_util::utc_datetime_schema")
    )]
    expire_at: UtcDateTime,

    /// collection items
    items: Vec<T>,
}

#[cfg(feature = "openrpc-schema")]
mod schema_util {
    use schemars::{Schema, SchemaGenerator};

    pub fn utc_datetime_schema(_gen: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({
            "type": "string",
            "format": "date-time"
        })
    }
}
