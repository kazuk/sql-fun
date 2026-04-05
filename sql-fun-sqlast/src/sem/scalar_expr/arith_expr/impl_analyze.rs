use sql_fun_core::IVec;

use crate::{
    sem::{
        AnalysisError, FromClause, OperatorName, ParseContext, SemScalarExpr, WithClause,
        scalar_expr::AnalyzeScalarExpr,
    },
    syn::{AExprKind, Opt, ScanToken},
};

use super::ArithExpr;

impl AExprKind {
    fn has_name(&self) -> Result<bool, AnalysisError> {
        match self {
            Self::Undefined | Self::OptionNone => {
                AnalysisError::raise_unexpected_input("aexpr_kind")
            }
            Self::AexprOp | Self::AexprOpAny | Self::AexprOpAll => Ok(true),
            Self::AexprDistinct
            | Self::AexprNotDistinct
            | Self::AexprNullif
            | Self::AexprIn
            | Self::AexprLike
            | Self::AexprIlike
            | Self::AexprSimilar
            | Self::AexprBetween
            | Self::AexprNotBetween
            | Self::AexprBetweenSym
            | Self::AexprNotBetweenSym => Ok(false),
        }
    }
}

impl<TParseContext> AnalyzeScalarExpr<TParseContext, crate::syn::AExpr> for ArithExpr
where
    TParseContext: ParseContext,
{
    fn analyze_scalar_expr(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        syn: crate::syn::AExpr,
        tokens: &IVec<ScanToken>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        let Some(kind) = syn.get_kind().as_inner() else {
            AnalysisError::raise_unexpected_none("a_expr.kind")?
        };

        let (lexpr, new_context) = Self::analyze_optional_expr(
            context,
            with_clause,
            from_clause,
            syn.get_lexpr().as_inner(),
            tokens,
        )?;
        context = new_context;
        let (rexpr, new_context) = Self::analyze_optional_expr(
            context,
            with_clause,
            from_clause,
            syn.get_rexpr().as_inner(),
            tokens,
        )?;
        context = new_context;

        if kind.has_name()? {
            let Some(name) = OperatorName::try_from_report(syn.get_name(), &mut context)? else {
                AnalysisError::raise_unexpected_none("a_expr.name")?
            };
            match kind {
                AExprKind::AexprOp => Self::build_op(context, &name, lexpr, rexpr),
                AExprKind::AexprOpAny => Self::build_op_any(context, &name, lexpr, rexpr),
                AExprKind::AexprOpAll => Self::build_op_all(context, &name, lexpr, rexpr),
                _ => AnalysisError::raise_unexpected_input("a_expr.kind")?,
            }
        } else {
            match kind {
                AExprKind::AexprDistinct => Self::build_distinct(context, lexpr, rexpr),
                AExprKind::AexprNotDistinct => Self::build_not_distinct(context, lexpr, rexpr),
                AExprKind::AexprNullif => Self::build_null_if(context, lexpr, rexpr),
                AExprKind::AexprIn => Self::build_in(context, lexpr, rexpr),
                AExprKind::AexprLike => Self::build_like(context, lexpr, rexpr),
                AExprKind::AexprIlike => Self::build_ilike(context, lexpr, rexpr),
                AExprKind::AexprSimilar => Self::build_similar(context, lexpr, rexpr),
                AExprKind::AexprBetween => Self::build_between(context, lexpr, rexpr),
                AExprKind::AexprNotBetween => Self::build_not_between(context, lexpr, rexpr),
                AExprKind::AexprBetweenSym => Self::build_between_sym(context, lexpr, rexpr),
                AExprKind::AexprNotBetweenSym => Self::build_not_between_sym(context, lexpr, rexpr),
                _ => AnalysisError::raise_unexpected_input("a_expr.kind")?,
            }
        }
    }
}
