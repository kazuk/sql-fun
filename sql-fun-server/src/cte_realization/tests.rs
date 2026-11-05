use ::sql_fun_sqlast::{
    ParseStatementError, PlaceholderedSqlRenderError,
    ast::{AstMutationError, Dialect},
    context::ParseContext,
    sem::SemAst,
};
#[path = "test_helpers.rs"]
mod test_helpers;

use crate::{
    cte_realization::{ParameterHintConflict, realize_cte_catalog},
    schema_cache::test_helpers::SchemaProject,
    test_helpers::create_cache_context,
};

use self::test_helpers::resolved;

use ::sql_fun_core::PlaceholderedSql;

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
    let result = realize_cte_catalog(
        &PlaceholderedSql::from_query(base),
        &resolved(ctes),
        Dialect::postgres(),
    )
    .expect("realized query");
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
    assert!(
        realize_cte_catalog(
            &PlaceholderedSql::from_query(sql),
            &resolved(&[]),
            Dialect::postgres()
        )
        .is_err()
    );
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
    let error = realize_cte_catalog(
        &PlaceholderedSql::from_query(base),
        &resolved(&[(alias, body)]),
        Dialect::postgres(),
    )
    .expect_err("mutation must fail");
    assert!(
        error.source.downcast_ref::<AstMutationError>().is_some(),
        "{error}"
    );
}

