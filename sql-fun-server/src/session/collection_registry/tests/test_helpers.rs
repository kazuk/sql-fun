use crate::session::collection_registry::CollectionRegistry;
use std::{
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use ::serde_json::Value;
use ::sql_fun_sqlast::TrieMap;

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
    items: Vec<usize>,
    observation: Arc<Observation>,
}

impl ObservedSource {
    pub(super) fn new() -> Self {
        Self {
            items: vec![0, 1, 2, 3],
            observation: Arc::default(),
        }
    }

    pub(super) fn observation(&self) -> Arc<Observation> {
        Arc::clone(&self.observation)
    }

    pub(super) fn len(&self) -> usize {
        self.items.len()
    }

    pub(super) fn map(item: ObservedItem<'_>) -> Value {
        item.observation.mappings.fetch_add(1, Ordering::Relaxed);
        ::serde_json::json!({"problem": item.index})
    }
}

#[derive(Clone, Copy)]
pub(super) struct ObservedItem<'read> {
    index: usize,
    observation: &'read Observation,
}

impl<'read> IntoIterator for &'read ObservedSource {
    type Item = ObservedItem<'read>;
    type IntoIter = Box<dyn Iterator<Item = Self::Item> + 'read>;

    fn into_iter(self) -> Self::IntoIter {
        self.observation.iterators.fetch_add(1, Ordering::Relaxed);
        Box::new(self.items.iter().map(|index| ObservedItem {
            index: *index,
            observation: &self.observation,
        }))
    }
}

impl Drop for ObservedSource {
    fn drop(&mut self) {
        self.observation.dropped.store(true, Ordering::Relaxed);
    }
}
