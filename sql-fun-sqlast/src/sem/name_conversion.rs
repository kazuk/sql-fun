//! Syntax conversion and diagnostic adapters for shared SQL identities.

#[cfg(feature = "sqlite")]
use crate::syn::sqlite::{Name, QualifiedName};
use crate::{
    names::{
        CatalogName, ColumnName, FullName, LocalName, OperatorName, SchemaName, TableName,
        ValidOperatorName, ViewName,
    },
    sem::{AnalysisError, ParseContext, error::ProblemLikeError},
    syn::{ListOpt, Node, NodeListOpt, RangeVar, String as SynString, TypeName},
};

fn nonempty(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}

#[cfg(feature = "sqlite")]
impl TryFrom<&Name> for CatalogName {
    type Error = AnalysisError;

    fn try_from(value: &Name) -> Result<Self, Self::Error> {
        Ok(Self::from(value.value()))
    }
}

#[cfg(feature = "sqlite")]
impl TryFrom<&Name> for SchemaName {
    type Error = AnalysisError;

    fn try_from(value: &Name) -> Result<Self, Self::Error> {
        Ok(Self::from(value.value()))
    }
}

#[cfg(feature = "sqlite")]
impl TryFrom<&Name> for LocalName {
    type Error = AnalysisError;

    fn try_from(value: &Name) -> Result<Self, Self::Error> {
        Ok(Self::from(value.value()))
    }
}

#[cfg(feature = "sqlite")]
impl TryFrom<&QualifiedName> for FullName {
    type Error = AnalysisError;

    fn try_from(value: &QualifiedName) -> Result<Self, Self::Error> {
        Ok(Self::new(
            &None,
            &value.db_name().map(SchemaName::try_from).transpose()?,
            &LocalName::try_from(value.name())?,
        ))
    }
}

impl TryFrom<&Node> for ColumnName {
    type Error = AnalysisError;

    fn try_from(value: &Node) -> Result<Self, Self::Error> {
        let Some(name) = value.as_string().get_sval() else {
            AnalysisError::raise_unexpected_none("(Node as String).sval")?;
            AnalysisError::never();
        };
        Ok(Self::from(name))
    }
}

#[cfg(feature = "sqlite")]
impl From<&Name> for ColumnName {
    fn from(value: &Name) -> Self {
        Self::new(value.value())
    }
}

impl TryFrom<RangeVar> for TableName {
    type Error = AnalysisError;

    fn try_from(value: RangeVar) -> Result<Self, Self::Error> {
        Ok(Self::from(FullName::try_from(value)?))
    }
}

#[cfg(feature = "sqlite")]
impl TryFrom<&QualifiedName> for TableName {
    type Error = AnalysisError;

    fn try_from(value: &QualifiedName) -> Result<Self, Self::Error> {
        Ok(Self::from(FullName::try_from(value)?))
    }
}

#[cfg(feature = "sqlite")]
impl TryFrom<&Name> for TableName {
    type Error = AnalysisError;

    fn try_from(value: &Name) -> Result<Self, Self::Error> {
        Ok(Self::from(FullName::from_local_name(value.value())))
    }
}

impl From<RangeVar> for ViewName {
    fn from(value: RangeVar) -> Self {
        Self::from(&value)
    }
}

impl From<&RangeVar> for ViewName {
    fn from(value: &RangeVar) -> Self {
        let catalog = nonempty(value.get_catalogname());
        let schema = nonempty(value.get_schemaname());
        let relation = value.get_relname();
        Self::from_full_name(&FullName::new(
            &catalog.as_deref().map(CatalogName::from),
            &schema.as_deref().map(SchemaName::from),
            &LocalName::from(&relation),
        ))
    }
}

impl TryFrom<NodeListOpt> for FullName {
    type Error = AnalysisError;

    fn try_from(value: NodeListOpt) -> Result<Self, Self::Error> {
        let Some(name_elements) = value.map(|v| v.as_string()) else {
            AnalysisError::raise_unexpected_none("node_list")?
        };
        let elements: Vec<String> = name_elements.iter().map(SynString::get_sval).collect();
        match elements.len() {
            3 => Ok(Self::with_catalog(&elements[0], &elements[1], &elements[2])),
            2 => Ok(Self::with_schema(&elements[0], &elements[1])),
            1 => Ok(Self::from_local_name(&elements[0])),
            _ => Err(AnalysisError::type_name_format(&elements)),
        }
    }
}

impl TryFrom<RangeVar> for FullName {
    type Error = AnalysisError;

