//! Shared support compiled into the root of each integration-test crate.
// Each binary uses a subset of these helpers.
#![allow(dead_code)]

mod cache_context;
pub(crate) mod common_ast_deparse;
pub(crate) mod common_ast_mutation;
pub(crate) mod heavy;
pub(crate) mod qualified_columns;
pub(crate) mod schema_context_chain;
pub(crate) mod schema_file_loading;

pub(crate) use self::cache_context::create_cache_context;
