mod test_helpers {
    use crate::cte_catalog::resolve_tests::{CteCatalog, TestProjectDir, write_cte_catalog_toml};

    pub(super) fn catalog<'a>(
        project: &TestProjectDir,
    ) -> Result<CteCatalog<'a>, Box<dyn std::error::Error>> {
        let path = write_cte_catalog_toml(
            project,
            "cte/catalog.toml",
            r#"
[cte.shared]
query = "select ${id:int} as id"
[cte.other]
query = "select ${id:int} as id"
[cte.left]
query = "select * from shared"
with = [{cte_name = "shared", alias = "shared"}, {cte_name = "shared", alias = "shared"}]
[cte.right]
query = "select * from shared"
with = [{cte_name = "shared", alias = "shared"}]
[cte.conflicting]
query = "select * from shared"
with = [{cte_name = "other", alias = "shared"}]
"#,
        )?;
        let mut catalog = CteCatalog::default();
        catalog.load_from(&path)?;
        catalog.validate()?;
        Ok(catalog)
    }
}

use crate::{
    CteAlias, CteCatalog, CteCatalogError, CteCatalogReference, CteName, SqlDialect,
    test_helpers::{TestProjectDir, write_cte_catalog_toml},
};
use ::testresult::TestResult;

#[rstest::rstest]
#[case::diamond(&[("left", "left"), ("right", "right")], &["shared", "left", "right"])]
#[case::repeated_roots(&[("left", "left"), ("left", "left")], &["shared", "left"])]
#[case::distinct_aliases(&[("shared", "first"), ("shared", "second")], &["first", "second"])]
fn preserves_unique_dependency_order(
    #[case] references: &[(&str, &str)],
    #[case] expected: &[&str],
) -> TestResult {
    let project = TestProjectDir::new()?;
    let catalog = test_helpers::catalog(&project)?;
    let mut catalog_references = Vec::new();
    for (name, alias) in references {
        catalog_references.push(CteCatalogReference::new(
            &CteName::new(name),
            &CteAlias::new(alias),
        ));
    }
    let resolved = catalog.resolve(&catalog_references, SqlDialect::PostgreSQL)?;
    let aliases =
        resolved.try_map_cte_queries(|alias, _| Ok::<_, CteCatalogError>(alias.to_string()))?;
    assert_eq!(aliases, expected);
    Ok(())
}

#[rstest::rstest]
#[case::roots(&[("shared", "shared"), ("other", "shared")])]
#[case::branches(&[("left", "left"), ("conflicting", "conflicting")])]
#[case::root_then_dependency(&[("other", "shared"), ("left", "left")])]
#[case::dependency_then_root(&[("left", "left"), ("other", "shared")])]
#[case::parent_and_dependency(&[("left", "shared")])]
fn rejects_conflicting_definitions(#[case] references: &[(&str, &str)]) -> TestResult {
    let project = TestProjectDir::new()?;
    let catalog = test_helpers::catalog(&project)?;
    let mut catalog_references = Vec::new();
    for (name, alias) in references {
        catalog_references.push(CteCatalogReference::new(
            &CteName::new(name),
            &CteAlias::new(alias),
        ));
    }
    let error = catalog
        .resolve(&catalog_references, SqlDialect::PostgreSQL)
        .unwrap_err();
    let CteCatalogError::ConflictingAlias(first_alias, first_name, alias, name) = error else {
        panic!("expected conflicting alias");
    };
    assert_eq!(first_alias.as_str(), "shared");
    assert_eq!(alias.as_str(), "shared");
    assert_ne!(first_name, name);
    Ok(())
}

#[rstest::rstest]
#[case::postgres_exact(SqlDialect::PostgreSQL, "Shared", "Shared", true)]
#[case::postgres_case(SqlDialect::PostgreSQL, "Shared", "shared", false)]
#[case::postgres_unicode(SqlDialect::PostgreSQL, "Ä", "ä", false)]
#[cfg_attr(
    feature = "sqlite",
    case::sqlite_exact(SqlDialect::SQLite, "Shared", "Shared", true)
)]
#[cfg_attr(
    feature = "sqlite",
    case::sqlite_case(SqlDialect::SQLite, "Shared", "shared", true)
)]
#[cfg_attr(
    feature = "sqlite",
    case::sqlite_unicode(SqlDialect::SQLite, "Ä", "ä", false)
)]
fn compares_alias_values_for_the_selected_dialect(
    #[case] dialect: SqlDialect,
    #[case] first: &str,
    #[case] second: &str,
    #[case] equivalent: bool,
    #[values(false, true)] same_definition: bool,
) -> TestResult {
    let project = TestProjectDir::new()?;
    let catalog = test_helpers::catalog(&project)?;
    let references = [
        CteCatalogReference::new(&CteName::new("shared"), &CteAlias::new(first)),
        CteCatalogReference::new(
            &CteName::new(if same_definition { "shared" } else { "other" }),
            &CteAlias::new(second),
        ),
    ];
    let result = catalog.resolve(&references, dialect);
    if equivalent && !same_definition {
        let CteCatalogError::ConflictingAlias(first_alias, first_name, alias, name) =
            result.unwrap_err()
        else {
            panic!("expected alias conflict");
        };
        assert_eq!(first_alias.as_str(), first);
        assert_eq!(first_name, CteName::new("shared"));
        assert_eq!(alias.as_str(), second);
        assert_eq!(name, CteName::new("other"));
    } else {
        let aliases =
            result?.try_map_cte_queries(|alias, _| Ok::<_, CteCatalogError>(alias.to_string()))?;
        let expected = if equivalent {
            vec![first]
        } else {
            vec![first, second]
        };
        assert_eq!(aliases, expected);
    }
    Ok(())
}
