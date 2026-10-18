use super::FullName;
use crate::ast::Dialect;
use crate::sem::{CatalogName, LocalName};

#[rstest::rstest]
#[case::plain("simple_name", "simple_name")]
#[case::dot("a.b", r#""a.b""#)]
#[case::quote("a\"b", r#""a""b""#)]
#[case::mixed("Mixed", r#""Mixed""#)]
#[case::space("odd name", r#""odd name""#)]
#[case::reserved("select", r#""select""#)]
#[case::unicode("éclair", r#""éclair""#)]
#[case::empty("", r#""""#)]
#[case::numeric("12abc", r#""12abc""#)]
#[case::array_spelling("item[]", r#""item[]""#)]
fn formats_each_component_identically(#[case] component: &str, #[case] expected: &str) {
    assert_eq!(FullName::from_local_name(component).to_string(), expected);
    assert_eq!(
        FullName::with_schema(component, component).to_string(),
        format!("{expected}.{expected}")
    );
    assert_eq!(
        FullName::with_catalog(component, component, component).to_string(),
        format!("{expected}.{expected}.{expected}")
    );
}

#[test]
fn canonical_spelling_is_injective_and_reversible() -> testresult::TestResult {
    // Enumerate valid qualification shapes over adversarial component values.
    // This is one invariant check over the entire generated name set.
    let values = ["", "a", "A", "a.b", "a\"b", "a:b", "a[]", "é", "select"];
    let mut spellings = std::collections::HashMap::new();
    let mut check = |name: FullName| -> testresult::TestResult {
        let spelling = name.to_string();
        assert_eq!(spelling.parse::<FullName>()?, name);
        assert!(spellings.insert(spelling, name).is_none());
        Ok(())
    };
    for name in values {
        check(FullName::from_local_name(name))?;
        for schema in values {
            check(FullName::with_schema(schema, name))?;
            for catalog in values {
                check(FullName::with_catalog(catalog, schema, name))?;
            }
        }
    }
    Ok(())
}

#[test]
#[should_panic(expected = "a catalog requires a schema")]
fn constructor_prevents_catalog_only_names() {
    let _ = FullName::new(
        &Some(CatalogName::from("db")),
        &None,
        &LocalName::from("items"),
    );
}

#[rstest::rstest]
#[case::local("Mixed", FullName::from_local_name("Mixed"))]
#[case::quoted_dot(r#""a.b".c"#, FullName::with_schema("a.b", "c"))]
#[case::local_dot(r#"a."b.c""#, FullName::with_schema("a", "b.c"))]
#[case::three(
    r#""my.db"."a.b"."c.d""#,
    FullName::with_catalog("my.db", "a.b", "c.d")
)]
#[case::quoted_empty(r#""".x"#, FullName::with_schema("", "x"))]
fn canonical_parser_preserves_components(
    #[case] input: &str,
    #[case] expected: FullName,
) -> testresult::TestResult {
    assert_eq!(input.parse::<FullName>()?, expected);
    Ok(())
}

#[rstest::rstest]
#[case::empty("")]
#[case::missing_local("a.")]
#[case::missing_schema(".a")]
#[case::catalog_only("a..b")]
#[case::too_many("a.b.c.d")]
#[case::unclosed(r#""a"#)]
#[case::trailing(r#""a"b"#)]
#[case::unquoted_space("a b")]
fn malformed_names_are_rejected(#[case] input: &str) {
    assert!(input.parse::<FullName>().is_err());
}

#[rstest::rstest]
#[case::dot(FullName::with_schema("a.b", "c"))]
#[case::quote(FullName::with_schema("a", "b\"c"))]
#[case::catalog(FullName::with_catalog("my.db", "a.b", "c.d"))]
#[case::case(FullName::from_local_name("Mixed"))]
#[case::keyword(FullName::from_local_name("select"))]
#[case::unicode(FullName::from_local_name("éclair"))]
fn postgres_sql_parser_recovers_logical_names(#[case] name: FullName) -> testresult::TestResult {
    use crate::syn::ListOpt;
    for spelling in [name.to_string(), name.to_sql(Dialect::postgres())?] {
        let node =
            crate::test_helpers::SynBuilder::new().parse(&format!("SELECT * FROM {spelling}"));
        let relation = node
            .as_select_stmt()
            .get_from_clause()
            .map(|node| node.as_range_var())
            .expect("FROM");
        assert_eq!(FullName::try_from(relation[0].clone())?, name);
    }
    Ok(())
}

#[test]
fn postgres_case_folding_belongs_to_sql_parsing() -> testresult::TestResult {
    use crate::syn::ListOpt;
    let node = crate::test_helpers::SynBuilder::new().parse("SELECT * FROM Mixed");
    let relations = node
        .as_select_stmt()
        .get_from_clause()
        .map(|node| node.as_range_var())
        .expect("FROM");
    assert_eq!(
        FullName::try_from(relations[0].clone())?,
        FullName::from_local_name("mixed")
    );
    assert_eq!(
        "Mixed".parse::<FullName>()?,
        FullName::from_local_name("Mixed")
    );
    Ok(())
}

#[rstest::rstest]
#[case::empty(FullName::from_local_name(""))]
#[case::empty_schema(FullName::with_schema("", "x"))]
#[case::nul(FullName::from_local_name("a\0b"))]
fn checked_postgres_emission_rejects_invalid_sql(#[case] name: FullName) {
    assert!(
        name.to_sql(Dialect::postgres())
            .unwrap_err()
            .is_invalid_syntax()
    );
}

#[test]
fn checked_sqlite_emission_rejects_catalog() {
    assert!(
        FullName::with_catalog("db", "main", "x")
            .to_sql(Dialect::sqlite())
            .is_err()
    );
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::empty("")]
#[case::dot("a.b")]
#[case::quote("a\"b")]
#[case::case("Mixed")]
#[case::keyword("select")]
fn sqlite_parser_preserves_quoted_names(#[case] component: &str) -> testresult::TestResult {
    let name = FullName::with_schema("main", component);
    let sql = format!(
        "CREATE TABLE {} (id INTEGER)",
        name.to_sql(Dialect::sqlite())?
    );
    let statement = crate::test_helpers::SqliteSynBuilder.parse_as_stmt(&sql);
    let crate::syn::sqlite::Stmt::CreateTable(table) = statement else {
        panic!("CREATE TABLE")
    };
    assert_eq!(FullName::try_from(table.table_name())?, name);
    Ok(())
}
