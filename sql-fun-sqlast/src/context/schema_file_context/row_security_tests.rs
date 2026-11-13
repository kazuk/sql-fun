//! Contracts between completed ALTER analysis, catalog updates and subsequent analysis.

use crate::{
    ast::Dialect,
    context::BaseParseContext,
    sem::{AlterTable, Kind, RowSecurityAction, RowSecurityOutcome},
    test_helpers::{column_attributes as catalog, public_analysis},
};

#[rstest::rstest]
#[case::enable("ENABLE", RowSecurityAction::enable(), true, true)]
#[case::disable("DISABLE", RowSecurityAction::disable(), false, true)]
#[case::force("FORCE", RowSecurityAction::force(), true, true)]
#[case::no_force("NO FORCE", RowSecurityAction::no_force(), true, false)]
fn row_security_analysis_and_application(
    #[case] operation: &str,
    #[case] action: RowSecurityAction,
    #[case] enabled: bool,
    #[case] forced: bool,
) -> testresult::TestResult {
    let context = catalog::seed("CREATE TABLE public.t(a integer)", "17")?;
    let context = catalog::apply(
        context,
        "ALTER TABLE t ENABLE ROW LEVEL SECURITY, FORCE ROW LEVEL SECURITY",
    )?
    .into_context();
    let sql = format!("ALTER TABLE public.t {operation} ROW LEVEL SECURITY");
    let analyzed = AlterTable::analyze(
        &public_analysis::statement(&sql, Dialect::postgres()),
        context,
    )?;
    let statement = analyzed.ast().alter_table_statement().unwrap();
    assert_eq!(statement.resolved_target(), Some(&catalog::table_name()));
    assert!(statement.can_apply());
    let change = statement.commands()[0].row_security_change().unwrap();
    assert_eq!(change.action(), action);
    assert_eq!(change.outcome(), RowSecurityOutcome::applicable());
    let table = analyzed
        .context()
        .get_table(&catalog::table_name())
        .unwrap();
    assert!(table.row_security_enabled());
    assert!(table.row_security_forced());

    let applied = catalog::apply(analyzed.into_context(), &sql)?;
    assert!(applied.context().problems().is_empty());
    // Subsequent SQL analysis receives the catalog containing the completed change.
    let next = catalog::apply(applied.into_context(), "SELECT a FROM t")?;
    assert!(next.context().problems().is_empty());
    assert!(next.ast().select_statement().is_some());
    let table = next.context().get_table(&catalog::table_name()).unwrap();
    assert_eq!(table.row_security_enabled(), enabled);
    assert_eq!(table.row_security_forced(), forced);
    Ok(())
}

#[test]
fn row_security_sequences_keep_the_flags_independent() -> testresult::TestResult {
    let mut context = catalog::seed("CREATE TABLE t(a integer)", "17")?;
    let table = context.get_table(&catalog::table_name()).unwrap();
    assert!(!table.row_security_enabled());
    assert!(!table.row_security_forced());
    for (operations, enabled, forced) in [
        ("FORCE ROW LEVEL SECURITY", false, true),
        (
            "ENABLE ROW LEVEL SECURITY, DISABLE ROW LEVEL SECURITY",
            false,
            true,
        ),
        (
            "NO FORCE ROW LEVEL SECURITY, ENABLE ROW LEVEL SECURITY",
            true,
            false,
        ),
        (
            "ENABLE ROW LEVEL SECURITY, ENABLE ROW LEVEL SECURITY",
            true,
            false,
        ),
    ] {
        context = catalog::apply(context, &format!("ALTER TABLE t {operations}"))?.into_context();
        assert!(context.problems().is_empty());
        let table = context.get_table(&catalog::table_name()).unwrap();
        assert_eq!(table.row_security_enabled(), enabled);
        assert_eq!(table.row_security_forced(), forced);
    }
    Ok(())
}

#[rstest::rstest]
#[case::missing(
    "ALTER TABLE missing ENABLE ROW LEVEL SECURITY",
    RowSecurityOutcome::missing_target(),
    true
)]
#[case::skipped(
    "ALTER TABLE IF EXISTS missing FORCE ROW LEVEL SECURITY",
    RowSecurityOutcome::skipped(),
    false
)]
#[case::view(
    "ALTER TABLE IF EXISTS v DISABLE ROW LEVEL SECURITY",
    RowSecurityOutcome::invalid_target(Kind::view()),
    true
)]
fn row_security_target_outcomes(
    #[case] sql: &str,
    #[case] outcome: RowSecurityOutcome,
    #[case] diagnostic: bool,
) -> testresult::TestResult {
    let context = catalog::seed("CREATE TABLE t(a integer)", "17")?;
    let context = catalog::apply(context, "CREATE VIEW v AS SELECT a FROM t")?.into_context();
    let result = catalog::apply(context, sql)?;
    let statement = result.ast().alter_table_statement().unwrap();
    assert!(!statement.can_apply());
    assert!(statement.resolved_target().is_none());
    assert_eq!(
        statement.commands()[0]
            .row_security_change()
            .unwrap()
            .outcome(),
        outcome
    );
    assert_eq!(result.context().problems().len(), usize::from(diagnostic));
    if diagnostic {
        assert_eq!(result.context().problems()[0].code(), "table_not_found");
    }
    let table = result.context().get_table(&catalog::table_name()).unwrap();
    assert!(!table.row_security_enabled());
    assert!(!table.row_security_forced());
    Ok(())
}

#[rstest::rstest]
#[case::view("VIEW")]
#[case::sequence("SEQUENCE")]
#[case::index("INDEX")]
fn non_table_alter_cannot_change_a_table(#[case] kind: &str) -> testresult::TestResult {
    let context = catalog::seed("CREATE TABLE t(a integer)", "17")?;
    let result = catalog::apply(
        context,
        &format!("ALTER {kind} t ENABLE ROW LEVEL SECURITY"),
    )?;
    let statement = result.ast().alter_table_statement().unwrap();
    assert_eq!(statement.resolved_target(), Some(&catalog::table_name()));
    assert!(!statement.can_apply());
    assert_eq!(
        statement.commands()[0]
            .row_security_change()
            .unwrap()
            .outcome(),
        RowSecurityOutcome::invalid_statement_kind(),
    );
    assert_eq!(result.context().problems().len(), 1);
    assert_eq!(
        result.context().problems()[0].code(),
        "row_security_requires_table"
    );
    let table = result.context().get_table(&catalog::table_name()).unwrap();
    assert!(!table.row_security_enabled());
    assert!(!table.row_security_forced());
    Ok(())
}
