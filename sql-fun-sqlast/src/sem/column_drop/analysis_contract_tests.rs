//! Public analysis API, dependency classification and catalog-purity contracts.

#[cfg(feature = "sqlite")]
use crate::test_helpers::alter_table;

use crate::{
    ast::Dialect,
    context::{
        BaseParseContext, GetSourcePathWithSpan, GetSourcePathWithSpanOpt, IndexInfoRead,
        ParseContext, ViewInfoRead,
    },
    names::{ColumnName, TableName},
    sem::{
        AlterTable, ColumnDropOutcome, DropDependencyDecision, DropDependencyKind, DropKnowledgeGap,
    },
    test_helpers::{column_attributes, column_rename, public_analysis},
};

#[rstest::rstest]
#[case::ordinary("ALTER TABLE t DROP COLUMN a", ColumnDropOutcome::Applicable)]
#[case::only("ALTER TABLE ONLY t DROP COLUMN a", ColumnDropOutcome::Applicable)]
#[case::missing("ALTER TABLE t DROP COLUMN missing", ColumnDropOutcome::Rejected)]
#[case::skipped_column(
    "ALTER TABLE t DROP COLUMN IF EXISTS missing",
    ColumnDropOutcome::Skipped
)]
#[case::missing_table("ALTER TABLE absent DROP COLUMN a", ColumnDropOutcome::Rejected)]
#[case::skipped_table(
    "ALTER TABLE IF EXISTS absent DROP COLUMN a",
    ColumnDropOutcome::Skipped
)]
fn outcomes_preserve_input_catalog(
    #[case] sql: &str,
    #[case] outcome: ColumnDropOutcome,
) -> testresult::TestResult {
    let parent = column_rename::schema(&["CREATE TABLE t(a integer, b integer)"])?;
    let frozen = parent.freeze();
    let context = column_rename::child(&frozen);
    let statement = public_analysis::statement(sql, Dialect::postgres());
    let result = AlterTable::analyze(&statement, context)?;
    let alter = result.ast().alter_table_statement().unwrap();
    let change = alter.commands()[0].column_drop().unwrap();
    assert_eq!(change.outcome(), outcome);
    assert_eq!(change.only(), sql.contains(" ONLY "));
    assert_eq!(change.relation_if_exists(), sql.contains("TABLE IF EXISTS"));
    assert_eq!(change.if_exists(), sql.contains("COLUMN IF EXISTS"));
    assert_eq!(alter.can_apply(), outcome != ColumnDropOutcome::Rejected);
    assert!(alter.requires_application_validation());
    assert_eq!(
        alter.get_source_path_with_span().span(),
        result.context().get_source_path_with_span().unwrap().span()
    );
    assert!(
        change
            .knowledge_gaps()
            .contains(&DropKnowledgeGap::SequenceOwnership)
    );
    assert!(
        change
            .knowledge_gaps()
            .contains(&DropKnowledgeGap::ApplicationClosure)
    );
    let name = TableName::from(column_rename::name("t"));
    for table in [
        result.context().get_table(&name).unwrap(),
        frozen.get_table(&name).unwrap(),
    ] {
        assert!(table.has_column(&ColumnName::from("a")));
        assert!(table.has_column(&ColumnName::from("b")));
        assert!(table.alter_statements().is_empty());
    }
    if sql.contains("absent") {
        assert!(alter.resolved_target().is_none());
    }
    if outcome == ColumnDropOutcome::Rejected {
        assert!(!change.problems().is_empty());
    }
    Ok(())
}

#[rstest::rstest]
#[case::duplicate("DROP COLUMN a, DROP COLUMN a", ColumnDropOutcome::Rejected)]
#[case::skip_duplicate("DROP COLUMN a, DROP COLUMN IF EXISTS a", ColumnDropOutcome::Skipped)]
#[case::two_columns("DROP COLUMN a, DROP COLUMN b", ColumnDropOutcome::Applicable)]
fn repeated_drops_use_private_staged_columns(
    #[case] commands: &str,
    #[case] second: ColumnDropOutcome,
) -> testresult::TestResult {
    let context = column_rename::schema(&["CREATE TABLE t(a integer,b integer)"])?;
    let statement =
        public_analysis::statement(&format!("ALTER TABLE t {commands}"), Dialect::postgres());
    let result = AlterTable::analyze(&statement, context)?;
    let alter = result.ast().alter_table_statement().unwrap();
    assert_eq!(
        alter.commands()[0].column_drop().unwrap().outcome(),
        ColumnDropOutcome::Applicable
    );
    assert_eq!(alter.commands()[1].column_drop().unwrap().outcome(), second);
    assert!(
        result
            .context()
            .get_table(&TableName::from(column_rename::name("t")))
            .unwrap()
            .has_column(&ColumnName::from("a"))
    );
    Ok(())
}