    fn try_from(value: RangeVar) -> Result<Self, Self::Error> {
        let catalog = value.get_catalogname();
        let schema = value.get_schemaname();
        let name = value.get_relname();
        if !catalog.is_empty() && schema.is_empty() {
            return Err(AnalysisError::unexpected_input_error(
                "a catalog requires a schema",
            ));
        }
        Ok(Self::new(
            &if catalog.is_empty() {
                None
            } else {
                Some(CatalogName::from(&catalog))
            },
            &if schema.is_empty() {
                None
            } else {
                Some(SchemaName::from(&schema))
            },
            &LocalName::from(&name),
        ))
    }
}

#[cfg(test)]
mod range_var_tests {
    use crate::syn::RangeVar;

    use crate::names::FullName;

    #[test]
    fn rejects_catalog_without_schema() {
        let relation = ::pg_query::protobuf::RangeVar {
            catalogname: "db".to_owned(),
            relname: "items".to_owned(),
            ..Default::default()
        };
        let relation = RangeVar::from(relation);
        assert!(FullName::try_from(relation).is_err());
    }
}

impl TryFrom<TypeName> for FullName {
    type Error = AnalysisError;

    fn try_from(value: TypeName) -> Result<Self, Self::Error> {
        let Some(names) = value.get_names().map(|v| v.as_string()) else {
            AnalysisError::raise_unexpected_none("type_name.names")?
        };
        let names: Vec<String> = names.iter().map(SynString::get_sval).collect();

        match names.len() {
            3 => Ok(Self::with_catalog(&names[0], &names[1], &names[2])),
            2 => Ok(Self::with_schema(&names[0], &names[1])),
            1 => Ok(Self::from_local_name(&names[0])),
            _ => Err(AnalysisError::type_name_format(&names)),
        }
    }
}

impl std::str::FromStr for FullName {
    type Err = AnalysisError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_components(s).ok_or_else(|| AnalysisError::type_name_format(&s))
    }
}

#[cfg(test)]
mod canonical_parse_tests {
    use crate::names::FullName;
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
}

#[cfg(test)]
mod sql_name_conversion_tests {
    #[cfg(feature = "sqlite")]
    use crate::syn::sqlite::Stmt;
    #[cfg(feature = "sqlite")]
    use crate::test_helpers::SqliteSynBuilder;
    use crate::test_helpers::SynBuilder;

    use crate::{ast::Dialect, names::FullName};
    #[rstest::rstest]
    #[case::dot(FullName::with_schema("a.b", "c"))]
    #[case::quote(FullName::with_schema("a", "b\"c"))]
    #[case::catalog(FullName::with_catalog("my.db", "a.b", "c.d"))]
    #[case::case(FullName::from_local_name("Mixed"))]
    #[case::keyword(FullName::from_local_name("select"))]
    #[case::unicode(FullName::from_local_name("éclair"))]
    fn postgres_sql_parser_recovers_logical_names(
        #[case] name: FullName,
    ) -> testresult::TestResult {
        use crate::syn::ListOpt;
        for spelling in [name.to_string(), name.to_sql(Dialect::postgres())?] {
            let node = SynBuilder::new().parse(&format!("SELECT * FROM {spelling}"));
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
        let node = SynBuilder::new().parse("SELECT * FROM Mixed");
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
        let statement = SqliteSynBuilder.parse_as_stmt(&sql);
        let Stmt::CreateTable(table) = statement else {
            panic!("CREATE TABLE")
        };
        assert_eq!(FullName::try_from(table.table_name())?, name);
        Ok(())
    }
}

impl ValidOperatorName {
    /// create from [`crate::syn::NodeListOpt`]
    pub fn try_from(value: NodeListOpt) -> Result<Option<Self>, ProblemLikeError> {
        let Some(items) = value.map_values(|v| v.as_string().get_sval()) else {
            return Ok(None);
        };
        if items.is_empty() || items.len() >= 2 {
            Err(ProblemLikeError::invalid_operator_name(items))?
        } else if items.len() == 1 {
            Ok(Some(Self::from_parts(None, &items[0])))
        } else {
            Ok(Some(Self::from_parts(Some(&items[0]), &items[1])))
        }
    }
}

impl OperatorName {
    /// accept operator name or report problem
    pub fn try_from_report<TParseContext: ParseContext>(
        value: NodeListOpt,
        context: &mut TParseContext,
    ) -> Result<Option<Self>, AnalysisError> {
        let result = ValidOperatorName::try_from(value);
        match result {
            Ok(Some(v)) => Ok(Some(Self::from_valid(&v))),
            Ok(None) => Ok(None),
            Err(e) => {
                e.report_problem_to(context)?;
                Ok(Some(Self::invalid()))
            }
        }
    }
}
