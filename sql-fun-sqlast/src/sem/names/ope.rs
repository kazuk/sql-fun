use std::fmt::Display;

use crate::{
    sem::{AnalysisError, ParseContext, error::ProblemLikeError},
    syn::{ListOpt, NodeListOpt},
};

use super::SchemaName;

/// operator name with schema(optional)
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ValidOperatorName {
    schema_name: Option<SchemaName>,
    op_name: String,
}

impl Display for ValidOperatorName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(schema_name) = &self.schema_name {
            write!(f, "{schema_name}",)?;
        }
        write!(f, "{}", self.op_name)?;
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

impl OperatorName {
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
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid(_))
    }

    /// get name of operator, require vaild
    #[must_use]
    pub fn to_name_string(&self) -> String {
        match self {
            Self::Valid(v) => v.to_string(),
            _ => panic!("invalid name"),
        }
    }
}
