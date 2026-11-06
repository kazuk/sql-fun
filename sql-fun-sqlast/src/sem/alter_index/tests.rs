//! Enduring parser-to-semantics and schema-chain coverage without database dependencies.

use crate::{
    context::IndexInfoRead,
    names::{FullName, SchemaName, TableName},
    sem::{Kind, ParseContext, problem::IndexProblemKind},
};

#[rstest::rstest]
#[case::fillfactor("SET (fillfactor=75)", Some(75), None, None)]
#[case::fillfactor_min("SET (fillfactor=10)", Some(10), None, None)]
#[case::fillfactor_max("SET (fillfactor=100)", Some(100), None, None)]
#[case::rounded("SET (fillfactor=75.5)", Some(76), None, None)]
#[case::hex("SET (fillfactor='0x4b')", Some(75), None, None)]
#[case::binary("SET (fillfactor='0b1001011')", Some(75), None, None)]
#[case::rounded_even("SET (fillfactor=74.5)", Some(74), None, None)]
#[case::octal("SET (fillfactor='075')", Some(61), None, None)]
#[case::deduplicate("SET (deduplicate_items=off)", None, Some(false), None)]
#[case::bare_boolean("SET (deduplicate_items)", None, Some(true), None)]
#[case::boolean_prefix("SET (deduplicate_items=of)", None, Some(false), None)]
#[case::scale("SET (vacuum_cleanup_index_scale_factor=0.2)", None, None, Some(0.2))]
#[case::hex_scale(
    "SET (vacuum_cleanup_index_scale_factor='0x1p-1')",
    None,
    None,
    Some(0.5)
)]
#[case::exact_hex_subnormal(
    "SET (vacuum_cleanup_index_scale_factor='0x1p-1074')",
    None,
    None,
    Some(f64::from_bits(1))
)]
#[case::scale_max(
    "SET (vacuum_cleanup_index_scale_factor=10000000000)",
    None,
    None,
    Some(1e10)
)]
#[case::unknown_reset("RESET (unknown_option)", None, None, None)]
#[case::toast("SET (toast.fillfactor=75)", None, None, None)]
fn applies_storage_parameters(
    #[case] command: &str,
    #[case] fillfactor: Option<i32>,
    #[case] deduplicate: Option<bool>,
    #[case] scale: Option<f64>,
) -> testresult::TestResult {
    let result = test_helpers::alter(test_helpers::catalog()?, command)?;
    let statement = result.ast().as_alter_index().expect("ALTER INDEX");
    assert!(statement.is_applicable(), "{:?}", statement.problems());
    assert!(!statement.is_skipped());
    assert_eq!(
        statement.target().requested(),
        &FullName::from_local_name("idx")
    );
    assert_eq!(
        statement.target().index_name(),
        Some(&test_helpers::index_name())
    );
    assert_eq!(statement.target().kind(), Some(Kind::index()));
    let change = statement.operations()[0]
        .storage_change()
        .expect("storage operation");
    assert_eq!(change.parameters().len(), 1);
    let index = test_helpers::index(result.context());
    assert_eq!(index.access_method(), "btree");
    assert_eq!(test_helpers::fillfactor(result.context()), fillfactor);
    assert_eq!(test_helpers::deduplicate(index), deduplicate);
    assert_eq!(test_helpers::scale(index), scale);
    assert!(result.context().problems().is_empty());
    Ok(())
}

