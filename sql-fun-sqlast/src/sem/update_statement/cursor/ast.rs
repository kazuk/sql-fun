use crate::{
    ast::{CurrentOf, Expr},
    context::ParseContext,
    names::TableName,
    sem::{
        AnalysisError, AnalysisProblem, ResolutionFailure, UpdateCursor, problem::UpdateProblemKind,
    },
};

impl UpdateCursor {
    pub(crate) fn from_ast_current_of(
        ast: &Expr,
        context: &mut impl ParseContext,
        target: &TableName,
    ) -> Result<Option<Self>, ResolutionFailure> {
        match ast {
            Expr::CurrentOf(cursor) => Self::from_ast(cursor, context, target).map(Some),
            _ => Ok(None),
        }
    }
}

impl UpdateCursor {
    pub(crate) fn from_ast(
        ast: &CurrentOf,
        context: &mut impl ParseContext,
        target: &TableName,
    ) -> Result<Self, ResolutionFailure> {
        if ast.parameter != 0 {
            return Err(AnalysisError::unexpected_input_error(
                "CURRENT OF cursor parameters require a procedural variable resolver",
            )
            .into());
        }
        let cursor = Self::new(&ast.name, context.resolve_cursor(&ast.name));
        let problem = match cursor.binding() {
            None => Some(format!(
                "cursor {} does not exist in the current session scope",
                crate::identifier::QuotedIdentifier::from(ast.name.as_str())
            )),
            Some(info) if info.updatable() == Some(false) => Some(format!(
                "cursor {} is not statically updatable",
                crate::identifier::QuotedIdentifier::from(ast.name.as_str())
            )),
            Some(info) if info.target().is_some_and(|relation| relation != target) => {
                Some(format!(
                    "cursor {} does not target {}",
                    crate::identifier::QuotedIdentifier::from(ast.name.as_str()),
                    target
                ))
            }
            Some(_) => None,
        };
        if let Some(message) = problem {
            context.report_analysis_problem(AnalysisProblem::update(
                UpdateProblemKind::cursor(),
                &message,
            ))?;
        }
        Ok(cursor)
    }
}