#[test]
fn preserves_existing_declaration_attributes() {
    let result = realize_cte_catalog(
        &PlaceholderedSql::from_query(
            "with existing(id) as materialized (select 1) select * from existing",
        ),
        &resolved(&[("chosen", "select 2")]),
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
        &PlaceholderedSql::from_query("select * from changed"),
        &resolved(&[("changed", "delete from t returning id")]),
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
    assert!(
        realize_cte_catalog(
            &PlaceholderedSql::from_query(base),
            &resolved(ctes),
            Dialect::postgres()
        )
        .is_err()
    );
}

#[test]
fn retains_render_and_parse_sources() {
    let render = realize_cte_catalog(
        &PlaceholderedSql::from_query("select #[missing]"),
        &resolved(&[]),
        Dialect::postgres(),
    )
    .unwrap_err();
    assert!(
        render
            .source
            .downcast_ref::<PlaceholderedSqlRenderError>()
            .is_some()
    );
    let parse = realize_cte_catalog(
        &PlaceholderedSql::from_query("select * from chosen"),
        &resolved(&[("chosen", "select (")]),
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
    let base_query = PlaceholderedSql::from_query(
        "with existing as (select ${existing:text}, ${shared}) select ${base}, ${shared}, ${existing}",
    );
    let catalog = resolved(&[
        ("first", "select ${shared:int}, ${cte_only}, ${shared}"),
        ("second", "select ${cte_only}, ${later}"),
    ]);
    let result = realize_cte_catalog(&base_query, &catalog, dialect).unwrap();
    assert_eq!(
        result
            .bindings()
            .parameter_names()
            .into_iter()
            .map(::sql_fun_core::ParameterName::as_str)
            .collect::<Vec<_>>(),
        ["shared", "cte_only", "later", "existing", "base"]
    );
    assert_eq!(
        result.bindings().type_hints(),
        [
            Some("int".to_owned()),
            None,
            None,
            Some("text".to_owned()),
            None
        ]
    );
    // SQL emission must preserve shared numbered positions and gaps in each piece.
    let sql = result.statement().deparse_sql(dialect).unwrap();
    for (index, occurrences) in [(1, 4), (2, 2), (3, 1), (4, 2), (5, 1)] {
        assert_eq!(
            test_helpers::parameter_occurrences(&sql, dialect, index),
            occurrences,
            "{sql}"
        );
    }
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::modifying("select * from chosen", "chosen", "delete from t returning id")]
#[case::collision("with CHOSEN as (select 1) select * from chosen", "chosen", "select 2")]
fn rejects_sqlite_mutations(#[case] base: &str, #[case] alias: &str, #[case] body: &str) {
    let error = realize_cte_catalog(
        &PlaceholderedSql::from_query(base),
        &resolved(&[(alias, body)]),
        Dialect::sqlite(),
    )
    .unwrap_err();
    assert!(error.source.downcast_ref::<AstMutationError>().is_some());
}

#[cfg(feature = "sqlite")]
#[test]
fn realizes_sqlite_dependency_closure() {
    let base_query = PlaceholderedSql::from_query(
        "with existing as (select * from leaf) select * from existing",
    );
    let catalog = resolved(&[("root", "select 1 as id"), ("leaf", "select id from root")]);
    let result = realize_cte_catalog(&base_query, &catalog, Dialect::sqlite()).unwrap();
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
    let base_query = PlaceholderedSql::from_query("select ${renamed}, ${ID}, ${id}");
    let catalog = resolved(&[("chosen", "select ${id}, ${renamed}, ${id}")]);
    let result = realize_cte_catalog(&base_query, &catalog, Dialect::postgres()).unwrap();
    assert_eq!(
        result
            .bindings()
            .parameter_names()
            .into_iter()
            .map(::sql_fun_core::ParameterName::as_str)
            .collect::<Vec<_>>(),
        ["id", "renamed", "ID"]
    );
    assert_eq!(result.bindings().type_hints(), [None, None, None]);
}

#[rstest::rstest]
#[case::absent("shared", "shared")]
#[case::matching("shared:int4", "shared:int4")]
#[case::explicit_hint_is_not_inference("shared:uuid", "shared:uuid")]
fn realized_query_analysis_ignores_parameter_hints(
    #[case] cte_parameter: &str,
    #[case] base_parameter: &str,
) {
    let mut cache_context = create_cache_context();
    let project = SchemaProject::new();
    let cte = format!("select ${{{cte_parameter}}} + 1 as n");
    let base = format!("select n, ${{{base_parameter}}} + 2 as m from chosen");
    let realized = realize_cte_catalog(
        &PlaceholderedSql::from_query(&base),
        &resolved(&[("chosen", &cte)]),
        Dialect::postgres(),
    )
    .unwrap();
    let analyzed = SemAst::from_ast(
        realized.statement(),
        project.analysis_context("products"),
        &mut cache_context,
    )
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

#[rstest::rstest]
#[case::postgres(Dialect::postgres(), ::sql_fun_core::SqlDialect::PostgreSQL)]
#[cfg_attr(
    feature = "sqlite",
    case::sqlite(Dialect::sqlite(), ::sql_fun_core::SqlDialect::SQLite)
)]
fn realizes_catalog_diamond_output(
    #[case] dialect: Dialect,
    #[case] core_dialect: ::sql_fun_core::SqlDialect,
) -> ::testresult::TestResult {
    let base_query = PlaceholderedSql::from_query("select ${base}, ${right:text}, ${id} from q1");
    let catalog = test_helpers::resolved_diamond(core_dialect)?;
    let result = realize_cte_catalog(&base_query, &catalog, dialect)?;
    assert_eq!(
        result
            .statement()
            .with_clause()
            .unwrap()
            .iter()
            .map(|cte| cte.name().to_owned())
            .collect::<Vec<_>>(),
        ["shared", "q1", "q2"]
    );
    assert_eq!(
        result
            .bindings()
            .parameter_names()
            .into_iter()
            .map(::sql_fun_core::ParameterName::as_str)
            .collect::<Vec<_>>(),
        ["id", "untyped", "left", "right", "base"]
    );
    assert_eq!(
        result.bindings().type_hints(),
        [
            Some("int".to_owned()),
            None,
            Some("text".to_owned()),
            Some("text".to_owned()),
            None
        ]
    );
    let sql = result.statement().deparse_sql(dialect).unwrap();
    for (position, occurrences) in [(1, 4), (2, 1), (3, 1), (4, 2), (5, 1)] {
        assert_eq!(
            test_helpers::parameter_occurrences(&sql, dialect, position),
            occurrences,
            "{sql}"
        );
    }
    Ok(())
}

#[rstest::rstest]
#[case::absent_postgres("select ${p}, ${p}", &[], None, Dialect::postgres())]
#[cfg_attr(feature = "sqlite", case::absent_sqlite("select ${p}, ${p}", &[], None, Dialect::sqlite()))]
#[case::repeated_base_postgres("select ${p:text}, ${p:text}", &[], Some("text"), Dialect::postgres())]
#[cfg_attr(feature = "sqlite", case::repeated_base_sqlite("select ${p:text}, ${p:text}", &[], Some("text"), Dialect::sqlite()))]
#[case::late_base_hint_postgres("select ${p}, ${p:text}", &[], Some("text"), Dialect::postgres())]
#[cfg_attr(feature = "sqlite", case::late_base_hint_sqlite("select ${p}, ${p:text}", &[], Some("text"), Dialect::sqlite()))]
#[case::base_hint_postgres("select ${p:text}", &[("first", "select ${p}")], Some("text"), Dialect::postgres())]
#[cfg_attr(feature = "sqlite", case::base_hint_sqlite("select ${p:text}", &[("first", "select ${p}")], Some("text"), Dialect::sqlite()))]
#[case::cte_hint_postgres("select ${p}", &[("first", "select ${p:text}, ${p}")], Some("text"), Dialect::postgres())]
#[cfg_attr(feature = "sqlite", case::cte_hint_sqlite("select ${p}", &[("first", "select ${p:text}, ${p}")], Some("text"), Dialect::sqlite()))]
#[case::repeated_ctes_postgres("select ${p:text}", &[("first", "select ${p:text}"), ("second", "select ${p:text}")], Some("text"), Dialect::postgres())]
#[cfg_attr(feature = "sqlite", case::repeated_ctes_sqlite("select ${p:text}", &[("first", "select ${p:text}"), ("second", "select ${p:text}")], Some("text"), Dialect::sqlite()))]
fn returns_one_hint_per_logical_parameter(
    #[case] base: &str,
    #[case] ctes: &[(&str, &str)],
    #[case] hint: Option<&str>,
    #[case] dialect: Dialect,
) {
    let result = realize_cte_catalog(
        &PlaceholderedSql::from_query(base),
        &resolved(ctes),
        dialect,
    )
    .unwrap();
    assert_eq!(
        result
            .bindings()
            .parameter_names()
            .into_iter()
            .map(::sql_fun_core::ParameterName::as_str)
            .collect::<Vec<_>>(),
        ["p"]
    );
    assert_eq!(result.bindings().type_hints(), [hint.map(str::to_owned)]);
}

#[rstest::rstest]
#[case::base_only_postgres("select ${p:uuid}, ${p:text}", &[], Dialect::postgres())]
#[cfg_attr(feature = "sqlite", case::base_only_sqlite("select ${p:uuid}, ${p:text}", &[], Dialect::sqlite()))]
#[case::within_cte_postgres("select 1", &[("first", "select ${p:uuid}, ${p:text}")], Dialect::postgres())]
#[cfg_attr(feature = "sqlite", case::within_cte_sqlite("select 1", &[("first", "select ${p:uuid}, ${p:text}")], Dialect::sqlite()))]
#[case::between_ctes_postgres("select 1", &[("first", "select ${p:uuid}"), ("second", "select ${p:text}")], Dialect::postgres())]
#[cfg_attr(feature = "sqlite", case::between_ctes_sqlite("select 1", &[("first", "select ${p:uuid}"), ("second", "select ${p:text}")], Dialect::sqlite()))]
#[case::cte_and_base_postgres("select ${p:text}", &[("first", "select ${p:uuid}")], Dialect::postgres())]
#[cfg_attr(feature = "sqlite", case::cte_and_base_sqlite("select ${p:text}", &[("first", "select ${p:uuid}")], Dialect::sqlite()))]
fn rejects_conflicting_explicit_hints(
    #[case] base: &str,
    #[case] ctes: &[(&str, &str)],
    #[case] dialect: Dialect,
) {
    let error = realize_cte_catalog(
        &PlaceholderedSql::from_query(base),
        &resolved(ctes),
        dialect,
    )
    .unwrap_err();
    let conflict = error
        .source
        .downcast_ref::<ParameterHintConflict>()
        .expect("structured hint conflict");
    assert_eq!(conflict.name().as_str(), "p");
    assert_eq!(conflict.first_hint(), "uuid");
    assert_eq!(conflict.second_hint(), "text");
}

#[test]
fn realized_bindings_accept_query_scoped_sql_type_hints() {
    use ::sql_fun_sqlast::{
        context::ParameterTypeHintWrite,
        names::FullName,
        sem::{PgBuiltInType, TypeReference},
    };
    let mut cache_context = create_cache_context();

    let project = SchemaProject::new();
    let realized = realize_cte_catalog(
        &PlaceholderedSql::from_query(
            "select n, ${base:kind} as m, ${shared:int4} as repeated from chosen",
        ),
        &resolved(&[("chosen", "select ${shared:int4} as n")]),
        Dialect::postgres(),
    )
    .unwrap();
    assert_eq!(
        realized
            .bindings()
            .parameter_names()
            .into_iter()
            .map(::sql_fun_core::ParameterName::as_str)
            .collect::<Vec<_>>(),
        ["shared", "base"]
    );
    // The caller supplies one optional type per finalized SQL index. Occurrence
    // metadata does not introduce a second numbering or conflict-validation pass.
    let mut context = SemAst::from_ast(
        &::sql_fun_sqlast::parse_statements(
            "CREATE TYPE public.kind AS ENUM ('a')",
            Dialect::postgres(),
        )
        .next()
        .unwrap()
        .unwrap(),
        project.analysis_context("products"),
        &mut cache_context,
    )
    .unwrap()
    .into_context();
    let kind = TypeReference::concrete_type_ref(&FullName::with_schema("public", "kind"), false);
    context
        .setup_typehinting(&[Some("int4"), Some("kind")])
        .unwrap();
    let sql = realized
        .statement()
        .deparse_sql(Dialect::postgres())
        .unwrap();
    let analyzed = SemAst::from_ast(realized.statement(), context, &mut cache_context).unwrap();
    assert!(analyzed.context().problems().is_empty());
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    for (index, expected) in [PgBuiltInType::int4(), kind.clone(), PgBuiltInType::int4()]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            query
                .result_columns()
                .get_column_at(index)
                .unwrap()
                .col_type(),
            &Some(expected)
        );
    }
    for (position, expected) in [(1, PgBuiltInType::int4()), (2, kind)] {
        assert_eq!(
            analyzed
                .context()
                .parameter_constraints()
                .parameter(position)
                .unwrap()
                .constraint()
                .admissible_types(),
            Some([expected].as_slice())
        );
    }
    assert_eq!(
        realized
            .statement()
            .deparse_sql(Dialect::postgres())
            .unwrap(),
        sql
    );
    assert!(!sql.to_ascii_lowercase().contains("cast"));
}
