use crate::ast::TryFromWithTokens;
use crate::sem::SemAst;
use crate::test_helpers::{create_cache_context, source_span};
use crate::{
    context::{MutSourcePathWithSpan, ParseContext},
    test_helpers::schema_layer,
};

use crate::ast::{Dialect, StatementNode};
use crate::context::{BaseParseContext, SchemaFileContext};
use crate::names::{ColumnName, FullName, SchemaName};
use crate::sem::{AnalysisError, PgBuiltInType};
use crate::test_helpers::{SynBuilder, TestParseContext};

pub(crate) fn context() -> SchemaFileContext {
    let mut base = TestParseContext::default();
    base.set_search_path(&[SchemaName::from("public")]);
    base.setup_type(PgBuiltInType::int4());
    let mut context = schema_layer(std::path::Path::new("names.sql"), &base.freeze());
    context.set_source_path_with_span(&source_span());
    context
}

pub(crate) fn declare(
    context: SchemaFileContext,
    kind: &str,
    name: &FullName,
    marker: &str,
) -> Result<SchemaFileContext, AnalysisError> {
    let spelling = name
        .to_sql(Dialect::postgres())
        .expect("legal fixture name");
    let sql = match kind {
        "table" => format!("CREATE TABLE {spelling} ({marker} integer)"),
        "view" => format!("CREATE VIEW {spelling} AS SELECT 1 AS {marker}"),
        "sequence" => format!("CREATE SEQUENCE {spelling}"),
        "type" => format!("CREATE TYPE {spelling} AS ENUM ('{marker}')"),
        "function" => {
            format!("CREATE FUNCTION {spelling}() RETURNS integer LANGUAGE sql AS 'SELECT 1'")
        }
        _ => unreachable!("fixture kind"),
    };
    apply_sql(context, &sql)
}

pub(crate) fn apply_sql(
    mut context: SchemaFileContext,
    sql: &str,
) -> Result<SchemaFileContext, AnalysisError> {
    context.set_source_path_with_span(&source_span());
    let statement = StatementNode::try_from_with_tokens(
        &SynBuilder::new().parse(sql),
        &::sql_fun_core::IVec::default(),
    )?;
    Ok(SemAst::apply_ast_node(&statement, context, &mut create_cache_context())?.into_context())
}

pub(crate) fn assert_definition(
    context: &impl BaseParseContext,
    kind: &str,
    name: &FullName,
    marker: &str,
) {
    let column = ColumnName::from(marker);
    match kind {
        "table" => {
            let table = context.get_table_impl(name).expect("exact table");
            assert_eq!(table.name().full_name(), name);
            assert!(table.get_column_def(&column).is_some());
        }
        "view" => assert!(
            context
                .get_view_impl(name)
                .expect("exact view")
                .get_column_def(&column)
                .is_some()
        ),
        "sequence" => assert_eq!(
            context
                .get_sequence_impl(name)
                .expect("exact sequence")
                .name(),
            name
        ),
        "type" => assert_eq!(
            context.get_type_impl(name).expect("exact type").full_name(),
            Some(name)
        ),
        "function" => {
            let definitions = context.routine_definitions(name);
            assert_eq!(definitions.len(), 1);
            assert_eq!(definitions[0].name(), name);
            assert_eq!(
                context
                    .get_function_impl(name)
                    .expect("exact function")
                    .len(),
                1
            );
        }
        _ => unreachable!("fixture kind"),
    }
}
