//! SQL-to-semantics and catalog-chain regressions; no external server is required.
use crate::{
    context::{BaseParseContext, ParseContext},
    names::ColumnName,
    syn::{ListOpt, Opt},
};

#[rstest::rstest]
#[case::zero("0", Some(0), true)]
#[case::normal("123", Some(123), true)]
#[case::maximum("10000", Some(10000), true)]
#[case::clamped("10001", Some(10000), true)]
#[case::numeric_default("-1", None, true)]
#[case::keyword_default("DEFAULT", None, true)]
#[case::too_low("-2", Some(7), false)]
fn statistics_changes_are_bound_and_applied(
    #[case] request: &str,
    #[case] expected: Option<i32>,
    #[case] applicable: bool,
) -> testresult::TestResult {
    let context = test_helpers::seed("CREATE TABLE public.t(a integer)", "17")?;
    let context =
        test_helpers::apply(context, "ALTER TABLE t ALTER a SET STATISTICS 7")?.into_context();
    let result = test_helpers::apply(
        context,
        &format!("ALTER TABLE t ALTER a SET STATISTICS {request}"),
    )?;
    let statement = result
        .ast()
        .alter_table_statement()
        .expect("analyzed ALTER TABLE");
    assert_eq!(statement.can_apply(), applicable);
    assert_eq!(
        statement.resolved_target(),
        Some(&test_helpers::table_name())
    );
    let change = statement.commands()[0]
        .column_attribute_change()
        .expect("column attributes");
    assert_eq!(change.column(), &ColumnName::from("a"));
    let bound = change.binding().definition().expect("bound column");
    assert_eq!(bound.get_type(), Some(&crate::sem::PgBuiltInType::int4()));
    assert_eq!(bound.statistics_target(), Some(7));
    assert_eq!(
        change.action().statistics_request(),
        Some(if request == "DEFAULT" {
            None
        } else {
            Some(request.parse()?)
        })
    );
    assert_eq!(
        change.action().statistics_target().unwrap().effective(),
        if applicable { expected } else { None }
    );
    assert_eq!(
        test_helpers::column(result.context(), "a").statistics_target(),
        expected
    );
    let expected_code = if request == "10001" {
        Some("alter_column_statistics_reduced")
    } else if !applicable {
        Some("alter_column_statistics_too_low")
    } else {
        None
    };
    assert_eq!(
        result
            .context()
            .problems()
            .first()
            .map(crate::sem::AnalysisProblem::code),
        expected_code
    );
    let context = result.into_context();
    test_helpers::assert_select_nullability(context, "a", Some(false))?;
    Ok(())
}

#[rstest::rstest]
#[case::pg15("15", false)]
#[case::pg16("16", false)]
#[case::pg17("17", true)]
#[case::pg18("18", true)]
fn statistics_default_respects_selected_version(
    #[case] version: &str,
    #[case] accepted: bool,
) -> testresult::TestResult {
    let context = test_helpers::seed("CREATE TABLE t(a integer)", version)?;
    let result = test_helpers::apply(context, "ALTER TABLE t ALTER a SET STATISTICS DEFAULT")?;
    assert_eq!(
        result.ast().alter_table_statement().unwrap().can_apply(),
        accepted
    );
    assert_eq!(result.context().problems().len(), usize::from(!accepted));
    if !accepted {
        assert_eq!(
            result.context().problems()[0].code(),
            "alter_column_version"
        );
    }
    Ok(())
}

#[rstest::rstest]
#[case::set("SET NOT NULL", true)]
#[case::drop("DROP NOT NULL", false)]
#[case::phase_order("SET NOT NULL, ALTER a DROP NOT NULL", true)]
#[case::reverse_phase_order("DROP NOT NULL, ALTER a SET NOT NULL", true)]
fn nullability_reaches_subsequent_queries(
    #[case] operation: &str,
    #[case] expected: bool,
) -> testresult::TestResult {
    let context = test_helpers::seed("CREATE TABLE t(a integer NOT NULL)", "17")?;
    let result = test_helpers::apply(context, &format!("ALTER TABLE t ALTER a {operation}"))?;
    assert!(result.ast().alter_table_statement().unwrap().can_apply());
    assert!(result.context().problems().is_empty());
    let column = test_helpers::column(result.context(), "a");
    assert_eq!(column.is_not_null(), Some(expected));
    let constraint = column
        .constraints()
        .iter()
        .find_map(crate::sem::ColumnConstraint::as_nullability)
        .expect("nullability constraint");
    assert_eq!(constraint.is_not_null(), expected);
    test_helpers::assert_select_nullability(result.into_context(), "a", Some(expected))?;
    Ok(())
}

