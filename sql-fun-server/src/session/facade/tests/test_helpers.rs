use crate::InitializedSchema;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use ::sql_fun_sqlast::context::BaseParseContext;
use ::sql_fun_sqlast::{
    names::{FullName, TableName},
    sem::{ColumnDefinition, CreateTable},
};

/// Real schema borrow with a destructor that observes schema data on teardown.
/// Uses the small synthetic `SchemaProject` fixture, not the full builtin catalog.
pub(super) struct SchemaColumns<'ctx> {
    table: &'ctx CreateTable,
    dropped: Arc<AtomicBool>,
}

impl<'ctx> SchemaColumns<'ctx> {
    pub(super) fn new(schema: &'ctx InitializedSchema, dropped: &Arc<AtomicBool>) -> Self {
        let name = TableName::from(FullName::with_schema("public", "retained"));
        Self {
            table: schema.base_context().get_table(&name).unwrap(),
            dropped: Arc::clone(dropped),
        }
    }

    pub(super) fn len(&self) -> usize {
        (0..)
            .map_while(|index| self.table.get_column_at(index))
            .count()
    }

    pub(super) fn map(column: &ColumnDefinition) -> ::serde_json::Value {
        ::serde_json::json!(column.name().as_ref().unwrap().as_str())
    }
}

impl<'read> IntoIterator for &'read SchemaColumns<'_> {
    type Item = &'read ColumnDefinition;
    type IntoIter = Box<dyn Iterator<Item = Self::Item> + 'read>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new((0..).map_while(|index| self.table.get_column_at(index)))
    }
}

impl Drop for SchemaColumns<'_> {
    fn drop(&mut self) {
        assert!(self.table.get_column_at(0).unwrap().name().is_some());
        self.dropped.store(true, Ordering::Relaxed);
    }
}
