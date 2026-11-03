use crate::context::MutSourcePathWithSpan as _;

use crate::names::{ColumnName, SchemaName};
use crate::sem::{ColumnDefinition, PgBuiltInType};
use crate::test_helpers::{TestParseContext, source_span};
pub(crate) fn catalog(version: u32) -> TestParseContext {
    let metadata = ::serde_json::from_value(::serde_json::json!({"engine":"postgres", "dialect":"postgres", "version":version.to_string(), "search_path":["public"]})).unwrap();
    let mut context = TestParseContext::default().with_metadata(&metadata);
    context.set_source_path_with_span(&source_span());
    context
        .set_get_search_path_result(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    context.setup_type(PgBuiltInType::int4());
    context.setup_type(PgBuiltInType::text());
    context.setup_type(PgBuiltInType::bool());
    context.setup_binary("=", "int4", "int4", "bool", true);
    for (name, id, label) in [("items", "id", "label"), ("incoming", "sid", "slabel")] {
        context.setup_table(
            name,
            &[
                ColumnDefinition::new(
                    &Some(ColumnName::from(id)),
                    Some(&PgBuiltInType::int4()),
                    Some(false),
                    &[],
                ),
                ColumnDefinition::new(
                    &Some(ColumnName::from(label)),
                    Some(&PgBuiltInType::text()),
                    Some(false),
                    &[],
                ),
            ],
            &source_span(),
        );
    }
    context
}