#[rstest::rstest]
#[case::inline_primary("CREATE TABLE t(a integer PRIMARY KEY)", "alter_column_primary_key")]
#[case::table_primary(
    "CREATE TABLE t(a integer, CONSTRAINT pk PRIMARY KEY(a))",
    "alter_column_primary_key"
)]
#[case::identity(
    "CREATE TABLE t(a integer GENERATED ALWAYS AS IDENTITY)",
    "alter_column_identity"
)]
fn cannot_drop_required_not_null(
    #[case] declaration: &str,
    #[case] code: &str,
) -> testresult::TestResult {
    let context = test_helpers::seed(declaration, "17")?;
    let original_nullability = test_helpers::column(&context, "a").is_not_null();
    let result = test_helpers::apply(
        context,
        "ALTER TABLE t ALTER a SET STATISTICS 11, ALTER a DROP NOT NULL",
    )?;
    assert!(!result.ast().alter_table_statement().unwrap().can_apply());
    assert!(
        result
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code() == code)
    );
    assert_eq!(
        test_helpers::column(result.context(), "a").statistics_target(),
        None
    );
    test_helpers::assert_select_nullability(result.into_context(), "a", original_nullability)?;
    Ok(())
}

#[rstest::rstest]
#[case::positive("SET (n_distinct=5, n_distinct_inherited=-0.5)", Some(5.0), Some(-0.5))]
#[case::zero_exponent("SET (n_distinct='0e-9999')", Some(0.0), None)]
#[case::lower_bound("SET (n_distinct=-1)", Some(-1.0), None)]
#[case::quoted("SET (n_distinct=' 2 ')", Some(2.0), None)]
#[case::reset("SET (n_distinct=4), ALTER a RESET(n_distinct)", None, None)]
#[case::reset_unknown("RESET (unknown_option, unknown_option)", None, None)]
#[case::reset_repeated("RESET (n_distinct, n_distinct)", None, None)]
#[case::successive("SET (n_distinct=2), ALTER a SET(n_distinct=3)", Some(3.0), None)]
fn column_options_persist_and_reset(
    #[case] operation: &str,
    #[case] distinct: Option<f64>,
    #[case] inherited: Option<f64>,
) -> testresult::TestResult {
    let context = test_helpers::seed("CREATE TABLE t(a integer)", "17")?;
    let result = test_helpers::apply(context, &format!("ALTER TABLE t ALTER a {operation}"))?;
    assert!(result.ast().alter_table_statement().unwrap().can_apply());
    assert!(
        result.context().problems().is_empty(),
        "{:?}",
        result.context().problems()
    );
    let column = test_helpers::column(result.context(), "a");
    assert_eq!(column.distinct_estimate(), distinct);
    assert_eq!(column.inherited_distinct_estimate(), inherited);
    let json = ::serde_json::to_string(column)?;
    let restored: crate::sem::ColumnDefinition = ::serde_json::from_str(&json)?;
    assert_eq!(restored, *column);
    assert_eq!(column.clone(), restored);
    Ok(())
}

#[rstest::rstest]
#[case::unknown("SET (other=2)", "alter_column_unknown_option")]
#[case::namespace("SET (other.n_distinct=2)", "alter_column_unknown_option")]
#[case::duplicate("SET (n_distinct=2,n_distinct=3)", "alter_column_duplicate_option")]
#[case::low("SET (n_distinct=-1.1)", "alter_column_invalid_option")]
#[case::not_number("SET (n_distinct='abc')", "alter_column_invalid_option")]
#[case::nan("SET (n_distinct='NaN')", "alter_column_invalid_option")]
#[case::underflow("SET (n_distinct='1e-9999')", "alter_column_invalid_option")]
#[case::subnormal("SET (n_distinct='5e-324')", "alter_column_invalid_option")]
#[case::infinite("SET (n_distinct='Infinity')", "alter_column_invalid_option")]
#[case::missing("SET (n_distinct)", "alter_column_invalid_option")]
#[case::boolean("SET (n_distinct=true)", "alter_column_invalid_option")]
#[case::type_name("SET (n_distinct=integer)", "alter_column_invalid_option")]
#[case::keyword("SET (n_distinct=on)", "alter_column_invalid_option")]
#[case::reset_value("RESET(n_distinct=2)", "alter_column_reset_value")]
fn rejected_options_leave_catalog_unchanged(
    #[case] operation: &str,
    #[case] code: &str,
) -> testresult::TestResult {
    let context = test_helpers::seed("CREATE TABLE t(a integer)", "17")?;
    let original = test_helpers::column(&context, "a").clone();
    let result = test_helpers::apply(
        context,
        &format!("ALTER TABLE t ALTER a SET NOT NULL, ALTER a {operation}"),
    )?;
    let statement = result.ast().alter_table_statement().unwrap();
    assert!(!statement.can_apply());
    let change = statement
        .commands()
        .last()
        .unwrap()
        .column_attribute_change()
        .unwrap();
    assert!(change.binding().definition().is_some());
    assert!(!change.can_apply());
    assert!(change.action().attribute_options().is_some());
    assert!(
        change
            .problems()
            .iter()
            .any(crate::sem::problem::AlterColumnProblem::is_error)
    );
    assert!(
        result
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code() == code)
    );
    assert_eq!(test_helpers::column(result.context(), "a"), &original);
    test_helpers::assert_select_nullability(result.into_context(), "a", Some(false))?;
    Ok(())
}

