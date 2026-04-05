mod binary;
mod call_sites;
mod impl_analyze;
mod impl_build_op;
mod null_if;
mod unary;

use sql_fun_core::IVec;

use crate::{
    sem::{
        AnalysisError, FromClause, ParseContext, SemScalarExpr, TypeReference, WithClause,
        scalar_expr::SemScalarExprNode,
    },
    syn::ScanToken,
};

pub use self::{
    call_sites::{
        AllBinaryCallSite, AnyBinaryCallSite, BinaryOpCallSite, LeftUnaryCallSite,
        RightUnaryCallSite, UndefinedOperatorAllCallSite, UndefinedOperatorAnyCallSite,
        UndefinedOperatorCallSite,
    },
    null_if::NullIfExpr,
};

/// arith expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ArithExpr {
    /// binary operator expression
    OpBinary(Box<BinaryOpCallSite>),
    /// left unary expression
    OpLeftUnary(Box<LeftUnaryCallSite>),
    /// right unary expression
    OpRightUnary(Box<RightUnaryCallSite>),
    /// any binary expression
    OpAnyBinary(Box<AnyBinaryCallSite>),
    /// all binary expression
    OpAllBinary(Box<AllBinaryCallSite>),

    /// undefined operator
    UndefinedOperator(Box<UndefinedOperatorCallSite>),
    /// undefined operator Any
    UndefinedOperatorAny(Box<UndefinedOperatorAnyCallSite>),
    /// undefined operator All
    UndefinedOperatorAll(Box<UndefinedOperatorAllCallSite>),

    /// null-if  expression
    NullIfExpr(Box<NullIfExpr>),
}

impl ArithExpr {
    fn build_distinct<TParseContext>(
        _context: TParseContext,
        _lexpr: Option<SemScalarExpr>,
        _rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        todo!()
    }

    fn build_not_distinct<TParseContext>(
        _context: TParseContext,
        _lexpr: Option<SemScalarExpr>,
        _rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        todo!()
    }

    fn build_in<TParseContext>(
        _context: TParseContext,
        _lexpr: Option<SemScalarExpr>,
        _rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        todo!()
    }

    fn build_like<TParseContext>(
        _context: TParseContext,
        _lexpr: Option<SemScalarExpr>,
        _rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        todo!()
    }

    fn build_ilike<TParseContext>(
        _context: TParseContext,
        _lexpr: Option<SemScalarExpr>,
        _rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        todo!()
    }

    fn build_similar<TParseContext>(
        _context: TParseContext,
        _lexpr: Option<SemScalarExpr>,
        _rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        todo!()
    }

    fn build_between<TParseContext>(
        _context: TParseContext,
        _lexpr: Option<SemScalarExpr>,
        _rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        todo!()
    }

    fn build_not_between<TParseContext>(
        _context: TParseContext,
        _lexpr: Option<SemScalarExpr>,
        _rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        todo!()
    }

    fn build_between_sym<TParseContext>(
        _context: TParseContext,
        _lexpr: Option<SemScalarExpr>,
        _rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        todo!()
    }

    fn build_not_between_sym<TParseContext>(
        _context: TParseContext,
        _lexpr: Option<SemScalarExpr>,
        _rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        todo!()
    }

    fn inputs_is_not_null(&self) -> Option<bool> {
        match &self {
            Self::OpLeftUnary(call_site) => call_site.is_not_null(),
            Self::OpRightUnary(call_site) => call_site.is_not_null(),
            Self::UndefinedOperator(call_site) => call_site.inputs_is_not_null(),
            Self::OpBinary(call_site) => call_site.is_not_null(),
            Self::OpAnyBinary(call_site) => call_site.is_not_null(),
            Self::OpAllBinary(call_site) => call_site.is_not_null(),
            Self::UndefinedOperatorAny(call_site) => call_site.is_not_null(),
            Self::UndefinedOperatorAll(call_site) => call_site.is_not_null(),
            Self::NullIfExpr(null_if) => null_if.is_not_null(),
        }
    }

    fn operator_returns_null(&self) -> Option<bool> {
        None
    }

    fn analyze_optional_expr<TParseContext>(
        context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        expr: Option<crate::syn::Node>,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Option<SemScalarExpr>, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(expr) = expr else {
            return Ok((None, context));
        };
        let (sem_expr, new_context) =
            SemScalarExpr::analyze_scaler_expr(context, with_clause, from_clause, expr, tokens)?;
        Ok((Some(sem_expr), new_context))
    }
}

impl SemScalarExprNode for ArithExpr {
    fn get_type(&self) -> Option<TypeReference> {
        match &self {
            Self::OpBinary(call_site) => Some(call_site.get_return_type().clone()),
            Self::OpLeftUnary(call_site) => Some(call_site.get_return_type().clone()),
            Self::OpRightUnary(call_site) => Some(call_site.get_return_type().clone()),
            Self::OpAnyBinary(call_site) => Some(call_site.get_return_type().clone()),
            Self::OpAllBinary(call_site) => Some(call_site.get_return_type().clone()),
            Self::NullIfExpr(null_if) => null_if.get_type(),
            Self::UndefinedOperator(_)
            | Self::UndefinedOperatorAny(_)
            | Self::UndefinedOperatorAll(_) => None,
        }
    }

    fn is_not_null(&self) -> Option<bool> {
        let is_strict = match &self {
            Self::OpLeftUnary(site) => site.is_strict(),
            Self::OpRightUnary(site) => site.is_strict(),
            Self::OpBinary(call_site) => call_site.is_strict(),
            Self::OpAnyBinary(call_site) => call_site.is_strict(),
            Self::OpAllBinary(call_site) => call_site.is_strict(),
            Self::NullIfExpr(null_if) => null_if.is_strict(),
            Self::UndefinedOperator(_)
            | Self::UndefinedOperatorAny(_)
            | Self::UndefinedOperatorAll(_) => return None,
        };
        if is_strict {
            if matches!(self.inputs_is_not_null(), Some(true)) {
                if matches!(self.operator_returns_null(), Some(false)) {
                    Some(true)
                } else {
                    Some(false)
                }
            } else {
                Some(false)
            }
        } else {
            None
        }
    }
}
