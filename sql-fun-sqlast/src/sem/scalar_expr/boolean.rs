use crate::sem::scalar_expr::SemScalarExprNode;
use crate::sem::{AnalysisError, PgBuiltInType, SemScalarExpr, TypeReference};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum BooleanExprBody {
    And(Vec<SemScalarExpr>),
    Or(Vec<SemScalarExpr>),
    Not(Box<SemScalarExpr>),
}

impl BooleanExprBody {
    pub fn and(items: &[SemScalarExpr]) -> Self {
        Self::And(items.to_vec())
    }

    pub fn or(items: &[SemScalarExpr]) -> Self {
        Self::Or(items.to_vec())
    }

    pub fn not(expr: &SemScalarExpr) -> Self {
        Self::Not(Box::new(expr.clone()))
    }
}

/// boolean expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BooleanExpr {
    body: BooleanExprBody,
}

impl BooleanExpr {
    /// create boolean expression from body
    pub const fn new(body: BooleanExprBody) -> Self {
        Self { body }
    }
}

impl SemScalarExprNode for BooleanExpr {
    fn get_type(&self) -> Option<TypeReference> {
        Some(PgBuiltInType::bool())
    }

    fn is_not_null(&self) -> Option<bool> {
        match &self.body {
            BooleanExprBody::And(items) => {
                for item in items {
                    let is_not_null = item.is_not_null()?;
                    if !is_not_null {
                        return Some(false);
                    }
                }
                Some(true)
            }
            BooleanExprBody::Or(items) => {
                for item in items {
                    let is_not_null = item.is_not_null()?;
                    if !is_not_null {
                        return Some(false);
                    }
                }
                Some(true)
            }
            BooleanExprBody::Not(expr) => expr.is_not_null(),
        }
    }
}

/// `NullTestExpr`
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NullTestExpr {
    null_test_type: NullTestType,
    arg_expr: Box<SemScalarExpr>,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum NullTestType {
    IsNull,
    IsNotNull,
}

impl NullTestType {
    pub(crate) const fn from_negated(negated: bool) -> Self {
        if negated {
            Self::IsNotNull
        } else {
            Self::IsNull
        }
    }
}

impl NullTestExpr {
    /// create null test expression from semantic input expression
    pub fn new(null_test_type: NullTestType, arg_expr: SemScalarExpr) -> Self {
        Self {
            null_test_type,
            arg_expr: Box::new(arg_expr),
        }
    }
}

impl TryFrom<crate::syn::NullTestType> for NullTestType {
    type Error = AnalysisError;

    fn try_from(value: crate::syn::NullTestType) -> Result<Self, Self::Error> {
        match value {
            crate::syn::NullTestType::OptionNone | crate::syn::NullTestType::Undefined => {
                AnalysisError::raise_unexpected_input("nulltesttype")?
            }
            crate::syn::NullTestType::IsNull => Ok(Self::IsNull),
            crate::syn::NullTestType::IsNotNull => Ok(Self::IsNotNull),
        }
    }
}

impl SemScalarExprNode for NullTestExpr {
    fn get_type(&self) -> Option<TypeReference> {
        Some(PgBuiltInType::bool())
    }

    fn is_not_null(&self) -> Option<bool> {
        Some(true)
    }
}
