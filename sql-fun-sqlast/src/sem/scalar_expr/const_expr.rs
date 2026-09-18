use sql_fun_core::IVec;

use crate::{
    sem::{
        AnalysisError, FromClause, ParseContext, PgBuiltInType, SemScalarExpr, TypeReference,
        WithClause,
    },
    syn::ScanToken,
};

use super::{AnalyzeScalarExpr, SemScalarExprNode};

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

impl<TParseContext> AnalyzeScalarExpr<TParseContext, crate::syn::AConst> for ScalarConstExpr
where
    TParseContext: ParseContext,
{
    fn analyze_scalar_expr(
        context: TParseContext,
        _with_clause: &WithClause,
        _from_clause: &FromClause,
        syn: crate::syn::AConst,
        _tokens: &IVec<ScanToken>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        let cval = if let Some(ival) = syn.as_ival().get_ival() {
            Self::Integer(ival)
        } else if let Some(fval) = syn.as_fval().get_fval() {
            Self::Float(fval)
        } else if let Some(bval) = syn.as_boolval().get_boolval() {
            Self::Bool(bval)
        } else if let Some(sval) = syn.as_sval().get_sval() {
            Self::String(sval)
        } else if let Some(bsval) = syn.as_bsval().get_bsval() {
            Self::BitString(bsval)
        } else {
            AnalysisError::raise_unexpected_none("AConst: unknown variant")?
        };
        Ok((SemScalarExpr::Const(cval), context))
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
