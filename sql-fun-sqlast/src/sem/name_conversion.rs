//! Syntax conversion and diagnostic adapters for shared SQL identities.

#[cfg(feature = "sqlite")]
use crate::syn::sqlite::{Name, QualifiedName};
use crate::{
    names::{
        CatalogName, ColumnName, FullName, LocalName, OperatorName, SchemaName, TableName,
        ValidOperatorName, ViewName,
    },
    sem::{AnalysisError, ParseContext, ProblemLikeError},
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

#[cfg(test)]
mod tests {
    use crate::{names::FullName, test_helpers::type_syntax::build_string_node_list_opt};

    #[test]
    fn test_try_from_nodelistopt_two_elements() {
        let nlo = build_string_node_list_opt(&["schema", "name"]);
        let qt = FullName::try_from(nlo).unwrap();
        assert!(qt.catalog().is_none());
        assert_eq!(qt.schema().unwrap().as_str(), "schema");
        assert_eq!(qt.name().as_str(), "name");
    }

    #[test]
    fn test_try_from_nodelistopt_three_elements() {
        let nlo = build_string_node_list_opt(&["cat", "sch", "nm"]);
        let qt = FullName::try_from(nlo).unwrap();
        assert_eq!(qt.catalog().unwrap().as_str(), "cat");
        assert_eq!(qt.schema().unwrap().as_str(), "sch");
        assert_eq!(qt.name().as_str(), "nm");
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
    use crate::{names::FullName, syn::RangeVar};

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
    use crate::{ast::Dialect, names::FullName, test_helpers::SynBuilder};
    #[cfg(feature = "sqlite")]
    use crate::{syn::sqlite::Stmt, test_helpers::SqliteSynBuilder};

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
        let Some(nodes) = value.as_inner() else {
            return Ok(None);
        };
        let mut parts = Vec::new();
        for node in nodes {
            let Some(part) = node.as_string().get_sval() else {
                return Err(ProblemLikeError::invalid_operator_name(parts));
            };
            parts.push(part);
        }
        Self::try_from_components(&parts).map(Some)
    }

    pub(crate) fn try_from_components(parts: &[String]) -> Result<Self, ProblemLikeError> {
        if parts.iter().any(String::is_empty) {
            return Err(ProblemLikeError::invalid_operator_name(parts.to_vec()));
        }
        match parts {
            [local] => Ok(Self::from_parts(None, local)),
            [schema, local] => Ok(Self::from_parts(Some(schema), local)),
            [] => Err(ProblemLikeError::invalid_operator_name(parts.to_vec())),
            _ => Err(ProblemLikeError::cross_database_operator_reference(parts)),
        }
    }
}

#[cfg(test)]
mod operator_conversion_tests {
    use crate::{
        names::ValidOperatorName,
        syn::{ListOpt, Node, NodeListOpt},
        test_helpers::{SynBuilder, operator_names::parsed_name},
    };

    #[rstest::rstest]
    #[case("SELECT 1 + 2", None, "+")]
    #[case("SELECT 1 OPERATOR(+) 2", None, "+")]
    #[case("SELECT 1 OPERATOR(pg_catalog.+) 2", Some("pg_catalog"), "+")]
    #[case(r#"SELECT 1 OPERATOR("Mixed".+) 2"#, Some("Mixed"), "+")]
    #[case(r#"SELECT 1 OPERATOR("a.b".+) 2"#, Some("a.b"), "+")]
    #[case(r#"SELECT 1 OPERATOR("a""b".+) 2"#, Some("a\"b"), "+")]
    #[case("SELECT 1 OPERATOR(pg_catalog.!=) 2", Some("pg_catalog"), "<>")]
    #[case("SELECT OPERATOR(pg_catalog.-) 1", Some("pg_catalog"), "-")]
    fn parser_components_retain_identity(
        #[case] sql: &str,
        #[case] schema: Option<&str>,
        #[case] operator: &str,
    ) -> testresult::TestResult {
        let nodes = parsed_name(sql);
        let parts = nodes
            .map_values(|node| node.as_string().get_sval())
            .unwrap();
        let expected = if let Some(schema) = schema {
            vec![schema.to_owned(), operator.to_owned()]
        } else {
            vec![operator.to_owned()]
        };
        assert_eq!(parts, expected);
        assert_eq!(
            ValidOperatorName::try_from(nodes)?,
            Some(ValidOperatorName::from_parts(schema, operator))
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case("SELECT 1 OPERATOR(a.b.+) 2")]
    #[case("SELECT 1 OPERATOR(a.b.c.+) 2")]
    fn parser_accepted_database_qualification_is_semantic_error(
        #[case] sql: &str,
    ) -> testresult::TestResult {
        let nodes = parsed_name(sql);
        let problem = ValidOperatorName::try_from(nodes)
            .unwrap_err()
            .into_problem();
        assert_eq!(problem.code(), "cross_database_operator_reference");
        let serialized = serde_json::to_string(&problem)?;
        let restored: sql_fun_core::analysis_problem::AnalysisProblem =
            serde_json::from_str(&serialized)?;
        assert_eq!(restored.code(), problem.code());
        assert!(
            restored
                .render_metadata()
                .unwrap()
                .message()
                .contains("cross-database")
        );
        Ok(())
    }

    #[test]
    fn absence_and_malformed_nodes_remain_distinct() -> testresult::TestResult {
        let builder = SynBuilder::new();
        assert_eq!(ValidOperatorName::try_from(NodeListOpt::none())?, None);
        for nodes in [
            builder.empty_node_list(),
            builder.as_node_list(&[Node::none()]),
            builder.string_node_list(&[""]),
            builder.as_node_list(&[
                builder.as_string_node(builder.string("a")),
                Node::none(),
                builder.as_string_node(builder.string("+")),
            ]),
        ] {
            let problem = ValidOperatorName::try_from(NodeListOpt::from(&nodes))
                .unwrap_err()
                .into_problem();
            assert_eq!(problem.code(), "operator_name_syntax");
            assert!(problem.render_metadata().is_some());
        }
        Ok(())
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

#[cfg(test)]
mod operator_reporting_tests {
    use crate::{
        names::OperatorName,
        syn::NodeListOpt,
        test_helpers::{SynBuilder, TestParseContext, operator_names::parsed_name},
    };

    #[rstest::rstest]
    #[case("SELECT 1 OPERATOR(pg_catalog.+) 2", None)]
    #[case(
        "SELECT 1 OPERATOR(a.b.+) 2",
        Some("cross_database_operator_reference")
    )]
    #[case(
        "SELECT 1 OPERATOR(a.b.c.+) 2",
        Some("cross_database_operator_reference")
    )]
    fn reports_semantic_failure_once(
        #[case] sql: &str,
        #[case] code: Option<&str>,
    ) -> testresult::TestResult {
        let mut context = TestParseContext::default();
        let name = OperatorName::try_from_report(parsed_name(sql), &mut context)?.unwrap();
        if let Some(code) = code {
            assert!(!name.is_valid());
            assert_eq!(context.reported_problem_count(), 1);
            assert!(context.has_reported_problem(code));
        } else {
            assert_eq!(name, OperatorName::from_parts(Some("pg_catalog"), "+"));
            assert_eq!(context.reported_problem_count(), 0);
        }
        Ok(())
    }

    #[test]
    fn malformed_shape_reports_syntax_but_absence_reports_nothing() -> testresult::TestResult {
        let mut context = TestParseContext::default();
        assert_eq!(
            OperatorName::try_from_report(NodeListOpt::none(), &mut context)?,
            None
        );
        assert_eq!(context.reported_problem_count(), 0);
        let empty = SynBuilder::new().empty_node_list();
        let name = OperatorName::try_from_report(NodeListOpt::from(&empty), &mut context)?.unwrap();
        assert!(!name.is_valid());
        assert_eq!(context.reported_problem_count(), 1);
        assert!(context.has_reported_problem("operator_name_syntax"));
        Ok(())
    }
}
