//! Shared cache setup for integration-test crates.

pub(crate) fn create_cache_context() -> ::sql_fun_sqlast::CacheContext {
    ::sql_fun_sqlast::CacheContext::new()
}
