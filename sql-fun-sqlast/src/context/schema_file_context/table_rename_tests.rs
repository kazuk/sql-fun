//! Enduring SQL-to-catalog rename contracts with explicit metadata.

#[cfg(feature = "sqlite")]
use crate::{context::ViewInfoRead, names::FullName as NamesFullName};
use crate::{
    context::{GetSourcePathWithSpan, ObjectInfoRead, ParseContext, TableInfoRead, TypeInfoRead},
    names::SchemaName,
    sem::{ColumnConstraint, QueryMetadata, SemAst},
    test_helpers::{ownership::apply_ownership_sql, table_rename as test_helpers},
};

#[rstest::rstest]
#[case::unqualified("ALTER TABLE before_name RENAME TO after_name")]
#[case::qualified("ALTER TABLE public.before_name RENAME TO after_name")]
fn renamed_table_replaces_the_catalog_identity(
    #[case] sql: &str,
    #[values(false, true)] inherited: bool,
) -> testresult::TestResult {
    let context = test_helpers::table_layer(inherited)?;
    let old = test_helpers::name("before_name");
    let new = test_helpers::name("after_name");
    let original_source = context
        .get_table_impl(&old)
        .unwrap()
        .get_source_path_with_span()
        .clone();
    let result = apply_ownership_sql(context, sql)?;
    let statement = result
        .ast()
        .alter_table_statement()
        .expect("analyzed rename");
    assert!(statement.can_apply());
    assert_eq!(statement.resolved_target().unwrap().full_name(), &old);
    assert_eq!(statement.rename_destination().unwrap().full_name(), &new);
    let context = result.into_context();
    assert!(context.get_table_impl(&old).is_none());
    assert!(context.object_kind(&old).is_none());
    assert!(context.get_type_impl(&old).is_none());
    assert_eq!(
        context.get_table_impl(&new).unwrap().name().full_name(),
        &new
    );
    assert_eq!(
        context
            .get_table_impl(&new)
            .unwrap()
            .get_source_path_with_span()
            .span(),
        original_source.span()
    );
    assert_eq!(
        context
            .get_table_impl(&new)
            .unwrap()
            .get_source_path_with_span()
            .source_path(),
        original_source.source_path()
    );
    assert!(context.get_type_impl(&new).is_some());
    let history = context.get_table_impl(&new).unwrap().alter_statements();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].resolved_target().unwrap().full_name(), &old);
    test_helpers::assert_select(context, "SELECT id FROM after_name")?;
    Ok(())
}

#[rstest::rstest]
#[case::missing("ALTER TABLE missing RENAME TO after_name", "missing", 1)]
#[case::if_exists("ALTER TABLE IF EXISTS missing RENAME TO after_name", "missing", 0)]
#[case::same_name("ALTER TABLE before_name RENAME TO before_name", "before_name", 1)]
#[case::occupied_table("ALTER TABLE before_name RENAME TO occupied", "before_name", 1)]
#[case::occupied_type("ALTER TABLE before_name RENAME TO type_only", "before_name", 1)]
fn rejected_or_skipped_rename_preserves_the_catalog(
    #[case] sql: &str,
    #[case] requested: &str,
    #[case] problem_count: usize,
) -> testresult::TestResult {
    let context = test_helpers::conflicting_layer()?;
    let result = apply_ownership_sql(context, sql)?;
    let statement = result
        .ast()
        .alter_table_statement()
        .expect("analyzed rename");
    assert!(!statement.can_apply());
    assert_eq!(statement.table().full_name().name().as_str(), requested);
    assert_eq!(result.context().problems().len(), problem_count);
    assert!(
        result
            .context()
            .get_table_impl(&test_helpers::name("before_name"))
            .is_some()
    );
    assert!(
        result
            .context()
            .get_table_impl(&test_helpers::name("after_name"))
            .is_none()
    );
    test_helpers::assert_select(result.into_context(), "SELECT id FROM before_name")?;
    Ok(())
}

