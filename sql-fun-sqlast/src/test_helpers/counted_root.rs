use crate::context::{FrozenBaseContext, ParseContext, TypeInfoRead};
use crate::names::FullName;
use crate::sem::{TypeDefinition, TypeReference};
use crate::test_helpers::TestParseContext;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

#[derive(Debug)]
struct CountingRoot {
    base: FrozenBaseContext,
    probes: Arc<AtomicUsize>,
}

impl CountingRoot {
    fn base(&self) -> &FrozenBaseContext {
        &self.base
    }
}

crate::context::forward_catalog_reads!(CountingRoot, base);

impl TypeInfoRead for CountingRoot {
    fn type_references(&self) -> Vec<TypeReference> {
        self.base.type_references()
    }

    fn builtin_type_ordered(&self, name: &FullName) -> Option<bool> {
        self.base.builtin_type_ordered(name)
    }

    fn get_type_impl(&self, name: &FullName) -> Option<&TypeDefinition> {
        assert_eq!(
            self.probes.fetch_add(1, Ordering::Relaxed),
            0,
            "a qualified lookup must not probe the root twice"
        );
        self.base.get_type_impl(name)
    }

    fn get_array_item_type(&self, name: &TypeReference) -> Option<TypeReference> {
        self.base.get_array_item_type(name)
    }
}

pub(crate) fn counted_root() -> (FrozenBaseContext, Arc<AtomicUsize>) {
    let probes = Arc::new(AtomicUsize::new(0));
    let root = CountingRoot {
        base: TestParseContext::default().freeze(),
        probes: Arc::clone(&probes),
    };
    (FrozenBaseContext::new(root), probes)
}
