use crate::sem::{
    AnalysisError, FromClause, ParseContext, PgBuiltInType, SemScalarExpr, TypeReference,
    WithClause,
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
    /// create a NULL
    pub fn null() -> SemScalarExpr {
        SemScalarExpr::Const(Self::Null)
    }

    /// create integer constant value
    pub fn new_integer(value: i32) -> Self {
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
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        let cval = if let Some(ival) = syn.as_ival().get_ival() {
            ScalarConstExpr::Integer(ival)
        } else if let Some(fval) = syn.as_fval().get_fval() {
            ScalarConstExpr::Float(fval)
        } else if let Some(bval) = syn.as_boolval().get_boolval() {
            ScalarConstExpr::Bool(bval)
        } else if let Some(sval) = syn.as_sval().get_sval() {
            ScalarConstExpr::String(sval)
        } else if let Some(bsval) = syn.as_bsval().get_bsval() {
            ScalarConstExpr::BitString(bsval)
        } else {
            AnalysisError::raise_unexpected_none("AConst: unknown variant")?
        };
        Ok((SemScalarExpr::Const(cval), context))
    }
}

impl SemScalarExprNode for ScalarConstExpr {
    fn get_type(&self) -> Option<TypeReference> {
        match self {
            ScalarConstExpr::Integer(_) => Some(PgBuiltInType::int4()),
            ScalarConstExpr::Float(_) => Some(PgBuiltInType::numeric()),
            ScalarConstExpr::Bool(_) => Some(PgBuiltInType::bool()),
            ScalarConstExpr::String(_) => Some(PgBuiltInType::text()),
            ScalarConstExpr::BitString(_) => Some(PgBuiltInType::varbit()),
            ScalarConstExpr::Null => None,
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
