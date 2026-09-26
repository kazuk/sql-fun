use crate::sem::scalar_expr::SemScalarExprNode;
use crate::sem::{PgBuiltInType, SemScalarExpr, TypeReference};

/// constant expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ScalarConstExpr {
    /// integer value constant
    Integer(i32),
    /// floating numeric value constant
    Float(String),
    /// boolean constant value
    Bool(bool),
    /// string constant value
    String(String),
    /// bit-string constant
    BitString(String),
    /// NULL constant
    Null,
}

impl ScalarConstExpr {
    pub(crate) fn numeric(value: &str) -> Self {
        Self::Float(value.to_owned())
    }
    pub(crate) fn string(value: &str) -> Self {
        Self::String(value.to_owned())
    }
    pub(crate) const fn boolean(value: bool) -> Self {
        Self::Bool(value)
    }
    pub(crate) fn bit_string(value: &str) -> Self {
        Self::BitString(value.to_owned())
    }

    /// get string variant value
    pub const fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// create a NULL
    #[must_use]
    pub const fn null() -> SemScalarExpr {
        SemScalarExpr::Const(Self::Null)
    }

    /// create integer constant value
    #[must_use]
    pub const fn new_integer(value: i32) -> Self {
        Self::Integer(value)
    }
}

impl SemScalarExprNode for ScalarConstExpr {
    fn get_type(&self) -> Option<TypeReference> {
        match self {
            Self::Integer(_) => Some(PgBuiltInType::int4()),
            Self::Float(_) => Some(PgBuiltInType::numeric()),
            Self::Bool(_) => Some(PgBuiltInType::bool()),
            Self::String(_) => Some(PgBuiltInType::text()),
            Self::BitString(_) => Some(PgBuiltInType::varbit()),
            Self::Null => None,
        }
    }

    fn is_not_null(&self) -> Option<bool> {
        if matches!(self, Self::Null) {
            Some(false)
        } else {
            Some(true)
        }
    }
}
