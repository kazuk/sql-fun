//! Helpers loading real builtin metadata. Each call initializes its own catalog.

pub(crate) mod drop_rule;
pub(crate) mod drop_trigger;
#[cfg(feature = "sqlite")]
pub(crate) mod drop_trigger_sqlite;
pub(crate) mod selected_metadata;
pub(crate) mod selected_root;
