//! Public analyzer regressions over a small in-memory catalog.

mod test_helpers;

use crate::{
    ast::Dialect,
    parse_statements,
    sem::{ParseContext, PgBuiltInType},
};
use std::collections::BTreeMap;

#[rstest::rstest]
#[case::numeric_index("SELECT (NULL::integer[])[1.0::numeric]")]
#[case::numeric_bound("SELECT (NULL::integer[])[1.0::numeric:2]")]
fn postgres_assignment_subscripts_are_legal(
    #[case] sql: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // The fixture explicitly supplies numeric-to-int4 assignment coercion.
    let statement = parse_statements(sql, Dialect::postgres()).next().unwrap()?;
    let analyzed = statement.analyze(test_helpers::context())?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    Ok(())
}

#[rstest::rstest]
#[case::index("SELECT ($1::integer[])[$2] AS n", false, 0)]
#[case::slice("SELECT ($1::integer[])[$2:$3] AS n", true, 0)]
#[case::upper_only("SELECT ($1::integer[])[:$2] AS n", true, 0)]
#[case::lower_only("SELECT ($1::integer[])[$2:] AS n", true, 0)]
#[case::repeated_base("SELECT ($1)[$2] AS n, $1::integer[]", false, 0)]
#[case::repeated_base_first("SELECT $1::integer[], ($1)[$2] AS n", false, 1)]
fn parameterized_array_access_links_results(
    #[case] sql: &str,
    #[case] array_result: bool,
    #[case] column_index: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    // Only int4 identity and numeric-to-int4 assignment are needed here.
    let statement = parse_statements(sql, Dialect::postgres()).next().unwrap()?;
    let analyzed = statement.analyze(test_helpers::context())?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    let expected = crate::sem::TypeReference::concrete_type_ref(
        PgBuiltInType::int4().full_name(),
        array_result,
    );
    assert!(
        constraints
            .result_column(column_index)
            .unwrap()
            .allows(&expected)
    );
    assert!(!constraints.result_column(column_index).unwrap().is_any());
    assert!(constraints.allows(&BTreeMap::from([(2, PgBuiltInType::numeric())])));
    assert!(!constraints.allows(&BTreeMap::from([(2, PgBuiltInType::bool())])));
    Ok(())
}

#[rstest::rstest]
#[case::index("SELECT ($1)[$2]")]
#[case::slice("SELECT ($1)[$2:$3]")]
fn underconstrained_array_access_preserves_structure(
    #[case] sql: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Assignment coercion constrains bounds without selecting a base type.
    let analyzed = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(test_helpers::context())?;
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.parameter(1).unwrap().constraint().is_any());
    assert!(constraints.result_column(0).unwrap().is_any());
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    let array =
        crate::sem::TypeReference::concrete_type_ref(PgBuiltInType::int4().full_name(), true);
    assert!(constraints.allows(&BTreeMap::from([(1, array)])));
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let expression = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap();
    assert!(expression.as_indirection().is_some());
    let domains = test_helpers::serialized_parameter_domains(&::serde_json::to_value(expression)?);
    assert_eq!(domains.len(), constraints.iter().len());
    assert!(domains[0].is_any());
    assert!(!domains[1].is_any());
    assert!(domains[1].allows(&PgBuiltInType::numeric()));
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &None
    );
    Ok(())
}

#[rstest::rstest]
#[case::invalid_index("SELECT ($1::integer[])[$2::boolean]")]
#[case::invalid_slice("SELECT ($1::integer[])[$2::boolean:]")]
#[case::scalar_base("SELECT ($1::integer)[$2]")]
#[case::constant_index("SELECT ($1)[true]")]
fn invalid_parameterized_access_retains_conflicts(
    #[case] sql: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // The fixture supplies numeric assignment but no boolean assignment to int4.
    let analyzed = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(test_helpers::context())?;
    assert!(!analyzed.context().problems().is_empty());
    assert!(
        !analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::new())
    );
    Ok(())
}