#[rstest::rstest]
#[case::low_fillfactor("SET (fillfactor=9)", IndexProblemKind::invalid_value())]
#[case::high_fillfactor("SET (fillfactor=101)", IndexProblemKind::invalid_value())]
#[case::no_integer("SET (fillfactor)", IndexProblemKind::invalid_value())]
#[case::wrong_integer("SET (fillfactor=abc)", IndexProblemKind::invalid_value())]
#[case::wrong_boolean("SET (deduplicate_items=o)", IndexProblemKind::invalid_value())]
#[case::array_boolean("SET (deduplicate_items=off[])", IndexProblemKind::invalid_value())]
#[case::boolean_whitespace("SET (deduplicate_items=' true ')", IndexProblemKind::invalid_value())]
#[case::negative_scale(
    "SET (vacuum_cleanup_index_scale_factor=-1)",
    IndexProblemKind::invalid_value()
)]
#[case::high_scale(
    "SET (vacuum_cleanup_index_scale_factor=10000000001)",
    IndexProblemKind::invalid_value()
)]
#[case::underflow_scale(
    "SET (vacuum_cleanup_index_scale_factor='1e-9999')",
    IndexProblemKind::invalid_value()
)]
#[case::invalid_octal("SET (fillfactor='078')", IndexProblemKind::invalid_value())]
#[case::nan_scale(
    "SET (vacuum_cleanup_index_scale_factor='NaN')",
    IndexProblemKind::invalid_value()
)]
#[case::infinite_scale(
    "SET (vacuum_cleanup_index_scale_factor='Infinity')",
    IndexProblemKind::invalid_value()
)]
#[case::unknown("SET (fastupdate=on)", IndexProblemKind::unknown_parameter())]
#[case::namespace("SET (xyz.fillfactor=70)", IndexProblemKind::invalid_namespace())]
#[case::duplicate(
    "SET (fillfactor=70,fillfactor=80)",
    IndexProblemKind::duplicate_parameter()
)]
#[case::later_invalid(
    "SET (fillfactor=70), SET (deduplicate_items=o)",
    IndexProblemKind::invalid_value()
)]
#[case::normal_column(
    "ALTER COLUMN 1 SET STATISTICS 100",
    IndexProblemKind::non_expression_column()
)]
#[case::included(
    "ALTER COLUMN 3 SET STATISTICS 100",
    IndexProblemKind::included_column()
)]
#[case::too_many(
    "ALTER COLUMN 4 SET STATISTICS 100",
    IndexProblemKind::invalid_column()
)]
#[case::negative_target(
    "ALTER COLUMN 2 SET STATISTICS -2",
    IndexProblemKind::statistics_too_low()
)]
#[case::atomic_statistics(
    "SET (fillfactor=70), ALTER COLUMN 1 SET STATISTICS 100",
    IndexProblemKind::non_expression_column()
)]
fn rejects_without_changing_the_catalog(
    #[case] command: &str,
    #[case] kind: IndexProblemKind,
) -> testresult::TestResult {
    let catalog = test_helpers::catalog()?;
    let result = test_helpers::alter(catalog, command)?;
    let statement = result.ast().as_alter_index().expect("retained ALTER INDEX");
    assert!(!statement.is_applicable());
    assert!(!statement.is_skipped());
    assert_eq!(
        statement.target().index_name(),
        Some(&test_helpers::index_name())
    );
    assert_eq!(statement.problems().len(), 1);
    assert_eq!(statement.problems()[0].kind(), kind);
    assert!(!statement.problems()[0].is_warning());
    assert!(!statement.operations().is_empty());
    assert_eq!(result.context().problems().len(), 1);
    assert_eq!(
        result.context().problems()[0]
            .index_problem()
            .expect("structured problem")
            .kind(),
        kind
    );
    let index = test_helpers::index(result.context());
    assert!(index.storage_parameter("fillfactor").is_none());
    assert!(index.statistics_target(2).is_none());
    let continued = test_helpers::alter(result.into_context(), "SET (fillfactor=85)")?;
    assert!(continued.ast().as_alter_index().unwrap().is_applicable());
    assert_eq!(test_helpers::fillfactor(continued.context()), Some(85));
    Ok(())
}

#[rstest::rstest]
#[case::zero(0, Some(0), false)]
#[case::normal(100, Some(100), false)]
#[case::maximum(10000, Some(10000), false)]
#[case::clamped(10001, Some(10000), true)]
#[case::reset(-1, None, false)]
fn statistics_retain_requested_and_effective_values(
    #[case] requested: i32,
    #[case] expected: Option<i32>,
    #[case] warned: bool,
) -> testresult::TestResult {
    let context = test_helpers::alter(
        test_helpers::catalog()?,
        "ALTER COLUMN 2 SET STATISTICS 250",
    )?
    .into_context();
    let result = test_helpers::alter(
        context,
        &format!("ALTER COLUMN 2 SET STATISTICS {requested}"),
    )?;
    let statement = result.ast().as_alter_index().unwrap();
    assert!(statement.is_applicable());
    let statistics = statement.operations()[0].statistics_change().unwrap();
    assert_eq!(statistics.column(), 2);
    assert_eq!(statistics.requested_target(), requested);
    assert_eq!(statistics.effective_target(), Some(requested.min(10000)));
    assert_eq!(
        test_helpers::index(result.context()).statistics_target(2),
        expected
    );
    assert_eq!(result.context().problems().len(), usize::from(warned));
    if warned {
        let problem = result.context().problems()[0].index_problem().unwrap();
        assert!(problem.is_warning());
        assert_eq!(problem.kind(), IndexProblemKind::statistics_clamped());
        assert_eq!(problem.column(), Some(2));
        assert_eq!(problem.value(), Some("10001"));
    }
    Ok(())
}

