use crate::sem::scalar_expr::SemScalarExprNode;
use crate::{
    names::FullName,
    sem::{AnalysisError, SemScalarExpr, TypeReference},
};

/// SQL value function
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SqlValueExpr {
    /// `CURRENT_DATE()` SQL standard function
    CurrentDate,
    /// `CURRENT_TIME()` / `CURRENT_TIME(n)` SQL standard function
    CurrentTime(Option<TimePrecision>),
    /// `CURRENT_TIMESTAMP()` / `CURRENT_TIMESTAMP(n)` SQL standard function
    CurrentTimestamp(Option<TimePrecision>),
    /// `LOCALTIME` / `LOCALTIME(n)` SQL standard function
    LocalTime(Option<TimePrecision>),
    /// `LOCALTIMESTAMP` / `LOCALTIMESTAMP(n)` SQL standard function
    LocalTimestamp(Option<TimePrecision>),
    /// `CURRENT_USER` / `USER` / `CURRENT_ROLE` SQL standard function
    CurrentUser,
    /// `SESSION_USER`
    SessionUser,
    /// `CURRENT_SCHEMA` SQL standard function
    CurrentSchema,
    /// `CURRENT_CATALOG` SQL standard function
    CurrentCatalog,
}

/// Unexpected variant
#[derive(thiserror::Error, Debug)]
#[error("unexpected variant")]
pub struct UnexpectedVariant;

impl SqlValueExpr {
    pub(crate) fn from_common(
        name: &str,
        precision: Option<i32>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let precision = precision.map(TimePrecision::from_typmod);
        let value = match name {
            "current_date" => Self::CurrentDate,
            "current_time" => Self::CurrentTime(precision),
            "current_timestamp" => Self::CurrentTimestamp(precision),
            "localtime" => Self::LocalTime(precision),
            "localtimestamp" => Self::LocalTimestamp(precision),
            "current_user" | "current_role" | "user" => Self::CurrentUser,
            "session_user" => Self::SessionUser,
            "current_schema" => Self::CurrentSchema,
            "current_catalog" => Self::CurrentCatalog,
            _ => return AnalysisError::raise_unexpected_input("SQL value name"),
        };
        Ok(SemScalarExpr::sql_value(&value))
    }

    /// get CurrentTimestamp variant
    pub const fn as_current_timestamp(&self) -> Result<&Option<TimePrecision>, UnexpectedVariant> {
        match self {
            Self::CurrentTimestamp(c) => Ok(c),
            _ => Err(UnexpectedVariant),
        }
    }
}

/// time precision typmod value
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TimePrecision(i32);

impl TimePrecision {
    const fn from_typmod(typmod: i32) -> Self {
        Self(typmod)
    }
}

impl SemScalarExprNode for SqlValueExpr {
    fn get_type(&self) -> Option<TypeReference> {
        match self {
            Self::CurrentDate => Some(TypeReference::concrete_type_ref(
                &FullName::from_local_name("date"),
                false,
            )),
            Self::CurrentTime(_) => Some(TypeReference::concrete_type_ref(
                &FullName::from_local_name("timetz"),
                false,
            )),
            Self::CurrentTimestamp(_) => Some(TypeReference::concrete_type_ref(
                &FullName::from_local_name("timestamptz"),
                false,
            )),
            Self::LocalTime(_) => Some(TypeReference::concrete_type_ref(
                &FullName::from_local_name("time"),
                false,
            )),
            Self::LocalTimestamp(_) => Some(TypeReference::concrete_type_ref(
                &FullName::from_local_name("timestamp"),
                false,
            )),
            Self::CurrentUser | Self::SessionUser | Self::CurrentSchema | Self::CurrentCatalog => {
                Some(TypeReference::concrete_type_ref(
                    &FullName::from_local_name("name"),
                    false,
                ))
            }
        }
    }

    fn is_not_null(&self) -> Option<bool> {
        match self {
            Self::CurrentSchema => Some(false),
            _ => Some(true),
        }
    }
}