#[rstest::rstest]
#[case::restrict("RESTRICT", DropDependencyDecision::Reject)]
#[case::omitted("", DropDependencyDecision::Reject)]
#[case::cascade("CASCADE", DropDependencyDecision::Cascade)]
fn retained_view_dependencies_do_not_publish_cascades(
    #[case] policy: &str,
    #[case] decision: DropDependencyDecision,
) -> testresult::TestResult {
    let context = column_rename::schema(&[
        "CREATE TABLE t(a integer,b integer)",
        "CREATE VIEW v AS SELECT a FROM t",
        "CREATE VIEW unrelated AS SELECT b FROM t",
    ])?;
    let statement = public_analysis::statement(
        &format!("ALTER TABLE t DROP a {policy}"),
        Dialect::postgres(),
    );
    let result = AlterTable::analyze(&statement, context)?;
    let change = result.ast().alter_table_statement().unwrap().commands()[0]
        .column_drop()
        .unwrap();
    let dependencies = change.dependencies();
    assert_eq!(dependencies.len(), 1);
    assert_eq!(dependencies[0].kind(), DropDependencyKind::View);
    assert_eq!(dependencies[0].relation(), &column_rename::name("v"));
    assert_eq!(dependencies[0].decision(), decision);
    assert!(
        result
            .context()
            .get_view_impl(&column_rename::name("v"))
            .is_some()
    );
    assert!(
        result
            .context()
            .get_view_impl(&column_rename::name("unrelated"))
            .is_some()
    );
    Ok(())
}

#[test]
fn local_keys_indexes_and_cross_table_foreign_keys_have_distinct_decisions()
-> testresult::TestResult {
    let context = column_rename::schema(&[
        "CREATE TABLE t(a integer PRIMARY KEY,b integer)",
        "CREATE INDEX ix ON t(a,b)",
        "CREATE TABLE child(c integer REFERENCES t(a))",
    ])?;
    let statement = public_analysis::statement("ALTER TABLE t DROP a", Dialect::postgres());
    let result = AlterTable::analyze(&statement, context)?;
    let change = result.ast().alter_table_statement().unwrap().commands()[0]
        .column_drop()
        .unwrap();
    assert!(
        change
            .dependencies()
            .iter()
            .any(|dep| dep.kind() == DropDependencyKind::Index
                && dep.decision() == DropDependencyDecision::Automatic)
    );
    assert!(
        change
            .dependencies()
            .iter()
            .any(|dep| dep.relation() == &column_rename::name("child")
                && dep.decision() == DropDependencyDecision::Reject)
    );
    assert_eq!(change.outcome(), ColumnDropOutcome::Rejected);
    assert!(
        result
            .context()
            .get_index_impl(&column_rename::name("ix"))
            .is_some()
    );
    Ok(())
}

#[test]
fn quoted_names_retain_bindings_and_original_definition() -> testresult::TestResult {
    let context = column_rename::schema(&["CREATE TABLE t(\"A\" integer, a integer)"])?;
    let statement = public_analysis::statement(
        "ALTER TABLE public.t DROP COLUMN \"A\"",
        Dialect::postgres(),
    );
    let result = AlterTable::analyze(&statement, context)?;
    let alter = result.ast().alter_table_statement().unwrap();
    let change = alter.commands()[0].column_drop().unwrap();
    assert_eq!(
        alter.resolved_target(),
        Some(&TableName::from(column_rename::name("t")))
    );
    assert_eq!(change.requested().as_str(), "A");
    assert_eq!(
        change.original().unwrap().name(),
        &Some(ColumnName::from("A"))
    );
    assert!(
        result
            .context()
            .get_table(change.table())
            .unwrap()
            .has_column(&ColumnName::from("A"))
    );
    Ok(())
}