#[rstest::rstest]
#[case::known("RESET (fillfactor=70)")]
#[case::unknown("RESET (unknown=70)")]
#[case::namespace("RESET (xyz.fillfactor=70)")]
#[case::toast("RESET (toast.fillfactor=70)")]
#[case::atomic("RESET (fillfactor), RESET (deduplicate_items=on)")]
fn reset_values_retain_failure_and_leave_existing_settings_unchanged(
    #[case] command: &str,
) -> testresult::TestResult {
    let original = test_helpers::alter(
        test_helpers::catalog()?,
        "SET (fillfactor=65, deduplicate_items=off)",
    )?;
    let result = test_helpers::alter(original.into_context(), command)?;
    let statement = result.ast().as_alter_index().expect("retained ALTER INDEX");
    assert!(!statement.is_applicable());
    assert_eq!(statement.problems().len(), 1);
    let problem = &statement.problems()[0];
    assert_eq!(problem.kind(), IndexProblemKind::reset_value());
    assert!(!problem.is_warning());
    assert!(problem.value().is_some());
    assert_eq!(result.context().problems().len(), 1);
    assert_eq!(test_helpers::fillfactor(result.context()), Some(65));
    assert_eq!(
        test_helpers::deduplicate(test_helpers::index(result.context())),
        Some(false)
    );
    let continued = test_helpers::alter(result.into_context(), "SET (fillfactor=85)")?;
    assert!(continued.ast().as_alter_index().unwrap().is_applicable());
    assert_eq!(test_helpers::fillfactor(continued.context()), Some(85));
    Ok(())
}

#[rstest::rstest]
#[case::unknown_namespace("RESET (xyz.fillfactor)")]
#[case::toast("RESET (toast.fillfactor)")]
#[case::unknown_name("RESET (xyz.unknown)")]
fn namespaced_resets_preserve_existing_settings(#[case] command: &str) -> testresult::TestResult {
    let original = test_helpers::alter(test_helpers::catalog()?, "SET (fillfactor=65)")?;
    let result = test_helpers::alter(original.into_context(), command)?;
    let statement = result.ast().as_alter_index().expect("retained ALTER INDEX");
    assert!(statement.is_applicable());
    assert!(statement.problems().is_empty());
    assert!(result.context().problems().is_empty());
    let change = statement.operations()[0].storage_change().unwrap();
    assert!(change.is_reset());
    assert!(change.parameters()[0].namespace().is_some());
    assert_eq!(test_helpers::fillfactor(result.context()), Some(65));
    Ok(())
}

#[test]
fn reset_and_repeated_changes_preserve_unrelated_settings() -> testresult::TestResult {
    let result = test_helpers::alter(
        test_helpers::catalog()?,
        "SET (fillfactor=70, deduplicate_items=off), SET (fillfactor=80), ALTER COLUMN 2 SET STATISTICS 25",
    )?;
    assert!(result.ast().as_alter_index().unwrap().is_applicable());
    let result = test_helpers::alter(result.into_context(), "RESET (fillfactor, fillfactor)")?;
    assert!(
        result.ast().as_alter_index().unwrap().operations()[0]
            .storage_change()
            .unwrap()
            .is_reset()
    );
    assert_eq!(test_helpers::fillfactor(result.context()), None);
    assert_eq!(
        test_helpers::deduplicate(test_helpers::index(result.context())),
        Some(false)
    );
    assert_eq!(
        test_helpers::index(result.context()).statistics_target(2),
        Some(25)
    );
    assert_eq!(
        result
            .context()
            .indexes_for_table(&TableName::from(FullName::with_schema("public", "items")))
            .len(),
        1
    );
    Ok(())
}

