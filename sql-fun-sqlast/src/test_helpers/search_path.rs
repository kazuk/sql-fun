use crate::context::{FrozenBaseContext, ParseContext as _};
use crate::names::{FullName, SchemaName};
use crate::sem::{FunctionOverloadCollection, OverloadVariant, TypeDefinition, TypeReference};
use crate::test_helpers::TestParseContext;

pub(crate) fn search_path(schemas: &[&str]) -> Vec<SchemaName> {
    let mut names = Vec::new();
    for schema in schemas {
        names.push(SchemaName::from(schema));
    }
    names
}

pub(crate) fn catalog_with_search_path(schemas: &[&str]) -> FrozenBaseContext {
    let mut context = TestParseContext::default();
    context.set_search_path(&search_path(schemas));
    for schema in ["pg_catalog", "public", "app"] {
        let name = FullName::with_schema(schema, "item");
        context.set_get_type_impl_result(&name, &TypeDefinition::builtin(&name, None));
        let overload = OverloadVariant::new(
            &Some(TypeReference::from_full_name(name.clone())),
            &Vec::new(),
            true,
            true,
        );
        context.set_get_function_impl_result(&name, &FunctionOverloadCollection::new(&[overload]));
    }
    context.freeze()
}
