use crate::AstAndContextPair;
use crate::context::{BaseParseContext as _, MutSourcePathWithSpan as _};

use crate::names::{ColumnName, SchemaName};
use crate::sem::{AnalysisError, ColumnDefinition, PgBuiltInType, SelectStatement, SemAst};
#[cfg(feature = "sqlite")]
use crate::test_helpers::{SqliteSynBuilder, TestParseContext, source_span};

fn selected_context() -> TestParseContext {
    let mut metadata = ::sql_fun_core::SqlFunMetadataCollection::load_from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/selected-sqlite.toml"
    ))
    .expect("metadata fixture");
    let selected = metadata.remove("analytics").expect("selected datasource");
    let mut context = TestParseContext::default().with_metadata(&selected);
    context.set_source_path_with_span(&source_span());
    context.set_get_search_path_result(&[SchemaName::from("public")]);
    context.setup_type(PgBuiltInType::int4());
    context.setup_type(PgBuiltInType::text());
    context.setup_type(PgBuiltInType::bool());
    for (table, column, ty) in [
        ("a", "ax", PgBuiltInType::int4()),
        ("b", "bx", PgBuiltInType::text()),
        ("c", "cx", PgBuiltInType::text()),
    ] {
        context.setup_table(
            table,
            &[
                ColumnDefinition::new(&Some(ColumnName::from(column)), Some(&ty), Some(true), &[]),
                ColumnDefinition::new(&Some(ColumnName::from("id")), Some(&ty), Some(true), &[]),
            ],
            &source_span(),
        );
    }
    context
}

pub(crate) fn analyze(sql: &str) -> Result<(SelectStatement, TestParseContext), AnalysisError> {
    let context = selected_context();
    let statement = SqliteSynBuilder.parse_as_stmt(sql);
    let AstAndContextPair(ast, context) = SemAst::semantic_analysis_sqlite_stmt(
        context,
        statement,
        &::sql_fun_core::IVec::default(),
    )?;
    assert_eq!(
        context.metadata().connector().as_deref(),
        Some("selected-connector")
    );
    Ok((ast.into_select_statemtnt().expect("SELECT"), context))
}