#[rstest::rstest]
#[case::other_column("ALTER COLUMN b SET NOT NULL, DROP COLUMN a", true)]
#[case::removed_column("ALTER COLUMN a SET NOT NULL, DROP COLUMN a", false)]
fn attribute_combinations_analyze_without_publication(
    #[case] commands: &str,
    #[case] valid: bool,
) -> testresult::TestResult {
    let context = column_rename::schema(&["CREATE TABLE t(a integer,b integer)"])?;
    let statement =
        public_analysis::statement(&format!("ALTER TABLE t {commands}"), Dialect::postgres());
    let result = AlterTable::analyze(&statement, context)?;
    assert_eq!(
        result.ast().alter_table_statement().unwrap().can_apply(),
        valid
    );
    assert_eq!(
        result
            .context()
            .get_table(&TableName::from(column_rename::name("t")))
            .unwrap()
            .get_column_def(&ColumnName::from("b"))
            .unwrap()
            .is_not_null(),
        Some(false)
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::case_insensitive(&["CREATE TABLE t(a,b)"], "ALTER TABLE t DROP COLUMN A")]
#[case::last_column(&["CREATE TABLE t(a)"], "ALTER TABLE t DROP COLUMN a")]
#[case::last_non_generated(&["CREATE TABLE t(a,b GENERATED ALWAYS AS(1))"], "ALTER TABLE t DROP COLUMN a")]
#[case::key(&["CREATE TABLE t(a PRIMARY KEY,b)"], "ALTER TABLE t DROP COLUMN a")]
#[case::view(&["CREATE TABLE t(a,b)", "CREATE VIEW v AS SELECT a FROM t"], "ALTER TABLE t DROP COLUMN a")]
#[case::trigger_body(&["CREATE TABLE t(a,b)", "CREATE TRIGGER tr AFTER UPDATE ON t BEGIN SELECT NEW.a; END"], "ALTER TABLE t DROP a")]
#[case::trigger_event_only(&["CREATE TABLE t(a,b)", "CREATE TRIGGER tr AFTER UPDATE OF a ON t BEGIN SELECT 1; END"], "ALTER TABLE t DROP a")]
#[case::inline_check(&["CREATE TABLE t(a CHECK(a IS NOT NULL),b)"], "ALTER TABLE t DROP COLUMN a")]
#[case::wildcard_view(&["CREATE TABLE t(a,b)", "CREATE VIEW v AS SELECT * FROM t"], "ALTER TABLE t DROP COLUMN a")]
#[case::table_check(&["CREATE TABLE t(a,b,CHECK(a IS NOT NULL))"], "ALTER TABLE t DROP COLUMN a")]
fn sqlite_semantics_match_embedded_oracle(
    #[case] schema: &[&str],
    #[case] sql: &str,
) -> testresult::TestResult {
    let oracle = ::rusqlite::Connection::open_in_memory()?;
    for ddl in schema {
        oracle.execute_batch(ddl)?;
    }
    let accepted = oracle.execute_batch(sql).is_ok();
    let context = column_rename::sqlite_schema(schema)?;
    let statement = public_analysis::statement(sql, Dialect::sqlite());
    let result = AlterTable::analyze(&statement, context)?;
    let alter = result.ast().alter_table_statement().unwrap();
    assert_eq!(alter.can_apply(), accepted, "{schema:?}: {sql}");
    let change = alter.commands()[0].column_drop().unwrap();
    assert!(
        result
            .context()
            .get_table(change.table())
            .unwrap()
            .has_column(&ColumnName::from("a"))
    );
    assert!(
        result
            .context()
            .get_table(change.table())
            .unwrap()
            .alter_statements()
            .is_empty()
    );
    Ok(())
}

#[test]
fn trigger_and_rule_bindings_are_retained_without_dropping_their_registrations()
-> testresult::TestResult {
    let context = column_rename::schema(&[
        "CREATE TABLE t(a integer,b integer)",
        "CREATE FUNCTION audit_function() RETURNS trigger LANGUAGE plpgsql AS 'BEGIN RETURN NEW; END'",
        "CREATE TRIGGER audit BEFORE UPDATE OF a ON t FOR EACH ROW WHEN (NEW.a IS NULL) EXECUTE FUNCTION audit_function()",
        "CREATE RULE audit_rule AS ON UPDATE TO t DO ALSO SELECT NEW.a",
    ])?;
    let statement = public_analysis::statement("ALTER TABLE t DROP a CASCADE", Dialect::postgres());
    let result = AlterTable::analyze(&statement, context)?;
    let change = result.ast().alter_table_statement().unwrap().commands()[0]
        .column_drop()
        .unwrap();
    assert!(
        change
            .dependencies()
            .iter()
            .any(|dep| dep.kind() == DropDependencyKind::Trigger)
    );
    assert!(
        change
            .dependencies()
            .iter()
            .any(|dep| dep.kind() == DropDependencyKind::Rule)
    );
    assert_eq!(change.outcome(), ColumnDropOutcome::Applicable);
    let table = result.context().get_table(change.table()).unwrap();
    assert_eq!(table.triggers().names().len(), 1);
    assert_eq!(table.rules().identities().len(), 1);
    Ok(())
}

#[test]
fn unbound_generation_expression_is_an_explicit_knowledge_gap() -> testresult::TestResult {
    let context = column_attributes::generation_dependency_context(false)?;
    let statement = public_analysis::statement("ALTER TABLE t DROP a", Dialect::postgres());
    let result = AlterTable::analyze(&statement, context)?;
    let alter = result.ast().alter_table_statement().unwrap();
    let change = alter.commands()[0].column_drop().unwrap();
    assert!(
        change
            .knowledge_gaps()
            .contains(&DropKnowledgeGap::UnboundExpression)
    );
    assert!(alter.requires_application_validation());
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_index_metadata_prevents_removal() -> testresult::TestResult {
    let oracle = ::rusqlite::Connection::open_in_memory()?;
    oracle.execute_batch("CREATE TABLE t(a,b); CREATE INDEX ix ON t(a)")?;
    assert!(oracle.execute_batch("ALTER TABLE t DROP a").is_err());
    let context = alter_table::sqlite_drop_index_context()?;
    let statement = public_analysis::statement("ALTER TABLE t DROP a", Dialect::sqlite());
    let result = AlterTable::analyze(&statement, context)?;
    let change = result.ast().alter_table_statement().unwrap().commands()[0]
        .column_drop()
        .unwrap();
    assert_eq!(change.outcome(), ColumnDropOutcome::Rejected);
    assert!(
        change
            .dependencies()
            .iter()
            .any(|dep| dep.kind() == DropDependencyKind::Index)
    );
    Ok(())
}

#[test]
fn earlier_cascade_decisions_retire_shared_dependencies_for_later_drops() -> testresult::TestResult
{
    let context = column_rename::schema(&[
        "CREATE TABLE t(a integer,b integer,c integer,UNIQUE(a,b))",
        "CREATE TABLE child(x integer,y integer,FOREIGN KEY(x,y) REFERENCES t(a,b))",
    ])?;
    let statement = public_analysis::statement(
        "ALTER TABLE t DROP a CASCADE, DROP b RESTRICT",
        Dialect::postgres(),
    );
    let result = AlterTable::analyze(&statement, context)?;
    let alter = result.ast().alter_table_statement().unwrap();
    assert!(alter.can_apply(), "{:?}", result.context().problems());
    assert!(
        alter.commands()[1]
            .column_drop()
            .unwrap()
            .dependencies()
            .is_empty()
    );
    assert!(
        result
            .context()
            .get_table(&TableName::from(column_rename::name("child")))
            .unwrap()
            .foreign_keys()
            .next()
            .is_some()
    );
    Ok(())
}

#[rstest::rstest]
#[case::restrict("DROP a RESTRICT", false)]
#[case::cascade("DROP a CASCADE", true)]
#[case::cascade_then_skip("DROP a CASCADE, DROP IF EXISTS g", true)]
fn generated_dependencies_use_supplied_bindings(
    #[case] commands: &str,
    #[case] applicable: bool,
) -> testresult::TestResult {
    let context = column_attributes::generation_dependency_context(true)?;
    let statement =
        public_analysis::statement(&format!("ALTER TABLE t {commands}"), Dialect::postgres());
    let result = AlterTable::analyze(&statement, context)?;
    let alter = result.ast().alter_table_statement().unwrap();
    assert_eq!(alter.can_apply(), applicable);
    let change = alter.commands()[0].column_drop().unwrap();
    assert!(
        change
            .dependencies()
            .iter()
            .any(|dep| dep.kind() == DropDependencyKind::GeneratedColumn && dep.member() == "g")
    );
    assert!(
        !change
            .knowledge_gaps()
            .contains(&DropKnowledgeGap::UnboundExpression)
    );
    assert!(
        result
            .context()
            .get_table(change.table())
            .unwrap()
            .has_column(&ColumnName::from("g"))
    );
    if commands.contains("IF EXISTS") {
        assert_eq!(
            alter.commands()[1].column_drop().unwrap().outcome(),
            ColumnDropOutcome::Skipped
        );
    }
    Ok(())
}

#[rstest::rstest]
#[case::whole_row("(t.*) IS NULL", true)]
#[case::removed_field("(t.*).a", false)]
#[case::other_field("(t.*).b", true)]
#[case::aliased_removed_field("(r.*).x", false)]
#[case::aliased_other_field("(r.*).y", true)]
fn whole_rows_and_fields_have_distinct_dependencies(
    #[case] projection: &str,
    #[case] applicable: bool,
) -> testresult::TestResult {
    let source = if projection.contains("r.*") {
        "t AS r(x,y)"
    } else {
        "t"
    };
    let context = column_rename::schema(&[
        "CREATE TABLE t(a integer,b integer)",
        &format!("CREATE VIEW v AS SELECT {projection} FROM {source}"),
    ])?;
    let statement = public_analysis::statement("ALTER TABLE t DROP a", Dialect::postgres());
    let result = AlterTable::analyze(&statement, context)?;
    let change = result.ast().alter_table_statement().unwrap().commands()[0]
        .column_drop()
        .unwrap();
    assert_eq!(change.is_locally_applicable(), applicable);
    assert_eq!(change.dependencies().is_empty(), applicable);
    Ok(())
}
