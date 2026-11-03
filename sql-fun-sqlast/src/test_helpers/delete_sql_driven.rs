use crate::ast::Dialect;
use crate::context::MutSourcePathWithSpan as _;

use crate::names::SchemaName;
use crate::sem::PgBuiltInType;
use crate::test_helpers::{TestParseContext, source_span};

pub(crate) fn context() -> TestParseContext {
    let mut context = TestParseContext::default().with_dialect(Dialect::postgres());
    context.set_source_path_with_span(&source_span());
    context
        .set_get_search_path_result(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    context.setup_type(PgBuiltInType::int4());
    context.setup_type(PgBuiltInType::text());
    context.setup_type(PgBuiltInType::bool());
    context.setup_binary("=", "int4", "int4", "bool", true);
    context
}
