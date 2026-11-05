use crate::{
    ast::{CurrentOf, Expr},
    context::ParseContext,
    names::TableName,
    sem::{AnalysisError, AnalysisProblem, DeleteCursor, problem::DeleteProblemKind},
};

impl DeleteCursor {
    pub(crate) fn from_ast_delete_current_of(
        ast: &Expr,
        context: &mut impl ParseContext,
        target: &TableName,
    ) -> Result<Option<Self>, AnalysisError> {
        match ast {
            Expr::CurrentOf(cursor) => Self::from_ast(cursor, context, target).map(Some),
            _ => Ok(None),
        }
    }
}

impl DeleteCursor {
    pub(crate) fn from_ast(
        ast: &CurrentOf,
        context: &mut impl ParseContext,
        target: &TableName,
    ) -> Result<Self, AnalysisError> {
        if ast.parameter != 0 {
            context.report_analysis_problem(AnalysisProblem::delete(
                DeleteProblemKind::cursor_parameter(ast.parameter),
            ))?;
            return Ok(Self::parameter(ast.parameter));
        }
        let cursor = Self::new(&ast.name, context.resolve_cursor(&ast.name));
        let problem = match cursor.binding() {
            None => Some(DeleteProblemKind::missing_cursor(&ast.name)),
            Some(info) if info.updatable() == Some(false) => {
                Some(DeleteProblemKind::non_updatable_cursor(&ast.name))
            }
            Some(info) => info
                .target()
                .filter(|relation| *relation != target)
                .map(|actual| DeleteProblemKind::cursor_target_mismatch(&ast.name, target, actual)),
        };
        if let Some(kind) = problem {
            context.report_analysis_problem(AnalysisProblem::delete(kind))?;
        }
        Ok(cursor)
    }
}