#[rstest::rstest]
#[case::operator("SELECT ($1)[$2] + 1 AS n")]
#[case::cast_result("SELECT (($1)[$2])::integer AS n")]
#[case::slice_then_index("SELECT (($1::integer[])[$2:$3])[$4] AS n")]
fn surrounding_context_constrains_array_access(
    #[case] sql: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // One integer addition signature and identity casts constrain the result.
    let analyzed = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(test_helpers::context())?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    let array =
        crate::sem::TypeReference::concrete_type_ref(PgBuiltInType::int4().full_name(), true);
    assert!(constraints.allows(&BTreeMap::from([(1, array)])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    assert!(
        constraints
            .result_column(0)
            .unwrap()
            .allows(&PgBuiltInType::int4())
    );
    assert!(!constraints.result_column(0).unwrap().is_any());
    Ok(())
}

#[rstest::rstest]
#[case::forward("select ($1::public.point_value).n", 0)]
#[case::deferred("select ($1).n, $1::public.point_value", 0)]
#[case::reverse_order("select $1::public.point_value, ($1).n", 1)]
#[case::array_element("select (($1::public.point_value[])[1]).n", 0)]
fn composite_access_propagates_named_field_types(
    #[case] sql: &str,
    #[case] result: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::sem::{FullName, TypeReference};
    // SchemaFileContext supplies composite identity casts from the declared type.
    let definition = parse_statements(
        "create type public.point_value as (n integer)",
        Dialect::postgres(),
    )
    .next()
    .unwrap()?;
    let context = definition.analyze(test_helpers::context())?.into_context();
    let statement = parse_statements(sql, Dialect::postgres()).next().unwrap()?;
    let analyzed = statement.analyze(context)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    let domain = constraints.result_column(result).unwrap();
    assert!(!domain.is_any());
    assert!(domain.allows(&PgBuiltInType::int4()));
    assert!(!domain.allows(&PgBuiltInType::text()));
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let expression = query
        .result_columns()
        .get_column_at(result)
        .unwrap()
        .expression()
        .unwrap();
    assert_eq!(expression.get_type(), Some(PgBuiltInType::int4()));
    let access = expression.as_indirection().expect("retained member access");
    let crate::sem::IndirectionExpr::MemberRef(member) = access else {
        panic!("member binding");
    };
    assert_eq!(member.member(), "n");
    assert!(member.base().get_type().is_some());
    let base = TypeReference::concrete_type_ref(
        &FullName::with_schema("public", "point_value"),
        sql.contains("[]"),
    );
    assert!(constraints.allows(&BTreeMap::from([(1, base)])));
    Ok(())
}

#[rstest::rstest]
#[case::unbounded("select ($1).n", true)]
#[case::result_constrained("select (($1).n)::integer", false)]
fn deferred_composite_accepts_owned_anonymous_definitions(
    #[case] sql: &str,
    #[case] unbounded_result: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::sem::{
        ColumnCollection, ColumnDefinition, ColumnName, TypeDefinition, TypeReference,
    };
    // The result-context case needs only an int4 identity cast.
    let statement = parse_statements(sql, Dialect::postgres()).next().unwrap()?;
    let analyzed = statement.analyze(test_helpers::context())?;
    let constraints = analyzed.context().parameter_constraints().clone();
    drop(analyzed);
    assert!(constraints.parameter(1).unwrap().constraint().is_any());
    assert_eq!(
        constraints.result_column(0).unwrap().is_any(),
        unbounded_result
    );
    let record = |name: &str, ty: &TypeReference| {
        TypeReference::dynamic(&TypeDefinition::ImplicitComposite(ColumnCollection::new(
            vec![ColumnDefinition::new(
                &Some(ColumnName::from(name)),
                Some(ty),
                None,
                &[],
            )],
        )))
    };
    assert!(constraints.allows(&BTreeMap::from([(1, record("n", &PgBuiltInType::int4()))])));
    assert!(!constraints.allows(&BTreeMap::from([(
        1,
        record("other", &PgBuiltInType::int4())
    )])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    Ok(())
}

#[rstest::rstest]
#[case::mutable(false)]
#[case::frozen(true)]
fn nested_composite_and_table_fields_use_owned_definitions(
    #[case] frozen: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::sem::{FullName, TypeReference};
    // Production schema layers provide nested field definitions and row types.
    let context = test_helpers::schema(
        "create type public.inner_value as (n integer); create type public.outer_value as (v public.inner_value); create table public.table_value (v public.inner_value);",
        frozen,
    )?;
    let statement = parse_statements(
        "select (($1::public.outer_value).v).n, (($2::public.table_value).v).n",
        Dialect::postgres(),
    )
    .next()
    .unwrap()?;
    let analyzed = statement.analyze(context)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints().clone();
    drop(analyzed);
    for column in 0..2 {
        let domain = constraints.result_column(column).unwrap();
        assert!(!domain.is_any());
        assert!(domain.allows(&PgBuiltInType::int4()));
    }
    assert!(constraints.allows(&BTreeMap::from([
        (
            1,
            TypeReference::from_full_name(FullName::with_schema("public", "outer_value"))
        ),
        (
            2,
            TypeReference::from_full_name(FullName::with_schema("public", "table_value"))
        ),
    ])));
    Ok(())
}

#[rstest::rstest]
#[case::array_base(
    "select ($1::public.point_value[]).n",
    "member_access_requires_composite_type"
)]
#[case::missing_member(
    "select ($1).missing, $1::public.point_value",
    "composite_type_field_not_found"
)]
#[case::scalar_base("select ($1).n, $1::integer", "member_access_requires_composite_type")]
fn deferred_member_access_retains_conflicts(
    #[case] sql: &str,
    #[case] diagnostic: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let context = test_helpers::schema("create type public.point_value as (n integer)", false)?;
    let analyzed = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(context)?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code() == diagnostic),
        "{:?}",
        analyzed.context().problems()
    );
    assert!(
        !analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::new())
    );
    Ok(())
}