#[rstest::rstest]
#[case::missing_column(
    "CREATE TABLE t(a integer)",
    "ALTER TABLE t ALTER missing SET STATISTICS 4",
    "alter_column_missing_column"
)]
#[case::missing_relation(
    "CREATE TABLE other(a integer)",
    "ALTER TABLE t ALTER a SET STATISTICS 4",
    "alter_column_missing_relation"
)]
#[case::wrong_kind(
    "CREATE VIEW t AS SELECT 1 AS a",
    "ALTER TABLE t ALTER a SET STATISTICS 4",
    "alter_column_missing_relation"
)]
#[case::wrong_kind_if_exists(
    "CREATE VIEW t AS SELECT 1 AS a",
    "ALTER TABLE IF EXISTS t ALTER a SET STATISTICS 4",
    "alter_column_missing_relation"
)]
fn failed_bindings_are_retained(
    #[case] declaration: &str,
    #[case] alteration: &str,
    #[case] code: &str,
) -> testresult::TestResult {
    let context = test_helpers::seed(declaration, "17")?;
    let result = test_helpers::apply(context, alteration)?;
    let statement = result.ast().alter_table_statement().unwrap();
    assert!(!statement.can_apply());
    assert!(
        statement.commands()[0]
            .column_attribute_change()
            .unwrap()
            .binding()
            .definition()
            .is_none()
    );
    assert_eq!(result.context().problems()[0].code(), code);
    let context =
        test_helpers::apply(result.into_context(), "CREATE TABLE later(a integer)")?.into_context();
    assert!(
        context
            .get_table(&crate::names::TableName::from(
                crate::names::FullName::with_schema("public", "later")
            ))
            .is_some()
    );
    Ok(())
}

#[test]
fn if_exists_retains_a_skipped_missing_target() -> testresult::TestResult {
    let result = test_helpers::apply(
        test_helpers::context("17"),
        "ALTER TABLE IF EXISTS missing ALTER a SET NOT NULL",
    )?;
    let statement = result.ast().alter_table_statement().unwrap();
    assert!(!statement.can_apply());
    assert!(statement.resolved_target().is_none());
    assert!(result.context().problems().is_empty());
    Ok(())
}

