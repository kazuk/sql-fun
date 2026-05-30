use sql_fun_core::IVec;

use crate::{
    sem::{
        AnalysisError, FromClause, FullName, ParseContext, SemScalarExpr, TypeReference, WithClause,
    },
    syn::{Opt, ScanToken, SqlValueFunctionOp},
};

use super::{AnalyzeScalarExpr, SemScalarExprNode};

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
    /// get CurrentTimestamp variant
    pub fn as_current_timestamp(&self) -> Result<&Option<TimePrecision>, UnexpectedVariant> {
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
    fn from_typmod(typmod: i32) -> Self {
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

impl<TParseContext> AnalyzeScalarExpr<TParseContext, crate::syn::SqlValueFunction> for SqlValueExpr
where
    TParseContext: ParseContext,
{
    fn analyze_scalar_expr(
        context: TParseContext,
        _with_clause: &WithClause,
        _from_clause: &FromClause,
        syn: crate::syn::SqlValueFunction,
        _tokens: &IVec<ScanToken>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        let Some(operator) = syn.get_op().as_inner() else {
            AnalysisError::raise_unexpected_none("sqlvaluefunction.op")?
        };

        let node = match operator {
            SqlValueFunctionOp::SvfopCurrentDate => Self::CurrentDate,
            SqlValueFunctionOp::SvfopCurrentTime => Self::CurrentTime(None),
            SqlValueFunctionOp::SvfopCurrentTimeN => {
                let typmod = syn.get_typmod();
                Self::CurrentTime(Some(TimePrecision::from_typmod(typmod)))
            }
            SqlValueFunctionOp::SvfopCurrentTimestamp => Self::CurrentTimestamp(None),
            SqlValueFunctionOp::SvfopCurrentTimestampN => {
                let typmod = syn.get_typmod();
                Self::CurrentTimestamp(Some(TimePrecision::from_typmod(typmod)))
            }
            SqlValueFunctionOp::SvfopLocaltime => Self::LocalTime(None),
            SqlValueFunctionOp::SvfopLocaltimeN => {
                let typmod = syn.get_typmod();
                Self::LocalTime(Some(TimePrecision::from_typmod(typmod)))
            }
            SqlValueFunctionOp::SvfopLocaltimestamp => Self::LocalTimestamp(None),
            SqlValueFunctionOp::SvfopLocaltimestampN => {
                let typmod = syn.get_typmod();
                Self::LocalTimestamp(Some(TimePrecision::from_typmod(typmod)))
            }
            SqlValueFunctionOp::SvfopCurrentRole => Self::CurrentUser,
            SqlValueFunctionOp::SvfopCurrentUser => Self::CurrentUser,
            SqlValueFunctionOp::SvfopUser => Self::CurrentUser,
            SqlValueFunctionOp::SvfopSessionUser => Self::SessionUser,
            SqlValueFunctionOp::SvfopCurrentCatalog => Self::CurrentCatalog,
            SqlValueFunctionOp::SvfopCurrentSchema => Self::CurrentSchema,
            SqlValueFunctionOp::OptionNone | SqlValueFunctionOp::SqlvalueFunctionOpUndefined => {
                AnalysisError::raise_unexpected_input("sqlvaluefunctionop value")?;
                AnalysisError::never();
            }
        };
        Ok((SemScalarExpr::SqlValue(node), context))
    }
}
