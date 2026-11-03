mod test_helpers {
    use crate::cte_catalog::fingerprint_tests::{
        CteAlias, CteName, CteParameterCollection, IndexMap, PlaceholderedSql, ResolvedCteCatalog,
    };

    pub(super) fn catalog(ctes: &[(&str, &str)], definition: &str) -> ResolvedCteCatalog {
        let mut parameters = CteParameterCollection::default();
        let mut queries = IndexMap::new();
        for (alias, sql) in ctes {
            let query = PlaceholderedSql::from_query(sql);
            for (name, hints) in query.parameters() {
                parameters.push_query_parameter(&CteName::new(definition), name, hints);
            }
            queries.insert(CteAlias::new(alias), query);
        }
        ResolvedCteCatalog::new(&parameters, &queries)
    }
}

use crate::{
    PlaceholderedSql,
    cte_catalog::{CteAlias, CteName, CteParameterCollection, ResolvedCteCatalog},
};
use ::indexmap::IndexMap;

#[rstest::rstest]
#[case::plain(&[])]
#[case::dependencies(&[("z_dependency", "select ${shared:int} as id"), ("a_visible", "select id from z_dependency")])]
fn full_identity_survives_query_and_dependencies_round_trip(#[case] ctes: &[(&str, &str)]) {
    let base = PlaceholderedSql::from_query("select ${shared:int}, ${base:text} from a_visible");
    let catalog = test_helpers::catalog(ctes, "library::definition");
    let recovered = PlaceholderedSql::from_query(&base.encode());
    let restored: ResolvedCteCatalog =
        ::serde_json::from_str(&::serde_json::to_string(&catalog).unwrap()).unwrap();
    assert_eq!(
        catalog.finger_print(&base).unwrap(),
        restored.finger_print(&recovered).unwrap()
    );
}

#[rstest::rstest]
#[case::base_sql("select ${id:int}, 2", &[("first", "select ${id:int}"), ("second", "select 2")], "definition")]
#[case::base_hint("select ${id:text}", &[("first", "select ${id:int}"), ("second", "select 2")], "definition")]
#[case::cte_sql("select ${id:int}", &[("first", "select ${id:int}, 1"), ("second", "select 2")], "definition")]
#[case::alias("select ${id:int}", &[("renamed", "select ${id:int}"), ("second", "select 2")], "definition")]
#[case::order("select ${id:int}", &[("second", "select 2"), ("first", "select ${id:int}")], "definition")]
#[case::binding_metadata("select ${id:int}", &[("first", "select ${id:int}"), ("second", "select 2")], "other_definition")]
fn full_identity_changes_with_each_input(
    #[case] sql: &str,
    #[case] ctes: &[(&str, &str)],
    #[case] definition: &str,
) {
    let base = PlaceholderedSql::from_query("select ${id:int}");
    let original = test_helpers::catalog(
        &[("first", "select ${id:int}"), ("second", "select 2")],
        "definition",
    );
    let changed = test_helpers::catalog(ctes, definition);
    assert_ne!(
        original.finger_print(&base).unwrap(),
        changed
            .finger_print(&PlaceholderedSql::from_query(sql))
            .unwrap()
    );
}
