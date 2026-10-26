//! Catalog identity across quoted component boundaries and frozen layers.
use crate::sem::{FullName, ParseContext};

#[rstest::rstest]
#[case::tables("table")]
#[case::views("view")]
#[case::sequences("sequence")]
#[case::types("type")]
#[case::functions("function")]
fn distinct_names_survive_exact_lookup_and_layering(
    #[case] kind: &str,
    #[values(false, true)] inherited: bool,
) -> testresult::TestResult {
    let left = FullName::with_schema("a.b", "c");
    let right = FullName::with_schema("a", "b.c");
    let mut context = test_helpers::context();
    context = test_helpers::declare(context, kind, &left, "left_marker")?;
    if inherited {
        context =
            crate::test_helpers::schema_layer(std::path::Path::new("child.sql"), &context.freeze());
    }
    context = test_helpers::declare(context, kind, &right, "right_marker")?;
    test_helpers::assert_definition(&context, kind, &left, "left_marker");
    test_helpers::assert_definition(&context, kind, &right, "right_marker");
    let frozen = context.freeze();
    test_helpers::assert_definition(&frozen, kind, &left, "left_marker");
    test_helpers::assert_definition(&frozen, kind, &right, "right_marker");
    Ok(())
}

#[test]
fn duplicate_child_preserves_both_qualified_parent_tables() -> testresult::TestResult {
    let left = FullName::with_schema("a.b", "c");
    let right = FullName::with_schema("a", "b.c");
    let mut context = test_helpers::context();
    context = test_helpers::declare(context, "table", &left, "parent_marker")?;
    context = test_helpers::declare(context, "table", &right, "other_marker")?;
    let parent = context.freeze();
    let child = crate::test_helpers::schema_layer(std::path::Path::new("child.sql"), &parent);
    let child = test_helpers::declare(child, "table", &left, "child_marker")?;
    assert_eq!(child.problems().len(), 1);
    let child = child.freeze();
    test_helpers::assert_definition(&child, "table", &left, "parent_marker");
    test_helpers::assert_definition(&child, "table", &right, "other_marker");
    test_helpers::assert_definition(&parent, "table", &left, "parent_marker");
    Ok(())
}

#[test]
fn comment_targets_preserve_quoted_names_and_function_signatures() -> testresult::TestResult {
    let mut context = test_helpers::context();
    for sql in [
        r#"COMMENT ON TABLE "a.b".c IS 'left'"#,
        r#"COMMENT ON TABLE a."b.c" IS 'right'"#,
        r#"COMMENT ON FUNCTION a.f(integer) IS 'integer overload'"#,
        r#"COMMENT ON FUNCTION a.f(text) IS 'text overload'"#,
    ] {
        context = test_helpers::apply_sql(context, sql)?;
    }
    assert_eq!(context.catalog.comments.iter().count(), 4);
    assert_eq!(
        context
            .catalog
            .comments
            .get(r#""a.b"."c""#)
            .expect("left comment")
            .1,
        "left"
    );
    assert_eq!(
        context
            .catalog
            .comments
            .get(r#""a"."b.c""#)
            .expect("right comment")
            .1,
        "right"
    );
    Ok(())
}

mod test_helpers {
    use crate::ast::{Dialect, StatementNode, TryFromWithTokens};
    use crate::context::BaseParseContext;
    use crate::test_helpers::{SynBuilder, TestParseContext};
    use crate::{
        context::MutSourcePathWithSpan,
        sem::{
            AnalysisError, ColumnName, FullName, ParseContext, PgBuiltInType, SchemaFileContext,
            SchemaName,
        },
    };

    pub(super) fn context() -> SchemaFileContext {
        let mut base = TestParseContext::default();
        base.set_search_path(&[SchemaName::from("public")]);
        base.setup_type(PgBuiltInType::int4());
        let mut context =
            crate::test_helpers::schema_layer(std::path::Path::new("names.sql"), &base.freeze());
        context.set_source_path_with_span(&crate::test_helpers::source_span());
        context
    }

    pub(super) fn declare(
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

    pub(super) fn apply_sql(
        mut context: SchemaFileContext,
        sql: &str,
    ) -> Result<SchemaFileContext, AnalysisError> {
        context.set_source_path_with_span(&crate::test_helpers::source_span());
        let statement = StatementNode::try_from_with_tokens(
            &SynBuilder::new().parse(sql),
            &::sql_fun_core::IVec::default(),
        )?;
        Ok(statement.resolve_and_apply(context)?.into_context())
    }

    pub(super) fn assert_definition(
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
}
