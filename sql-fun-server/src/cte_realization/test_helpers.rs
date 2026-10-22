use ::indexmap::IndexMap;
use ::sql_fun_core::{
    CteAlias, CteName, CteParameterCollection, PlaceholderedSql, ResolvedCteCatalog,
};

/// Build an already-resolved dependency closure without catalog files.
pub(super) fn resolved(ctes: &[(&str, &str)]) -> ResolvedCteCatalog {
    let mut parameters = CteParameterCollection::default();
    let mut queries = IndexMap::new();
    for (alias, sql) in ctes {
        let query = PlaceholderedSql::from_query(sql);
        for (name, hints) in query.parameters() {
            parameters.push_query_parameter(&CteName::new(alias), name, hints);
        }
        queries.insert(CteAlias::new(alias), query);
    }
    ResolvedCteCatalog::new(&parameters, &queries)
}

/// Resolve a diamond with repeated roots and repeated dependency references.
pub(super) fn resolved_diamond(
    dialect: ::sql_fun_core::SqlDialect,
) -> ::testresult::TestResult<ResolvedCteCatalog> {
    let project = ::tempfile::tempdir()?;
    let path = project.path().join("diamond.toml");
    std::fs::write(
        &path,
        r#"
sql_fun_cte_catalog_format_version = "1"
sql_dialect = "PostgreSQL"
[cte.shared]
query = "select ${id:int} as id, ${untyped} as n"
[cte.q1]
query = "select ${left:text}, ${id} from shared"
with = [{cte_name = "shared", alias = "shared"}, {cte_name = "shared", alias = "shared"}]
[cte.q2]
query = "select ${right}, ${id:int} from shared"
with = [{cte_name = "shared", alias = "shared"}]
"#,
    )?;
    let mut catalog = ::sql_fun_core::CteCatalog::default();
    catalog.load_from(&path)?;
    catalog.validate()?;
    let references = ["q1", "q2", "q1"].map(|name| {
        ::sql_fun_core::CteCatalogReference::new(&CteName::new(name), &CteAlias::new(name))
    });
    Ok(catalog.resolve(&references, dialect)?)
}

/// Count actual parameter tokens, excluding quoted column labels such as "?1".
pub(super) fn parameter_occurrences(
    sql: &str,
    dialect: ::sql_fun_sqlast::ast::Dialect,
    position: usize,
) -> usize {
    let core_dialect = ::sql_fun_core::SqlDialect::PostgreSQL;
    #[cfg(feature = "sqlite")]
    let core_dialect = if dialect == ::sql_fun_sqlast::ast::Dialect::sqlite() {
        ::sql_fun_core::SqlDialect::SQLite
    } else {
        core_dialect
    };
    let marker = if dialect == ::sql_fun_sqlast::ast::Dialect::postgres() {
        "$"
    } else {
        "?"
    };
    let expected = format!("{marker}{position}");
    ::sql_fun_sqlast::tokenize_sql(sql, core_dialect)
        .unwrap()
        .tokens()
        .filter(|token| token.kind().is_constant() && sql[token.span()] == expected)
        .count()
}
