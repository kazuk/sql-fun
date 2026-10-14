use super::super::CollectionRegistry;
use crate::InitializedSchema;
use std::{
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use ::serde_json::Value;
use ::sql_fun_sqlast::{
    TrieMap,
    sem::{BaseParseContext, ColumnDefinition, CreateTable, FullName, TableName},
};

pub(super) fn registry<'ctx>(limit: usize) -> CollectionRegistry<'ctx> {
    CollectionRegistry::new(NonZeroUsize::new(limit).unwrap())
}

pub(super) struct BorrowedTrie<'ctx>(&'ctx TrieMap<Value>);

impl<'ctx> From<&'ctx TrieMap<Value>> for BorrowedTrie<'ctx> {
    fn from(map: &'ctx TrieMap<Value>) -> Self {
        Self(map)
    }
}

impl BorrowedTrie<'_> {
    pub(super) fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'read> IntoIterator for &'read BorrowedTrie<'_> {
    type Item = &'read Value;
    type IntoIter = Box<dyn Iterator<Item = Self::Item> + 'read>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.0.iter().map(|(_, value)| value))
    }
}

#[derive(Default)]
pub(super) struct Observation {
    iterators: AtomicUsize,
    mappings: AtomicUsize,
    dropped: AtomicBool,
}

impl Observation {
    pub(super) fn iterator_count(&self) -> usize {
        self.iterators.load(Ordering::Relaxed)
    }

    pub(super) fn mapping_count(&self) -> usize {
        self.mappings.load(Ordering::Relaxed)
    }

    pub(super) fn was_dropped(&self) -> bool {
        self.dropped.load(Ordering::Relaxed)
    }
}

pub(super) struct ObservedSource {
    observation: Arc<Observation>,
}

impl ObservedSource {
    pub(super) fn new() -> Self {
        Self {
            observation: Arc::default(),
        }
    }

    pub(super) fn observation(&self) -> Arc<Observation> {
        Arc::clone(&self.observation)
    }

    pub(super) fn len(&self) -> usize {
        4
    }

    pub(super) fn map(item: ObservedItem<'_>) -> Value {
        item.observation.mappings.fetch_add(1, Ordering::Relaxed);
        ::serde_json::json!({"problem": item.index})
    }
}

pub(super) struct ObservedItem<'read> {
    index: usize,
    observation: &'read Observation,
}

impl<'read> IntoIterator for &'read ObservedSource {
    type Item = ObservedItem<'read>;
    type IntoIter = Box<dyn Iterator<Item = Self::Item> + 'read>;

    fn into_iter(self) -> Self::IntoIter {
        self.observation.iterators.fetch_add(1, Ordering::Relaxed);
        Box::new((0..self.len()).map(|index| ObservedItem {
            index,
            observation: &self.observation,
        }))
    }
}

impl Drop for ObservedSource {
    fn drop(&mut self) {
        self.observation.dropped.store(true, Ordering::Relaxed);
    }
}

/// Real schema borrow with a destructor that observes schema data on teardown.
/// Uses the small synthetic SchemaProject fixture, not the full builtin catalog.
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

    pub(super) fn map(column: &ColumnDefinition) -> Value {
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
