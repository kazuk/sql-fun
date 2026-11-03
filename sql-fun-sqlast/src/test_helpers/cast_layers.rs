use crate::context::ParseContext as _;
use crate::test_helpers::schema_layer;

use crate::context::SchemaFileContext;
use crate::names::FullName;
use crate::sem::{CastContext, CastDefinition, TypeReference};
use crate::test_helpers::TestParseContext;
use std::path::Path;

pub(crate) fn named_type(name: &str) -> TypeReference {
    TypeReference::from_full_name(FullName::with_schema("fixture", name))
}

pub(crate) fn cast_layer() -> (
    SchemaFileContext,
    TypeReference,
    TypeReference,
    TypeReference,
) {
    let root = TestParseContext::default().freeze();
    let source = named_type("source");
    let shared = named_type("shared");
    let inherited = named_type("inherited");
    let mut parent = schema_layer(Path::new("parent.sql"), &root);
    register(&mut parent, &source, &shared, CastContext::implicit());
    register(&mut parent, &source, &inherited, CastContext::implicit());
    let child = schema_layer(Path::new("child.sql"), &parent.freeze());
    (child, source, shared, inherited)
}

pub(crate) fn register(
    context: &mut SchemaFileContext,
    source: &TypeReference,
    target: &TypeReference,
    cast_context: CastContext,
) {
    context.set_test_cast(source, target, CastDefinition::new(cast_context));
}