#[test]
fn canonical_binding_survives_freezing_and_preserves_quoted_names() -> testresult::TestResult {
    let context = test_helpers::seed(r#"CREATE TABLE public."T"("A" integer)"#, "17")?;
    let context = test_helpers::child(context.freeze());
    let result = test_helpers::apply(context, r#"ALTER TABLE "T" ALTER "A" SET STATISTICS 25"#)?;
    let target = crate::names::TableName::from(crate::names::FullName::with_schema("public", "T"));
    let statement = result.ast().alter_table_statement().unwrap();
    assert_eq!(statement.resolved_target(), Some(&target));
    let context = test_helpers::child(result.into_context().freeze());
    assert_eq!(
        context
            .get_table(&target)
            .unwrap()
            .get_column_def(&ColumnName::from("A"))
            .unwrap()
            .statistics_target(),
        Some(25)
    );
    Ok(())
}

#[rstest::rstest]
#[case::stored("STORED", false, true)]
#[case::ordinary("", false, false)]
#[case::ordinary_if_exists("", true, true)]
#[case::virtual_column("VIRTUAL", false, false)]
#[case::virtual_if_exists("VIRTUAL", true, false)]
fn drop_expression_records_applicability(
    #[case] mode: &str,
    #[case] if_exists: bool,
    #[case] accepted: bool,
) -> testresult::TestResult {
    let declaration = if mode.is_empty() {
        "CREATE TABLE t(a integer, g integer)".to_owned()
    } else {
        format!("CREATE TABLE t(a integer, g integer GENERATED ALWAYS AS (1) {mode})")
    };
    let context = if mode == "VIRTUAL" {
        test_helpers::virtual_column_context()
    } else {
        test_helpers::seed(&declaration, "18")?
    };
    let original = test_helpers::column(&context, "g").clone();
    let clause = if if_exists { " IF EXISTS" } else { "" };
    let result = test_helpers::apply(
        context,
        &format!("ALTER TABLE t ALTER g DROP EXPRESSION{clause}"),
    )?;
    assert_eq!(
        result.ast().alter_table_statement().unwrap().can_apply(),
        accepted
    );
    if accepted {
        assert!(
            test_helpers::column(result.context(), "g")
                .constraints()
                .iter()
                .all(|constraint| constraint.as_generated().is_none())
        );
    } else {
        assert_eq!(test_helpers::column(result.context(), "g"), &original);
    }
    assert_eq!(result.context().problems().len(), usize::from(!accepted));
    Ok(())
}

#[test]
fn drop_expression_changes_insert_and_update_rules() -> testresult::TestResult {
    let context = test_helpers::seed(
        "CREATE TABLE t(a integer, g integer GENERATED ALWAYS AS (1) STORED)",
        "17",
    )?;
    let result = test_helpers::apply(context, "INSERT INTO t(a,g) VALUES(1,2)")?;
    assert!(
        result
            .context()
            .problems()
            .iter()
            .any(|problem| problem.insert_problem().is_some())
    );
    let result = test_helpers::apply(
        result.into_context(),
        "ALTER TABLE t ALTER g DROP EXPRESSION",
    )?;
    let count = result.context().problems().len();
    let result = test_helpers::apply(result.into_context(), "INSERT INTO t(a,g) VALUES(1,2)")?;
    assert_eq!(result.context().problems().len(), count);
    let result = test_helpers::apply(result.into_context(), "UPDATE t SET g=2")?;
    assert_eq!(result.context().problems().len(), count);
    Ok(())
}

#[test]
fn a_failed_repeated_drop_expression_is_atomic() -> testresult::TestResult {
    let context = test_helpers::seed(
        "CREATE TABLE t(a integer, g integer GENERATED ALWAYS AS (1) STORED)",
        "17",
    )?;
    let original = test_helpers::column(&context, "g").clone();
    let result = test_helpers::apply(
        context,
        "ALTER TABLE t ALTER g DROP EXPRESSION, ALTER g DROP EXPRESSION",
    )?;
    assert!(!result.ast().alter_table_statement().unwrap().can_apply());
    assert_eq!(test_helpers::column(result.context(), "g"), &original);
    assert_eq!(
        result.context().problems()[0].code(),
        "alter_column_not_stored_generated"
    );
    Ok(())
}

#[test]
fn standalone_semantic_command_api_resolves_column_attributes() -> testresult::TestResult {
    let context = test_helpers::seed("CREATE TABLE t(a integer)", "17")?;
    let node =
        crate::test_helpers::SynBuilder::new().parse("ALTER TABLE t ALTER a SET STATISTICS 42");
    let statement = node.as_alter_table_stmt().as_inner().unwrap();
    let commands = statement.get_cmds().as_inner().unwrap();
    let command = commands[0].as_alter_table_cmd().as_inner().unwrap();
    let subtype = command.get_subtype().as_inner().unwrap();
    let (command, context) = crate::sem::AlterObjSubCommand::analyze_alter_table_cmd(
        context,
        &test_helpers::table_name(),
        command,
        subtype,
        &::sql_fun_core::IVec::default(),
    )?;
    let change = command.column_attribute_change().unwrap();
    assert!(change.can_apply());
    assert_eq!(
        change.action().statistics_target().unwrap().effective(),
        Some(42)
    );
    assert!(change.binding().definition().is_some());
    assert!(context.problems().is_empty());
    // Command analysis returns semantics without applying the enclosing DDL.
    assert_eq!(
        test_helpers::column(&context, "a").statistics_target(),
        None
    );
    Ok(())
}

#[test]
fn virtual_generated_column_rejects_statistics_changes() -> testresult::TestResult {
    let result = test_helpers::apply(
        test_helpers::virtual_column_context(),
        "ALTER TABLE t ALTER g SET STATISTICS 10",
    )?;
    assert!(!result.ast().alter_table_statement().unwrap().can_apply());
    assert_eq!(
        result.context().problems()[0].code(),
        "alter_column_virtual_generated"
    );
    assert_eq!(
        test_helpers::column(result.context(), "g").statistics_target(),
        None
    );
    Ok(())
}

#[test]
fn legacy_column_json_shape_remains_readable() -> testresult::TestResult {
    let context = test_helpers::seed("CREATE TABLE t(a integer)", "17")?;
    let original = test_helpers::column(&context, "a");
    let json = ::serde_json::to_value(original)?;
    assert!(json.get("attributes").is_none());
    let restored: crate::sem::ColumnDefinition = ::serde_json::from_value(json)?;
    assert_eq!(&restored, original);
    Ok(())
}

mod test_helpers {
    // pg_query 6 uses PostgreSQL 17 grammar. Seed the supported semantic catalog
    // mode directly so PostgreSQL 18 virtual-column restrictions are exercised.
    pub(super) fn virtual_column_context() -> crate::context::SchemaFileContext {
        use crate::{
            context::TableInfoWrite,
            sem::{
                ColumnConstraint, CreateTable, CreateTableMode, GeneratedColumnConstraint,
                GeneratedColumnMode, PgBuiltInType, ScalarConstExpr, SemScalarExpr,
            },
        };
        let mut context = context("18");
        let column = ColumnName::from("g");
        let constraint = ColumnConstraint::generated(GeneratedColumnConstraint::new(
            &table_name(),
            &column,
            SemScalarExpr::new_const(ScalarConstExpr::new_integer(1)),
            Some(GeneratedColumnMode::r#virtual()),
        ));
        let table = CreateTable::new(
            &table_name(),
            vec![ColumnDefinition::new(
                &Some(column),
                Some(&PgBuiltInType::int4()),
                Some(false),
                &[constraint],
            )],
            CreateTableMode::from_if_not_exists(false),
            false,
            &crate::test_helpers::source_span(),
        );
        context.insert_create_table(table_name().full_name(), &table);
        context
    }

    use crate::{
        context::{
            BaseParseContext, FrozenBaseContext, MutSourcePathWithSpan, ParseContext,
            SchemaFileContext,
        },
        names::{ColumnName, TableName},
        sem::{AnalysisError, AstAndContextPair, ColumnDefinition},
    };
    pub(super) fn table_name() -> TableName {
        TableName::from(crate::names::FullName::with_schema("public", "t"))
    }
    pub(super) fn context(version: &str) -> SchemaFileContext {
        let metadata = ::serde_json::from_value(::serde_json::json!({"engine":"postgres", "dialect":"postgres", "version":version, "search_path":["public"]})).unwrap();
        let mut inventory =
            crate::test_helpers::TestParseContext::default().with_metadata(&metadata);
        inventory.set_search_path(&[crate::names::SchemaName::from("public")]);
        inventory.setup_type(crate::sem::PgBuiltInType::int4());
        child(inventory.freeze())
    }
    pub(super) fn child(parent: FrozenBaseContext) -> SchemaFileContext {
        let mut context = SchemaFileContext::new(
            std::path::Path::new("attributes.sql"),
            &parent,
            parent.get_args(),
            parent.metadata(),
        );
        context.set_source_path_with_span(&crate::test_helpers::source_span());
        context
    }
    pub(super) fn apply(
        context: SchemaFileContext,
        sql: &str,
    ) -> Result<AstAndContextPair<SchemaFileContext>, AnalysisError> {
        crate::test_helpers::ownership::ownership_statement(sql)?
            .resolve_and_apply(context, &mut crate::test_helpers::create_cache_context())
    }
    pub(super) fn seed(sql: &str, version: &str) -> Result<SchemaFileContext, AnalysisError> {
        Ok(apply(context(version), sql)?.into_context())
    }
    pub(super) fn column<'a>(context: &'a SchemaFileContext, name: &str) -> &'a ColumnDefinition {
        context
            .get_table(&table_name())
            .unwrap()
            .get_column_def(&ColumnName::from(name))
            .unwrap()
    }
    pub(super) fn assert_select_nullability(
        context: SchemaFileContext,
        column: &str,
        expected: Option<bool>,
    ) -> Result<(), AnalysisError> {
        let result = apply(context, &format!("SELECT {column} FROM t"))?;
        let query = result.ast().select_statement().unwrap();
        assert_eq!(
            query
                .get_result_column_def(&ColumnName::from(column))
                .unwrap()
                .is_not_null(),
            expected
        );
        Ok(())
    }
}
