use ::indexmap::IndexMap;
use ::sql_fun_core::{
    CteAlias, CteName, CteParameterCollection, PlaceholderedSql, ResolvedCteCatalog,
};

/// Build an already-resolved dependency closure without catalog files.
pub(super) fn resolved(base: &str, ctes: &[(&str, &str)]) -> ResolvedCteCatalog {
    let mut parameters = CteParameterCollection::default();
    let mut queries = IndexMap::new();
    for (alias, sql) in ctes {
        let query = PlaceholderedSql::from_query(sql);
        for (name, hints) in query.parameters() {
            parameters.push_query_parameter(&CteName::new(alias), name, hints);
        }
        queries.insert(CteAlias::new(alias), query);
    }
    ResolvedCteCatalog::new(&PlaceholderedSql::from_query(base), &parameters, &queries)
}