#[rstest::rstest]
#[case::missing("ALTER INDEX absent SET (fillfactor=70)", true, false)]
#[case::skip_missing("ALTER INDEX IF EXISTS absent SET (fillfactor=70)", true, true)]
#[case::wrong_kind("ALTER INDEX items SET (fillfactor=70)", false, false)]
#[case::wrong_kind_if_exists("ALTER INDEX IF EXISTS items SET (fillfactor=70)", false, true)]
fn retains_target_failure_and_skip_outcomes(
    #[case] sql: &str,
    #[case] missing: bool,
    #[case] if_exists: bool,
) -> testresult::TestResult {
    let result = test_helpers::statement(test_helpers::catalog()?, sql)?;
    let statement = result.ast().as_alter_index().unwrap();
    assert_eq!(statement.target().is_missing(), missing);
    assert_eq!(statement.if_exists(), if_exists);
    assert_eq!(statement.is_skipped(), missing && if_exists);
    assert!(!statement.is_applicable());
    assert!(statement.target().index_name().is_none());
    assert_eq!(
        statement.target().kind(),
        if missing { None } else { Some(Kind::table()) }
    );
    assert_eq!(statement.operations().len(), 1);
    assert_eq!(
        result.context().problems().len(),
        usize::from(!(missing && if_exists))
    );
    assert_eq!(test_helpers::fillfactor(result.context()), None);
    if !missing {
        assert_eq!(
            statement.target().relation_name(),
            Some(&FullName::with_schema("public", "items"))
        );
        assert_eq!(
            statement.problems()[0].kind(),
            IndexProblemKind::wrong_kind()
        );
    } else if !if_exists {
        assert_eq!(
            statement.problems()[0].kind(),
            IndexProblemKind::missing_target()
        );
    }
    assert!(
        test_helpers::alter(result.into_context(), "SET (fillfactor=80)")?
            .ast()
            .as_alter_index()
            .unwrap()
            .is_applicable()
    );
    Ok(())
}

#[test]
fn respects_wrong_kind_search_path_shadowing_and_qualification() -> testresult::TestResult {
    let context = test_helpers::seed(
        "CREATE TABLE a.idx(id integer); CREATE TABLE b.items(id integer); CREATE INDEX idx ON b.items(id)",
    )?;
    let mut context = context;
    context.set_search_path(&[SchemaName::from("a"), SchemaName::from("b")]);
    let result = test_helpers::statement(context, "ALTER INDEX IF EXISTS idx SET (fillfactor=70)")?;
    let target = result.ast().as_alter_index().unwrap().target();
    assert_eq!(target.kind(), Some(Kind::table()));
    assert_eq!(
        target.relation_name(),
        Some(&FullName::with_schema("a", "idx"))
    );
    assert!(
        result
            .context()
            .get_index_impl(&FullName::with_schema("b", "idx"))
            .unwrap()
            .storage_parameter("fillfactor")
            .is_none()
    );
    let result = test_helpers::statement(
        result.into_context(),
        "ALTER INDEX b.idx SET (fillfactor=80)",
    )?;
    assert_eq!(
        result.ast().as_alter_index().unwrap().target().index_name(),
        Some(&FullName::with_schema("b", "idx"))
    );
    assert!(result.ast().as_alter_index().unwrap().is_applicable());
    Ok(())
}

#[test]
fn quoted_identity_and_catalog_layers_preserve_parent_and_sibling() -> testresult::TestResult {
    let context = test_helpers::seed(
        r#"CREATE TABLE "a.b".items(id integer); CREATE INDEX "Mixed.Index" ON "a.b".items(id) WITH (fillfactor=65, deduplicate_items=off)"#,
    )?;
    let parent = context.freeze();
    let child = test_helpers::child(&parent);
    let result = test_helpers::statement(
        child,
        r#"ALTER INDEX "a.b"."Mixed.Index" SET (fillfactor=75)"#,
    )?;
    let name = FullName::with_schema("a.b", "Mixed.Index");
    assert_eq!(
        result.ast().as_alter_index().unwrap().target().index_name(),
        Some(&name)
    );
    assert_eq!(test_helpers::named_fillfactor(&parent, &name), Some(65));
    assert_eq!(
        test_helpers::named_fillfactor(result.context(), &name),
        Some(75)
    );
    assert_eq!(
        test_helpers::named_fillfactor(&test_helpers::child(&parent), &name),
        Some(65)
    );
    let frozen = result.into_context().freeze();
    assert_eq!(test_helpers::named_fillfactor(&frozen, &name), Some(75));
    let result = test_helpers::statement(
        test_helpers::child(&frozen),
        r#"ALTER INDEX "a.b"."Mixed.Index" SET (fillfactor=80), SET (fillfactor=2)"#,
    )?;
    assert!(!result.ast().as_alter_index().unwrap().is_applicable());
    assert_eq!(
        test_helpers::named_fillfactor(result.context(), &name),
        Some(75)
    );
    assert_eq!(test_helpers::named_fillfactor(&frozen, &name), Some(75));
    Ok(())
}

