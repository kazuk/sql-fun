//! Catalog identity across quoted component boundaries and frozen layers.

use crate::{context::ParseContext, names::FullName, test_helpers::schema_layer};

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
        context = schema_layer(std::path::Path::new("child.sql"), &context.freeze());
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
    let child = schema_layer(std::path::Path::new("child.sql"), &parent);
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

use crate::test_helpers::name_identity as test_helpers;
