use crate::sem::names::SchemaName;
use crate::{
    sem::{AnalysisError, ParseContext, error::ProblemLikeError},
    syn::{ListOpt, NodeListOpt},
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
    use crate::sem::names::ope::ValidOperatorName;

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
    pub(super) fn from_parts(schema: Option<&str>, name: &str) -> Self {
        Self {
            schema_name: schema.map(SchemaName::from),
            op_name: name.to_owned(),
        }
    }

    /// create from [`crate::syn::NodeListOpt`]
    pub fn try_from(value: NodeListOpt) -> Result<Option<Self>, ProblemLikeError> {
        let Some(items) = value.map_values(|v| v.as_string().get_sval()) else {
            return Ok(None);
        };
        if items.is_empty() || items.len() >= 2 {
            Err(ProblemLikeError::invalid_operator_name(items))?
        } else if items.len() == 1 {
            Ok(Some(Self {
                schema_name: None,
                op_name: items[0].clone(),
            }))
        } else {
            Ok(Some(Self {
                schema_name: Some(SchemaName::from(items[0].as_str())),
                op_name: items[1].clone(),
            }))
        }
    }

    /// create instance for test
    #[cfg(test)]
    pub fn new(operator_name: &str) -> Self {
        Self {
            schema_name: None,
            op_name: String::from(operator_name),
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
    use super::OperatorName;

    #[test]
    fn presents_valid_names_and_invalid_lookup_state() {
        assert_eq!(
            OperatorName::from_parts(Some("a.b"), "+").to_string(),
            r#""a.b".+"#
        );
        assert_eq!(OperatorName::Invalid.to_string(), "invalid operator name");
    }
}

impl OperatorName {
    pub(crate) fn from_parts(schema: Option<&str>, name: &str) -> Self {
        Self::Valid(ValidOperatorName::from_parts(schema, name))
    }

    /// accept operator name or report problem
    pub fn try_from_report<TParseContext: ParseContext>(
        value: NodeListOpt,
        context: &mut TParseContext,
    ) -> Result<Option<Self>, AnalysisError> {
        let result = ValidOperatorName::try_from(value);
        match result {
            Ok(Some(v)) => Ok(Some(Self::Valid(v))),
            Ok(None) => Ok(None),
            Err(e) => {
                e.report_problem_to(context)?;
                Ok(Some(Self::Invalid))
            }
        }
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

    /// create instance for test
    #[cfg(test)]
    pub fn new(name: &str) -> Self {
        Self::Valid(ValidOperatorName::new(name))
    }
}
