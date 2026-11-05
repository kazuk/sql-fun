//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::{Expr as AstExpr, ddl::AlterDefinition as AstAlterDefinition},
    context::ParseContext,
    sem::{
        AnalysisError, FromClause, ResolutionFailure, ScalarAnalysisContext, SemScalarExpr,
        WithClause,
    },
};

impl SemScalarExpr {
    pub(crate) fn from_ast_view_default_definition(
        ast: &AstAlterDefinition,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, ResolutionFailure> {
        match ast {
            AstAlterDefinition::None => Ok(None),
            AstAlterDefinition::Expression(value) => {
                Self::from_ast_view_default_expression(value, context)
            }
            _ => {
                AnalysisError::raise_unexpected_input("view default expression").map_err(Into::into)
            }
        }
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_view_default_expression(
        ast: &AstExpr,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, ResolutionFailure> {
        Self::from_ast(
            ast,
            context,
            &ScalarAnalysisContext::new(&WithClause::default(), &FromClause::default()),
        )
        .map(Some)
        .map_err(Into::into)
    }
}
