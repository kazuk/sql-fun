//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::Expr as AstExpr,
    context::ParseContext,
    names::{ColumnName, TableName},
    sem::{AnalysisError, DefaultColumnConstraint, ScalarAnalysisContext, SemScalarExpr},
};

impl DefaultColumnConstraint {
    pub(in crate::sem::column_constraints) fn from_ast_expression(
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        expression: &AstExpr,
    ) -> Result<Self, AnalysisError> {
        Ok(Self::new(
            table,
            column,
            SemScalarExpr::from_ast_in(expression, context, &ScalarAnalysisContext::default())?,
        ))
    }
}
