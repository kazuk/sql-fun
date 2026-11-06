use crate::{
    ast::Name,
    names::{FullName, SchemaName},
    sem::ProblemLikeError,
};
use std::fmt::Display;

/// operator name with schema(optional)
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ValidOperatorName {
    schema_name: Option<SchemaName>,
    op_name: String,
}

impl Display for ValidOperatorName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(schema_name) = &self.schema_name {
            write!(
                f,
                "{}.",
                crate::identifier::CanonicalIdentifier::from(schema_name.as_str())
            )?;
        }
        write!(f, "{}", self.op_name)?;
        Ok(())
    }
}

#[cfg(test)]
mod display_tests {
    use crate::names::ValidOperatorName;

    #[rstest::rstest]
    #[case(None, "+")]
    #[case(Some("pg_catalog"), "pg_catalog.+")]
    #[case(Some("a.b"), r#""a.b".+"#)]
    #[case(Some("a\"b"), r#""a""b".+"#)]
    fn preserves_qualification(#[case] schema: Option<&str>, #[case] expected: &str) {
        assert_eq!(
            ValidOperatorName::from_parts(schema, "+").to_string(),
            expected
        );
    }
}

impl ValidOperatorName {
    pub(crate) fn from_parts(schema: Option<&str>, name: &str) -> Self {
        Self {
            schema_name: schema.map(SchemaName::from),
            op_name: name.to_owned(),
        }
    }
}

/// Operator name
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum OperatorName {
    /// accepted operator name
    Valid(ValidOperatorName),
    /// non acceptable operator name
    Invalid,
}

impl Display for OperatorName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Valid(name) => Display::fmt(name, f),
            Self::Invalid => f.write_str("invalid operator name"),
        }
    }
}

#[cfg(test)]
mod operator_display_tests {
    use crate::names::OperatorName;

    #[test]
    fn presents_valid_names_and_invalid_lookup_state() {
        assert_eq!(
            OperatorName::from_parts(Some("a.b"), "+").to_string(),
            r#""a.b".+"#
        );
        assert_eq!(OperatorName::invalid().to_string(), "invalid operator name");
    }
}

impl OperatorName {
    pub(crate) fn from_parts(schema: Option<&str>, name: &str) -> Self {
        Self::Valid(ValidOperatorName::from_parts(schema, name))
    }

    pub(crate) fn from_valid(name: &ValidOperatorName) -> Self {
        Self::Valid(name.clone())
    }

    pub(crate) const fn invalid() -> Self {
        Self::Invalid
    }

    pub(crate) fn full_name(&self) -> Option<FullName> {
        match self {
            Self::Valid(name) => Some(if let Some(schema) = &name.schema_name {
                FullName::with_schema(schema.as_str(), &name.op_name)
            } else {
                FullName::from_local_name(&name.op_name)
            }),
            Self::Invalid => None,
        }
    }

    pub(crate) fn from_full_name(name: &FullName) -> Self {
        Self::from_parts(name.schema().map(SchemaName::as_str), name.name().as_str())
    }

    /// operator name is valid
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        matches!(self, Self::Valid(_))
    }

    /// get name of operator, require valid
    #[must_use]
    pub fn to_name_string(&self) -> String {
        match self {
            Self::Valid(v) => v.to_string(),
            _ => panic!("invalid name"),
        }
    }
}

impl OperatorName {
    pub(crate) fn from_ast_name(ast: &Name) -> Result<Self, ProblemLikeError> {
        ValidOperatorName::try_from_components(ast.parts()).map(|name| Self::from_valid(&name))
    }
}

#[cfg(test)]
mod operator_reporting_tests {
    use crate::{
        ast::Name,
        names::OperatorName,
        test_helpers::{TestParseContext, operator_names::parsed_name},
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
        let name = match OperatorName::from_ast_name(&parsed_name(sql)) {
            Ok(name) => name,
            Err(problem) => {
                problem.report_problem_to(&mut context)?;
                OperatorName::invalid()
            }
        };
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
        let absent: Option<&Name> = None;
        assert_eq!(absent.map(OperatorName::from_ast_name).transpose()?, None);
        assert_eq!(context.reported_problem_count(), 0);
        let empty = Name::from_parts(&[]);
        let problem = OperatorName::from_ast_name(&empty).unwrap_err();
        problem.report_problem_to(&mut context)?;
        let name = OperatorName::invalid();
        assert!(!name.is_valid());
        assert_eq!(context.reported_problem_count(), 1);
        assert!(context.has_reported_problem("operator_name_syntax"));
        Ok(())
    }
}

#[cfg(test)]
mod operator_name_validation_tests {
    use crate::{ast::Name, names::OperatorName};

    #[rstest::rstest]
    #[case(&[], "operator_name_syntax")]
    #[case(&[""], "operator_name_syntax")]
    #[case(&["schema", ""], "operator_name_syntax")]
    #[case(&["a", "b", "+"], "cross_database_operator_reference")]
    #[case(&["a", "b", "c", "+"], "cross_database_operator_reference")]
    fn common_names_classify_invalid_components(#[case] parts: &[&str], #[case] code: &str) {
        let parts: Vec<_> = parts.iter().map(|part| (*part).to_owned()).collect();
        let problem = OperatorName::from_ast_name(&Name::from_parts(&parts))
            .unwrap_err()
            .into_problem();
        assert_eq!(problem.code(), code);
        assert!(problem.render_metadata().is_some());
    }
}

impl ValidOperatorName {
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
        ast::Name,
        names::ValidOperatorName,
        test_helpers::operator_names::{malformed_names, parsed_name},
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
        let parts = nodes.parts();
        let expected = if let Some(schema) = schema {
            vec![schema.to_owned(), operator.to_owned()]
        } else {
            vec![operator.to_owned()]
        };
        assert_eq!(parts, &expected);
        assert_eq!(
            Some(ValidOperatorName::try_from_components(parts)?),
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
        let problem = ValidOperatorName::try_from_components(nodes.parts())
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
        let absent: Option<&Name> = None;
        assert_eq!(
            absent
                .map(|name| ValidOperatorName::try_from_components(name.parts()))
                .transpose()?,
            None
        );
        for nodes in malformed_names() {
            let problem = ValidOperatorName::try_from_components(nodes.parts())
                .unwrap_err()
                .into_problem();
            assert_eq!(problem.code(), "operator_name_syntax");
            assert!(problem.render_metadata().is_some());
        }
        Ok(())
    }
}
