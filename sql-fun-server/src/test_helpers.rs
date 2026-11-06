pub(crate) mod heavy;
pub(crate) mod pid_file;

/// Create independent cache state for a test-owned analysis base.
pub(crate) fn create_cache_context() -> ::sql_fun_sqlast::CacheContext {
    ::sql_fun_sqlast::CacheContext::new()
}