#[rstest::rstest]
#[case::missing_member(
    "select (($1).v).missing, $1::public.outer_value",
    "composite_type_field_not_found"
)]
#[case::scalar_base(
    "select ((($1).v).n).missing, $1::public.outer_value",
    "member_access_requires_composite_type"
)]
fn nested_deferred_members_report_structured_errors(
    #[case] sql: &str,
    #[case] diagnostic: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Declared nested types exercise deferred member resolution.
    let context = test_helpers::schema(
        "create type public.inner_value as (n integer); create type public.outer_value as (v public.inner_value);",
        false,
    )?;
    let analyzed = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(context)?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code() == diagnostic),
        "{:?}",
        analyzed.context().problems()
    );
    assert!(
        !analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::new())
    );
    Ok(())
}

#[test]
fn owned_named_composites_preserve_nominal_identity() -> Result<(), Box<dyn std::error::Error>> {
    use crate::sem::{BaseParseContext, FullName, TypeReference};
    // Schema cast metadata distinguishes named composites with identical fields.
    let context = test_helpers::schema(
        "create type public.first_value as (n integer); create type public.second_value as (n integer);",
        false,
    )?;
    let first = FullName::with_schema("public", "first_value");
    let second = FullName::with_schema("public", "second_value");
    let owned_first = TypeReference::dynamic(context.get_type(&first).unwrap());
    let owned_second = TypeReference::dynamic(context.get_type(&second).unwrap());
    let analyzed = parse_statements("select ($1).n, $1::public.first_value", Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(context)?;
    let constraints = analyzed.context().parameter_constraints().clone();
    drop(analyzed);
    assert!(constraints.allows(&BTreeMap::from([(1, owned_first)])));
    assert!(!constraints.allows(&BTreeMap::from([(1, owned_second)])));
    assert!(!constraints.allows(&BTreeMap::from([(
        1,
        TypeReference::from_full_name(second)
    )])));
    Ok(())
}

#[rstest::rstest]
#[case::array_member("(NULL::public.array_value).items", true)]
#[case::array_index("((NULL::public.array_value).items)[1]", false)]
#[case::scalar_member("(NULL::public.array_value).scalar", false)]
fn declared_composite_array_fields_retain_result_types(
    #[case] expression: &str,
    #[case] is_array: bool,
    #[values(false, true)] frozen: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let context = test_helpers::schema(
        "CREATE TYPE public.array_value AS (items integer[], scalar integer)",
        frozen,
    )?;
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    let analyzed = parse_statements(&format!("SELECT {expression}"), Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(context)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(crate::sem::TypeReference::concrete_type_ref(
            PgBuiltInType::int4().full_name(),
            is_array,
        ))
    );
    Ok(())
}

#[rstest::rstest]
fn declared_composite_scalar_field_rejects_subscripts(
    #[values(false, true)] frozen: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let context = test_helpers::schema(
        "CREATE TYPE public.array_value AS (items integer[], scalar integer)",
        frozen,
    )?;
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    let analyzed = parse_statements(
        "SELECT ((NULL::public.array_value).scalar)[1]",
        Dialect::postgres(),
    )
    .next()
    .unwrap()?
    .analyze(context)?;
    assert_eq!(analyzed.context().problems().len(), 1);
    assert_eq!(analyzed.context().problems()[0].code(), "require_array");
    Ok(())
}