#[test]
fn create_settings_reach_the_existing_index_definition() -> testresult::TestResult {
    let context = test_helpers::seed(
        "CREATE TABLE items(id integer); CREATE INDEX idx ON items(id) WITH (fillfactor=70, deduplicate_items=off, vacuum_cleanup_index_scale_factor=0.5)",
    )?;
    let index = test_helpers::index(&context);
    assert_eq!(test_helpers::fillfactor(&context), Some(70));
    assert_eq!(test_helpers::deduplicate(index), Some(false));
    assert_eq!(test_helpers::scale(index), Some(0.5));
    let result = test_helpers::alter(context, "RESET (fillfactor)")?;
    assert_eq!(test_helpers::fillfactor(result.context()), None);
    Ok(())
}

#[test]
fn statistics_preserve_a_non_btree_access_method() -> testresult::TestResult {
    let context = test_helpers::seed(
        "CREATE TABLE items(id integer); CREATE INDEX idx ON items USING hash ((1))",
    )?;
    let result = test_helpers::alter(context, "ALTER COLUMN 1 SET STATISTICS 100")?;
    assert!(result.ast().as_alter_index().unwrap().is_applicable());
    assert_eq!(
        test_helpers::index(result.context()).access_method(),
        "hash"
    );
    assert_eq!(
        test_helpers::index(result.context()).statistics_target(1),
        Some(100)
    );
    Ok(())
}

use crate::test_helpers::alter_index as test_helpers;

#[rstest::rstest]
#[case::zero("ALTER INDEX idx ALTER COLUMN 0 SET STATISTICS 100")]
#[case::ordinal_limit("ALTER INDEX idx ALTER COLUMN 32768 SET STATISTICS 100")]
fn parser_rejects_column_numbers_outside_the_postgresql_syntax_range(#[case] sql: &str) {
    assert!(crate::test_helpers::ast_query::postgres_syntax_rejected(
        sql
    ));
}

#[test]
fn altered_unique_partial_index_remains_an_insert_conflict_arbiter() -> testresult::TestResult {
    let context = test_helpers::alter(
        test_helpers::catalog()?,
        "SET (fillfactor=75), ALTER COLUMN 2 SET STATISTICS 100",
    )?
    .into_context();
    let result = test_helpers::statement(
        context,
        "INSERT INTO items(a,b) VALUES (1,2) ON CONFLICT (a,(1)) WHERE a IS NOT NULL DO NOTHING",
    )?;
    assert!(
        result.context().problems().is_empty(),
        "{:?}",
        result.context().problems()
    );
    let insert = result.ast().as_insert().unwrap();
    let arbiters = insert.conflicts()[0]
        .arbiter()
        .arbiters()
        .expect("matching unique partial index");
    assert_eq!(arbiters.len(), 1);
    assert_eq!(arbiters[0].index_name(), Some(&test_helpers::index_name()));
    assert_eq!(test_helpers::fillfactor(result.context()), Some(75));
    // The included-column boundary also survives the replacement.
    let result = test_helpers::alter(result.into_context(), "ALTER COLUMN 3 SET STATISTICS 50")?;
    assert_eq!(
        result.ast().as_alter_index().unwrap().problems()[0].kind(),
        IndexProblemKind::included_column()
    );
    Ok(())
}

#[test]
fn retained_options_and_rendered_diagnostics_describe_the_rejected_statement()
-> testresult::TestResult {
    let result = test_helpers::alter(
        test_helpers::catalog()?,
        "SET (fillfactor=70,deduplicate_items=o)",
    )?;
    let statement = result.ast().as_alter_index().unwrap();
    let storage = statement.operations()[0].storage_change().unwrap();
    assert_eq!(storage.parameters()[0].name(), "fillfactor");
    assert_eq!(storage.parameters()[0].value(), Some("70"));
    assert_eq!(storage.parameters()[0].namespace(), None);
    let problem = &statement.problems()[0];
    assert_eq!(
        problem.name().as_str(),
        test_helpers::index_name().to_string()
    );
    assert_eq!(problem.parameter_name(), Some("deduplicate_items"));
    assert_eq!(problem.value(), Some("o"));
    assert_eq!(problem.column(), None);
    let metadata = result.context().problems()[0]
        .render_metadata()
        .expect("index diagnostic metadata");
    assert_eq!(
        result.context().problems()[0].code(),
        "index_invalid_alteration"
    );
    assert!(metadata.message().contains("deduplicate_items"));
    assert_eq!(test_helpers::fillfactor(result.context()), None);
    Ok(())
}
