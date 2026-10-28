mod test_helpers;

use self::test_helpers::{context, expression, query};
use crate::ast::Dialect;
use crate::sem::{CatalogNameLookup, FullName, ParseContext, PgBuiltInType, SemScalarExpr};
use std::collections::BTreeMap;

#[rstest::rstest]
#[case::local(r#"SELECT 'x' COLLATE "C""#, FullName::from_local_name("C"))]
#[case::qualified(
    r#"SELECT 'x' COLLATE pg_catalog."C""#,
    FullName::with_schema("pg_catalog", "C")
)]
fn retains_completed_postgres_binding(
    #[case] sql: &str,
    #[case] requested: FullName,
) -> testresult::TestResult {
    let mut context = context(Dialect::postgres());
    let query = query(sql, &mut context)?;
    let value = expression(&query);
    let collate = value.as_collate().unwrap();
    assert_eq!(collate.collation().requested(), &requested);
    assert_eq!(
        collate.collation().resolved_name(),
        Some(&FullName::with_schema("pg_catalog", "C"))
    );
    assert!(collate.argument().as_const().is_some());
    assert!(collate.source_span().is_some());
    let encoded = ::serde_json::to_string(value)?;
    let restored: SemScalarExpr = ::serde_json::from_str(&encoded)?;
    assert_eq!(value, &restored);
    assert_eq!(context.reported_problem_count(), 0);
    Ok(())
}

#[test]
fn retains_missing_inner_binding_and_successful_outer_binding() -> testresult::TestResult {
    let mut context = context(Dialect::postgres());
    let query = query(
        r#"SELECT ('x' COLLATE absent484) COLLATE "C""#,
        &mut context,
    )?;
    let outer = expression(&query).as_collate().unwrap();
    let inner = outer.argument().as_collate().unwrap();
    assert!(inner.collation().is_missing());
    assert_eq!(inner.collation().requested().to_string(), "absent484");
    assert!(outer.collation().resolved_name().is_some());
    assert!(context.has_reported_problem("collation_not_found"));
    assert_eq!(context.reported_problem_count(), 1);
    Ok(())
}

#[test]
fn retains_ambiguous_lookup() -> testresult::TestResult {
    let mut context = context(Dialect::postgres());
    let name = FullName::from_local_name("conflicting");
    let candidates = [
        FullName::with_schema("one", "conflicting"),
        FullName::with_schema("two", "conflicting"),
    ];
    context.set_collation_lookup(&name, &CatalogNameLookup::ambiguous(&name, &candidates));
    let query = query("SELECT 'x' COLLATE conflicting", &mut context)?;
    assert_eq!(
        expression(&query)
            .as_collate()
            .unwrap()
            .collation()
            .ambiguous_candidates(),
        Some(candidates.as_slice())
    );
    assert!(context.has_reported_problem("collation_name_ambiguous"));
    Ok(())
}

#[rstest::rstest]
#[case::integer(r#"SELECT 1 COLLATE "C""#, true)]
#[case::null(r#"SELECT NULL COLLATE "C""#, false)]
#[case::text(r#"SELECT 'x'::text COLLATE "C""#, false)]
#[case::unknown(r#"SELECT 'x' COLLATE "C""#, false)]
#[case::deferred(r#"SELECT $1 COLLATE "C", $1::integer"#, true)]
fn checks_applicability_when_type_is_known(
    #[case] sql: &str,
    #[case] invalid: bool,
) -> testresult::TestResult {
    let mut context = context(Dialect::postgres());
    let query = query(sql, &mut context)?;
    assert!(expression(&query).as_collate().is_some());
    assert_eq!(
        context.has_reported_problem("collate_operand_type"),
        invalid
    );
    Ok(())
}

#[rstest::rstest]
#[case::direct(r#"SELECT $1 COLLATE "C", $1::text"#)]
#[case::nested(r#"SELECT ($1 COLLATE "C") COLLATE "POSIX", $1::text"#)]
#[case::consumed(r#"SELECT ($1 COLLATE "C")::text"#)]
fn operand_constraints_survive_annotations(#[case] sql: &str) -> testresult::TestResult {
    let mut context = context(Dialect::postgres());
    let query = query(sql, &mut context)?;
    assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::text()));
    assert!(
        context
            .parameter_constraints()
            .allows(&BTreeMap::from([(1, PgBuiltInType::text())]))
    );
    assert!(
        !context
            .parameter_constraints()
            .allows(&BTreeMap::from([(1, PgBuiltInType::bool())]))
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::builtin("SELECT 'x' COLLATE NOCASE")]
#[case::custom("SELECT 'x' COLLATE absent484")]
#[case::integer("SELECT 1 COLLATE BINARY")]
#[case::null("SELECT NULL COLLATE RTRIM")]
fn sqlite_missing_names_are_static_diagnostics(#[case] sql: &str) -> testresult::TestResult {
    let mut context = context(Dialect::sqlite());
    let query = query(sql, &mut context)?;
    let value = expression(&query).as_collate().unwrap();
    assert!(value.collation().is_missing());
    assert!(value.source_span().is_none());
    assert!(context.has_reported_problem("collation_not_found"));
    assert!(!context.has_reported_problem("collate_operand_type"));
    // SQLite can execute these projections without consulting a comparator.
    // Static resolution intentionally diagnoses against loaded metadata instead.
    let connection = ::rusqlite::Connection::open_in_memory()?;
    connection.prepare(sql)?.query([])?.next()?.unwrap();
    Ok(())
}

#[rstest::rstest]
#[case::literal(r#"SELECT 'x' COLLATE "C""#)]
#[case::null(r#"SELECT NULL COLLATE "C""#)]
#[case::typed_null(r#"SELECT NULL::text COLLATE "C""#)]
#[case::missing_operand(r#"SELECT absent484 COLLATE "C""#)]
fn preserves_operand_type_and_nullability(#[case] sql: &str) -> testresult::TestResult {
    let mut context = context(Dialect::postgres());
    let query = query(sql, &mut context)?;
    let value = expression(&query);
    let argument = value.as_collate().unwrap().argument();
    assert_eq!(value.get_type(), argument.get_type());
    assert_eq!(value.is_not_null(), argument.is_not_null());
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_resolves_every_nested_reference_and_continues() -> testresult::TestResult {
    let mut context = context(Dialect::sqlite());
    let first = query("SELECT ('x' COLLATE NOCASE) COLLATE BINARY", &mut context)?;
    let outer = expression(&first).as_collate().unwrap();
    assert!(outer.collation().is_missing());
    assert!(
        outer
            .argument()
            .as_collate()
            .unwrap()
            .collation()
            .is_missing()
    );
    assert_eq!(context.reported_problem_count(), 2);
    let next = query("SELECT 1", &mut context)?;
    assert_eq!(expression(&next).get_type(), Some(PgBuiltInType::int4()));
    Ok(())
}
