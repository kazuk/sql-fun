#[path = "test_helpers.rs"]
mod test_helpers;

use self::test_helpers::resolved;
use super::realize_cte_catalog;
use ::sql_fun_sqlast::{
    ParseStatementError, PlaceholderedSqlRenderError,
    ast::{AstMutationError, Dialect},
    sem::ParseContext,
};

#[rstest::rstest]
#[case::no_ctes("select 1", &[], &[])]
#[case::no_ctes_ddl("create table t (id integer)", &[], &[])]
#[case::one("select * from chosen", &[("chosen", "select 1")], &["chosen"])]
#[case::dependencies("select * from leaf", &[("root", "select 1 as id"), ("leaf", "select id from root")], &["root", "leaf"])]
#[case::existing("with existing as (select * from chosen) select * from existing", &[("chosen", "select 1")], &["chosen", "existing"])]
#[case::recursive_existing("with recursive existing as (select 1) select * from existing", &[("chosen", "select 2")], &["chosen", "existing"])]
fn preserves_cte_structure(
    #[case] base: &str,
    #[case] ctes: &[(&str, &str)],
    #[case] names: &[&str],
) {
    let result =
        realize_cte_catalog(&resolved(base, ctes), Dialect::postgres()).expect("realized query");
    let actual = result
        .statement()
        .with_clause()
        .map(|with| {
            with.iter()
                .map(|cte| cte.name().to_owned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert_eq!(actual, names);
    assert_eq!(
        result
            .statement()
            .with_clause()
            .is_some_and(|with| with.is_recursive()),
        base.starts_with("with recursive")
    );
}

#[rstest::rstest]
#[case::empty("")]
#[case::multiple("select 1; select 2")]
#[case::trailing_error("select 1; select (")]
fn rejects_invalid_statement_count_or_parse(#[case] sql: &str) {
    assert!(realize_cte_catalog(&resolved(sql, &[]), Dialect::postgres()).is_err());
}

#[rstest::rstest]
#[case::collision("with chosen as (select 1) select * from chosen", "chosen", "select 2")]
#[case::invalid_body("select 1", "chosen", "create table x (id integer)")]
#[case::invalid_alias("select 1", "", "select 2")]
#[case::unsupported_carrier("create table x (id integer)", "chosen", "select 2")]
#[case::nested_dml(
    "select 1",
    "chosen",
    "with changed as (delete from t returning id) select * from changed"
)]
fn preserves_mutation_error(#[case] base: &str, #[case] alias: &str, #[case] body: &str) {
    let error = realize_cte_catalog(&resolved(base, &[(alias, body)]), Dialect::postgres())
        .expect_err("mutation must fail");
    assert!(
        error.source.downcast_ref::<AstMutationError>().is_some(),
        "{error}"
    );
}

#[test]
fn preserves_existing_declaration_attributes() {
    let result = realize_cte_catalog(
        &resolved(
            "with existing(id) as materialized (select 1) select * from existing",
            &[("chosen", "select 2")],
        ),
        Dialect::postgres(),
    )
    .unwrap();
    let with = result.statement().with_clause().unwrap();
    let existing = with.find("existing").unwrap();
    assert_eq!(existing.columns().collect::<Vec<_>>(), ["id"]);
    assert_eq!(existing.materialized(), Some(true));
}

#[test]
fn accepts_postgres_modifying_body() {
    let result = realize_cte_catalog(
        &resolved(
            "select * from changed",
            &[("changed", "delete from t returning id")],
        ),
        Dialect::postgres(),
    )
    .unwrap();
    assert_eq!(
        result
            .statement()
            .with_clause()
            .unwrap()
            .find("changed")
            .unwrap()
            .body_kind(),
        ::sql_fun_sqlast::ast::StatementKind::delete()
    );
}

#[rstest::rstest]
#[case::base("select 1; select 2", &[])]
#[case::cte("select 1", &[("chosen", "select 1; select 2")])]
#[case::empty_cte("select 1", &[("chosen", "")])]
fn rejects_each_invalid_piece(#[case] base: &str, #[case] ctes: &[(&str, &str)]) {
    assert!(realize_cte_catalog(&resolved(base, ctes), Dialect::postgres()).is_err());
}

#[test]
fn retains_render_and_parse_sources() {
    let render =
        realize_cte_catalog(&resolved("select #[missing]", &[]), Dialect::postgres()).unwrap_err();
    assert!(
        render
            .source
            .downcast_ref::<PlaceholderedSqlRenderError>()
            .is_some()
    );
    let parse = realize_cte_catalog(
        &resolved("select * from chosen", &[("chosen", "select (")]),
        Dialect::postgres(),
    )
    .unwrap_err();
    assert!(parse.source.downcast_ref::<ParseStatementError>().is_some());
    assert!(parse.to_string().contains("chosen"));
}

#[rstest::rstest]
#[case::postgres(Dialect::postgres())]
#[cfg_attr(feature = "sqlite", case::sqlite(Dialect::sqlite()))]
fn returns_first_occurrence_bindings(#[case] dialect: Dialect) {
    let catalog = resolved(
        "with existing as (select ${existing:text}, ${shared}) select ${base}, ${shared}, ${existing}",
        &[
            ("first", "select ${shared:int}, ${cte_only}, ${shared}"),
            ("second", "select ${cte_only}, ${later}"),
        ],
    );
    let result = realize_cte_catalog(&catalog, dialect).unwrap();
    let bindings = result.bindings().iter().collect::<Vec<_>>();
    assert_eq!(
        result
            .bindings()
            .parameter_names()
            .into_iter()
            .map(::sql_fun_core::ParameterName::as_str)
            .collect::<Vec<_>>(),
        ["shared", "cte_only", "later", "existing", "base"]
    );
    assert_eq!(bindings[0].metadata().cte_bindings().len(), 1);
    assert_eq!(bindings[3].metadata().query_type_hints().len(), 1);
    assert!(bindings[4].metadata().cte_bindings().is_empty());
    // The actual common AST must retain the shared numbered identities, including
    // gaps in individual pieces. Span offsets are intentionally not asserted.
    let ast = format!("{:?}", result.statement());
    for (index, occurrences) in [(1, 4), (2, 2), (3, 1), (4, 2), (5, 1)] {
        assert_eq!(
            ast.matches(&format!("Parameter {{ position: {index},"))
                .count(),
            occurrences,
            "{ast}"
        );
    }
}

#[test]
fn errors_when_required_catalog_metadata_is_absent() {
    let mut queries = ::indexmap::IndexMap::new();
    queries.insert(
        ::sql_fun_core::CteAlias::new("chosen"),
        ::sql_fun_core::PlaceholderedSql::from_query("select ${missing}"),
    );
    let catalog = ::sql_fun_core::ResolvedCteCatalog::new(
        &::sql_fun_core::PlaceholderedSql::from_query("select 1"),
        &::sql_fun_core::CteParameterCollection::default(),
        &queries,
    );
    let error = realize_cte_catalog(&catalog, Dialect::postgres()).unwrap_err();
    assert!(error.to_string().contains("missing"));
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::modifying("select * from chosen", "chosen", "delete from t returning id")]
#[case::collision("with CHOSEN as (select 1) select * from chosen", "chosen", "select 2")]
fn rejects_sqlite_mutations(#[case] base: &str, #[case] alias: &str, #[case] body: &str) {
    let error =
        realize_cte_catalog(&resolved(base, &[(alias, body)]), Dialect::sqlite()).unwrap_err();
    assert!(error.source.downcast_ref::<AstMutationError>().is_some());
}

#[cfg(feature = "sqlite")]
#[test]
fn realizes_sqlite_dependency_closure() {
    let catalog = resolved(
        "with existing as (select * from leaf) select * from existing",
        &[("root", "select 1 as id"), ("leaf", "select id from root")],
    );
    let result = realize_cte_catalog(&catalog, Dialect::sqlite()).unwrap();
    assert_eq!(
        result
            .statement()
            .with_clause()
            .unwrap()
            .iter()
            .map(|cte| cte.name().to_owned())
            .collect::<Vec<_>>(),
        ["root", "leaf", "existing"]
    );
}

#[test]
fn keeps_distinct_resolved_names_including_renamed_parameters() {
    let catalog = resolved(
        "select ${renamed}, ${ID}, ${id}",
        &[("chosen", "select ${id}, ${renamed}, ${id}")],
    );
    let result = realize_cte_catalog(&catalog, Dialect::postgres()).unwrap();
    assert_eq!(
        result
            .bindings()
            .parameter_names()
            .into_iter()
            .map(::sql_fun_core::ParameterName::as_str)
            .collect::<Vec<_>>(),
        ["id", "renamed", "ID"]
    );
    let renamed = result.bindings().iter().nth(1).unwrap();
    assert_eq!(renamed.metadata().cte_bindings().len(), 1);
}

#[rstest::rstest]
#[case::absent("shared", "shared")]
#[case::matching("shared:int4", "shared:int4")]
#[case::conflicting("shared:uuid", "shared:text")]
fn realized_query_analysis_ignores_parameter_hints(
    #[case] cte_parameter: &str,
    #[case] base_parameter: &str,
) {
    let project = crate::schema_cache::test_helpers::SchemaProject::new();
    let cte = format!("select ${{{cte_parameter}}} + 1 as n");
    let base = format!("select n, ${{{base_parameter}}} + 2 as m from chosen");
    let realized =
        realize_cte_catalog(&resolved(&base, &[("chosen", &cte)]), Dialect::postgres()).unwrap();
    let analyzed = realized
        .statement()
        .analyze(project.analysis_context("products"))
        .unwrap();
    let inputs = analyzed.context().parameter_constraints();
    assert_eq!(
        inputs
            .iter()
            .map(::sql_fun_sqlast::sem::ParameterConstraint::position)
            .collect::<Vec<_>>(),
        [1]
    );
    let input = inputs.parameter(1).unwrap().constraint();
    assert!(input.allows(&::sql_fun_sqlast::sem::PgBuiltInType::int4()));
    assert!(!input.allows(&::sql_fun_sqlast::sem::PgBuiltInType::text()));
    assert_eq!(realized.bindings().iter().count(), 1);
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(::sql_fun_sqlast::sem::PgBuiltInType::int4())
    );
}