#[test]
fn freeze_repeated_renames_and_old_name_reuse_preserve_parent() -> testresult::TestResult {
    let context = test_helpers::table_layer(false)?;
    let parent = context.freeze();
    let context = test_helpers::child(&parent);
    let context = apply_ownership_sql(context, "ALTER TABLE before_name RENAME TO middle_name")?
        .into_context();
    let context =
        apply_ownership_sql(context, "CREATE TABLE before_name(other integer)")?.into_context();
    let frozen = context.freeze();
    let context = test_helpers::child(&frozen);
    let context = apply_ownership_sql(context, "ALTER TABLE middle_name RENAME TO after_name")?
        .into_context();
    assert!(
        parent
            .get_table_impl(&test_helpers::name("before_name"))
            .is_some()
    );
    assert!(
        parent
            .get_table_impl(&test_helpers::name("after_name"))
            .is_none()
    );
    assert!(
        context
            .get_table_impl(&test_helpers::name("middle_name"))
            .is_none()
    );
    assert!(
        context
            .get_table_impl(&test_helpers::name("before_name"))
            .unwrap()
            .has_column(&crate::names::ColumnName::from("other"))
    );
    test_helpers::assert_select(context, "SELECT id FROM after_name")?;
    Ok(())
}

#[test]
fn destination_stays_in_the_bound_schema_and_preserves_quoted_names() -> testresult::TestResult {
    let context = apply_ownership_sql(
        test_helpers::context(),
        r#"CREATE TABLE public.before_name(id integer)"#,
    )?
    .into_context();
    let context =
        apply_ownership_sql(context, r#"CREATE TABLE other."After.Name"(other integer)"#)?
            .into_context();
    let mut context = context;
    context.set_search_path(&[SchemaName::from("other"), SchemaName::from("public")]);
    let context =
        apply_ownership_sql(context, r#"ALTER TABLE before_name RENAME TO "After.Name""#)?
            .into_context();
    assert!(
        context
            .get_table_impl(&test_helpers::name("After.Name"))
            .is_some()
    );
    assert!(
        context
            .get_table_impl(&crate::names::FullName::with_schema("other", "After.Name"))
            .unwrap()
            .has_column(&crate::names::ColumnName::from("other"))
    );
    test_helpers::assert_select(context, r#"SELECT id FROM public."After.Name""#)?;
    Ok(())
}

#[test]
fn indexes_sequences_and_view_sources_follow_the_renamed_table() -> testresult::TestResult {
    use crate::{
        context::{IndexInfoRead, SequenceInfoRead, ViewInfoRead},
        names::TableName,
    };
    let context = apply_ownership_sql(
        test_helpers::context(),
        "CREATE TABLE before_name(id integer CONSTRAINT before_name_pkey PRIMARY KEY)",
    )?
    .into_context();
    let context = apply_ownership_sql(
        context,
        "CREATE SEQUENCE before_sequence OWNED BY before_name.id",
    )?
    .into_context();
    let context = apply_ownership_sql(
        context,
        "CREATE VIEW preserved AS SELECT id FROM before_name",
    )?
    .into_context();
    let parent = context.freeze();
    let context = apply_ownership_sql(
        test_helpers::child(&parent),
        "ALTER TABLE before_name RENAME TO after_name",
    )?
    .into_context();
    let old = TableName::from(test_helpers::name("before_name"));
    let new = TableName::from(test_helpers::name("after_name"));
    assert!(context.indexes_for_table(&old).is_empty());
    assert_eq!(context.indexes_for_table(&new).len(), 1);
    assert_eq!(
        context.indexes_for_table(&new)[0].name().name().as_str(),
        "before_name_pkey"
    );
    assert!(context.sequences_owned_by(&old).is_empty());
    assert_eq!(
        context
            .get_sequence_impl(&test_helpers::name("before_sequence"))
            .unwrap()
            .owned_by(),
        Some(&new)
    );
    let view = context
        .get_view_impl(&test_helpers::name("preserved"))
        .unwrap()
        .ordinary_definition()
        .unwrap();
    let metadata = QueryMetadata::from_statement(&SemAst::select(view.query()))?;
    assert_eq!(metadata.relations(), &[test_helpers::name("after_name")]);
    test_helpers::assert_select(context, "SELECT id FROM preserved")?;
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_case_insensitive_rename_survives_layers_and_repeated_renames() -> testresult::TestResult {
    use crate::test_helpers::table_rename::apply_sqlite;
    let context = apply_sqlite(
        test_helpers::sqlite_context(),
        "CREATE TABLE Before_Name(id INTEGER)",
    )?
    .into_context();
    let context = apply_sqlite(
        context,
        "CREATE VIEW preserved AS SELECT id FROM Before_Name",
    )?
    .into_context();
    let parent = context.freeze();
    let context = apply_sqlite(
        test_helpers::child(&parent),
        "ALTER TABLE BEFORE_NAME RENAME TO After_Name",
    )?
    .into_context();
    let old = NamesFullName::with_schema("main", "before_name");
    let new = NamesFullName::with_schema("main", "after_name");
    assert!(context.get_table_impl(&old).is_none());
    assert!(context.get_table_impl(&new).is_some());
    let result = apply_sqlite(context, "SELECT id FROM AFTER_NAME")?;
    assert!(result.context().problems().is_empty());
    let context = apply_sqlite(
        result.into_context(),
        "ALTER TABLE after_name RENAME TO Final_Name",
    )?
    .into_context();
    assert!(context.get_table_impl(&new).is_none());
    assert!(
        context
            .get_table_impl(&crate::names::FullName::with_schema("main", "final_name"))
            .is_some()
    );
    let result = apply_sqlite(context, "SELECT id FROM preserved")?;
    assert!(result.context().problems().is_empty());
    let view = result
        .context()
        .get_view_impl(&NamesFullName::with_schema("main", "preserved"))
        .unwrap()
        .ordinary_definition()
        .unwrap();
    let metadata = QueryMetadata::from_statement(&SemAst::select(view.query()))?;
    assert_eq!(
        metadata.relations(),
        &[crate::names::FullName::with_schema("main", "final_name")]
    );
    assert!(parent.get_table_impl(&old).is_some());
    let oracle = ::rusqlite::Connection::open_in_memory()?;
    oracle.execute_batch("CREATE TABLE Before_Name(id INTEGER); CREATE VIEW preserved AS SELECT id FROM Before_Name; ALTER TABLE BEFORE_NAME RENAME TO After_Name; ALTER TABLE after_name RENAME TO Final_Name;")?;
    oracle.prepare("SELECT id FROM preserved")?;
    assert_eq!(
        oracle.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name='Final_Name'",
            [],
            |row| row.get::<_, i64>(0)
        )?,
        1
    );
    Ok(())
}

#[rstest::rstest]
#[case::view("CREATE VIEW before_name AS SELECT id FROM after_name")]
#[case::enumeration("CREATE TYPE before_name AS ENUM ('x')")]
fn old_name_can_be_reused_without_resurrecting_the_table(
    #[case] declaration: &str,
) -> testresult::TestResult {
    let parent = test_helpers::table_layer(false)?.freeze();
    let context = apply_ownership_sql(
        test_helpers::child(&parent),
        "ALTER TABLE before_name RENAME TO after_name",
    )?
    .into_context();
    let context = apply_ownership_sql(context, declaration)?.into_context();
    assert!(context.problems().is_empty());
    assert!(
        context
            .get_table_impl(&test_helpers::name("before_name"))
            .is_none()
    );
    test_helpers::assert_select(context, "SELECT id FROM after_name")?;
    Ok(())
}

#[test]
fn qualified_foreign_key_and_row_type_consumers_follow_the_target() -> testresult::TestResult {
    use crate::{context::FunctionInfoRead, names::ColumnName};
    let context = apply_ownership_sql(
        test_helpers::context(),
        "CREATE TABLE before_name(id integer CONSTRAINT before_key PRIMARY KEY)",
    )?
    .into_context();
    let context = apply_ownership_sql(context, "CREATE TABLE dependent(id integer REFERENCES public.before_name(id), row_value public.before_name)")?.into_context();
    let context = apply_ownership_sql(
        context,
        "CREATE TYPE boxed AS (row_value public.before_name)",
    )?
    .into_context();
    let context = apply_ownership_sql(context, "CREATE FUNCTION retain_row(value public.before_name) RETURNS public.before_name LANGUAGE SQL AS 'SELECT value'")?.into_context();
    assert!(context.problems().is_empty());
    let parent = context.freeze();
    let context = apply_ownership_sql(
        test_helpers::child(&parent),
        "ALTER TABLE before_name RENAME TO after_name",
    )?
    .into_context();
    let child = context
        .get_table_impl(&test_helpers::name("dependent"))
        .unwrap();
    let column = child.get_column_def(&ColumnName::from("id")).unwrap();
    let fk = column
        .constraints()
        .iter()
        .find_map(ColumnConstraint::as_foreign_key)
        .unwrap();
    assert_eq!(
        fk.primary_key_table().unwrap().full_name(),
        &test_helpers::name("after_name")
    );
    assert_eq!(
        child
            .get_column_def(&ColumnName::from("row_value"))
            .unwrap()
            .get_type()
            .unwrap()
            .full_name(),
        Some(&test_helpers::name("after_name"))
    );
    let overloads = context
        .get_function_impl(&test_helpers::name("retain_row"))
        .unwrap();
    assert_eq!(
        overloads
            .get_at(0)
            .unwrap()
            .ret_type()
            .as_ref()
            .unwrap()
            .full_name(),
        Some(&test_helpers::name("after_name"))
    );
    test_helpers::assert_select(context, "SELECT (row_value).id FROM dependent")?;
    Ok(())
}

#[test]
fn trigger_and_rule_identities_follow_the_owning_table() -> testresult::TestResult {
    use crate::{
        context::{RuleInfoRead, TriggerInfoRead},
        sem::{RuleIdentity, TriggerIdentity},
    };
    let context = test_helpers::table_layer(false)?;
    let context = apply_ownership_sql(context, "CREATE FUNCTION audit_function() RETURNS trigger LANGUAGE plpgsql AS 'BEGIN RETURN NEW; END'")?.into_context();
    let context = apply_ownership_sql(context, "CREATE TRIGGER audit AFTER INSERT ON before_name FOR EACH ROW EXECUTE FUNCTION audit_function()")?.into_context();
    let context = apply_ownership_sql(
        context,
        "CREATE RULE audit_rule AS ON INSERT TO before_name DO ALSO NOTHING",
    )?
    .into_context();
    assert!(context.problems().is_empty());
    let parent = context.freeze();
    let context = apply_ownership_sql(
        test_helpers::child(&parent),
        "ALTER TABLE before_name RENAME TO after_name",
    )?
    .into_context();
    let old = test_helpers::name("before_name");
    let new = test_helpers::name("after_name");
    let trigger_name = test_helpers::name("audit");
    assert!(
        context
            .trigger(&TriggerIdentity::postgres(&old, &trigger_name))
            .is_none()
    );
    assert_eq!(
        context
            .trigger(&TriggerIdentity::postgres(&new, &trigger_name))
            .unwrap()
            .target(),
        &new
    );
    assert!(
        context
            .rule(&RuleIdentity::new(&old, "audit_rule"))
            .is_none()
    );
    let rule = context
        .rule(&RuleIdentity::new(&new, "audit_rule"))
        .unwrap();
    assert_eq!(rule.target(), &new);
    assert_eq!(rule.dependencies().relations(), &[new]);
    assert!(
        parent
            .trigger(&TriggerIdentity::postgres(&old, &trigger_name))
            .is_some()
    );
    Ok(())
}

#[test]
fn ownership_follows_the_table_without_resurrecting_the_old_identity() -> testresult::TestResult {
    use crate::{ast::ObjectKind, context::OwnershipInfoRead};
    let context = apply_ownership_sql(
        test_helpers::table_layer(false)?,
        "ALTER TABLE before_name OWNER TO CURRENT_USER",
    )?
    .into_context();
    let parent = context.freeze();
    let context = apply_ownership_sql(
        test_helpers::child(&parent),
        "ALTER TABLE before_name RENAME TO after_name",
    )?
    .into_context();
    assert!(
        context
            .ownership_definitions(ObjectKind::TABLE, &test_helpers::name("before_name"))
            .is_empty()
    );
    assert!(
        context
            .ownership_definitions(ObjectKind::TYPE, &test_helpers::name("before_name"))
            .is_empty()
    );
    assert!(
        context
            .get_table_impl(&test_helpers::name("after_name"))
            .unwrap()
            .owner()
            .is_some()
    );
    assert_eq!(
        context
            .ownership_definitions(ObjectKind::TABLE, &test_helpers::name("after_name"))
            .len(),
        1
    );
    assert_eq!(
        parent
            .ownership_definitions(ObjectKind::TABLE, &test_helpers::name("before_name"))
            .len(),
        1
    );
    Ok(())
}
