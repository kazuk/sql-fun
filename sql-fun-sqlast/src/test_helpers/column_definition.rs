use crate::ast::Dialect;

use crate::names::{FullName, SchemaName};
use crate::sem::TypeReference;
use crate::test_helpers::TestParseContext;

pub(crate) fn setup_context() -> TestParseContext {
    let mut context = TestParseContext::default().with_dialect(Dialect::sqlite());
    context.set_get_search_path_result(&[SchemaName::from("public")]);
    context.setup_type(TypeReference::concrete_type_ref(
        &FullName::with_schema("public", "integer"),
        false,
    ));
    context.setup_type(TypeReference::concrete_type_ref(
        &FullName::with_schema("public", "text"),
        false,
    ));
    context
}
