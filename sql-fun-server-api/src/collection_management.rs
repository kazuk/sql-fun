use time::UtcDateTime;

/// collection handle
#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(transparent)]
pub struct CollectionHandle(String);

/// argument for [`SqlFunServerApi::release_collection`]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct ReleaseCollectionArgs {
    /// releasing collection handle
    collection_handle: CollectionHandle,
}

/// argument for [`SqlFunServerApi::read_collection`]
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
    expire_at: UtcDateTime,

    /// collection items
    items: Vec<T>,
}
