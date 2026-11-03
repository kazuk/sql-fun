use crate::names::SchemaName;
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
