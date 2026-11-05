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
        ValidOperatorName::try_from_components(&ast.0).map(|name| Self::from_valid(&name))
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
