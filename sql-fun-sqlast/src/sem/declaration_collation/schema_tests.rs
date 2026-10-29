use crate::{
    context::{BaseParseContext, TypeInfoRead},
    sem::{FullName, TableName},
    test_helpers::{declaration_collation::context, ownership::apply_ownership_sql},
};

#[rstest::rstest]
#[case::table(r#"CREATE TABLE public.rejected(value public."custom.type" COLLATE "C")"#)]
#[case::composite(r#"CREATE TYPE public.rejected AS (value public."custom.type" COLLATE "C")"#)]
#[case::domain(r#"CREATE DOMAIN public.rejected AS public."custom.type" COLLATE "C""#)]
fn declarations_gate_catalog_publication(
    #[case] sql: &str,
    #[values(Some(true), Some(false), None)] fact: Option<bool>,
) -> testresult::TestResult {
    let pair = apply_ownership_sql(context(fact), sql)?;
    let name = FullName::with_schema("public", "rejected");
    assert_eq!(
        pair.context().get_type_impl(&name).is_some(),
        fact == Some(true)
    );
    assert_eq!(
        pair.context().problems().len(),
        usize::from(fact != Some(true))
    );
    let validation = pair.ast().first_declaration_collation();
    assert_eq!(validation.can_apply(), fact == Some(true));
    assert_eq!(
        validation.lookup().unwrap().resolved_name(),
        Some(&FullName::with_schema("pg_catalog", "C"))
    );
    let pair = apply_ownership_sql(
        pair.into_context(),
        "CREATE TABLE public.later(value integer)",
    )?;
    assert!(
        pair.context()
            .get_table(&TableName::from(FullName::with_schema("public", "later")))
            .is_some()
    );
    Ok(())
}

#[rstest::rstest]
#[case::table(r#"CREATE TABLE public.rejected(ok text, value integer COLLATE missing)"#)]
#[case::composite(r#"CREATE TYPE public.rejected AS (ok text, value integer COLLATE missing)"#)]
#[case::domain(r#"CREATE DOMAIN public.rejected AS integer COLLATE missing"#)]
fn missing_collation_leaves_no_definition(#[case] sql: &str) -> testresult::TestResult {
    let pair = apply_ownership_sql(context(Some(true)), sql)?;
    assert!(
        pair.context()
            .get_type_impl(&FullName::with_schema("public", "rejected"))
            .is_none()
    );
    assert_eq!(pair.context().problems().len(), 1);
    assert_eq!(pair.context().problems()[0].code(), "collation_not_found");
    Ok(())
}

#[test]
fn rejected_add_column_is_atomic_and_does_not_enter_unimplemented_application()
-> testresult::TestResult {
    let pair = apply_ownership_sql(context(Some(false)), "CREATE TABLE public.t(value integer)")?;
    let pair = apply_ownership_sql(
        pair.into_context(),
        r#"ALTER TABLE public.t ADD COLUMN ok text, ADD COLUMN bad integer COLLATE "C""#,
    )?;
    assert!(!pair.ast().alter_table_statement().unwrap().can_apply());
    let table = pair
        .context()
        .get_table(&TableName::from(FullName::with_schema("public", "t")))
        .unwrap();
    assert!(table.get_column_at(0).is_some());
    assert!(table.get_column_at(1).is_none());
    assert_eq!(pair.context().problems().len(), 1);
    Ok(())
}

#[rstest::rstest]
#[case::table(r#"CREATE TABLE public.rejected(value integer COLLATE "C")"#)]
#[case::composite(r#"CREATE TYPE public.rejected AS (value integer COLLATE "C")"#)]
#[case::domain(r#"CREATE DOMAIN public.rejected AS integer COLLATE "C""#)]
fn application_uses_retained_outcome_without_diagnostic_collection(
    #[case] sql: &str,
) -> testresult::TestResult {
    let pair = apply_ownership_sql(context(Some(false)), sql)?;
    assert!(!pair.ast().first_declaration_collation().can_apply());
    // A fresh application context contains none of the analysis diagnostics.
    let applied = pair.ast().apply_declaration(context(Some(false)))?;
    assert!(applied.problems().is_empty());
    assert!(
        applied
            .get_type_impl(&FullName::with_schema("public", "rejected"))
            .is_none()
    );
    Ok(())
}

#[test]
fn public_composite_analysis_entry_point_validates_fields() -> testresult::TestResult {
    use crate::{sem::CreateCompositeType, syn::Opt, test_helpers::SynBuilder};
    let syntax =
        SynBuilder::new().parse(r#"CREATE TYPE public.rejected AS (value integer COLLATE "C")"#);
    let pair = CreateCompositeType::analyze_create_comp_type(
        context(Some(false)),
        &None,
        syntax.as_composite_type_stmt().as_inner().unwrap(),
    )?;
    assert!(
        pair.context()
            .get_type_impl(&FullName::with_schema("public", "rejected"))
            .is_none()
    );
    assert_eq!(pair.context().problems()[0].code(), "collate_operand_type");
    Ok(())
}

#[rstest::rstest]
#[case::table(
    r#"CREATE TABLE public.accepted(value varchar(12) COLLATE "collation.schema"."Mixed.Name")"#
)]
#[case::composite(
    r#"CREATE TYPE public.accepted AS (value varchar(12) COLLATE "collation.schema"."Mixed.Name")"#
)]
#[case::domain(
    r#"CREATE DOMAIN public.accepted AS varchar(12) COLLATE "collation.schema"."Mixed.Name""#
)]
fn modifiers_and_qualified_collation_survive_analysis(#[case] sql: &str) -> testresult::TestResult {
    let pair = apply_ownership_sql(context(Some(true)), sql)?;
    assert!(
        pair.context().problems().is_empty(),
        "{:?}",
        pair.context().problems()
    );
    assert!(
        pair.context()
            .get_type_impl(&FullName::with_schema("public", "accepted"))
            .is_some()
    );
    let validation = pair.ast().first_declaration_collation();
    assert!(validation.can_apply());
    assert_eq!(
        validation.lookup().unwrap().resolved_name(),
        Some(&FullName::with_schema("collation.schema", "Mixed.Name"))
    );
    let requested = pair.ast().first_requested_type();
    assert_eq!(
        requested.value(),
        &crate::sem::TypeReference::from_master_type_name("varchar")
    );
    assert_eq!(requested.arguments(), &["12"]);
    assert_eq!(requested.typmod(), Some(16));
    Ok(())
}

#[rstest::rstest]
#[case::table(
    "CREATE TABLE public.rejected(value public.absent COLLATE public.missing)",
    "column_type_not_found"
)]
#[case::composite(
    "CREATE TYPE public.rejected AS (value public.absent COLLATE public.missing)",
    "column_type_not_found"
)]
#[case::domain(
    "CREATE DOMAIN public.rejected AS public.absent COLLATE public.missing",
    "SF000001"
)]
fn missing_type_is_primary_and_rejected_ddl_remains_inspectable(
    #[case] sql: &str,
    #[case] code: &str,
) -> testresult::TestResult {
    let pair = apply_ownership_sql(context(Some(true)), sql)?;
    assert_eq!(pair.context().problems().len(), 1);
    assert_eq!(pair.context().problems()[0].code(), code);
    let validation = pair.ast().first_declaration_collation();
    assert!(!validation.can_apply());
    assert!(validation.lookup().unwrap().is_missing());
    assert!(
        pair.context()
            .get_type_impl(&FullName::with_schema("public", "rejected"))
            .is_none()
    );
    Ok(())
}
