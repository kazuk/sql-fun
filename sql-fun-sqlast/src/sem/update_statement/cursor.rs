use crate::{
    ast::{CurrentOf, Expr},
    context::ParseContext,
    names::TableName,
    sem::{
        AnalysisError, AnalysisProblem, CursorInfo, ResolutionFailure, SemScalarExpr,
        problem::UpdateProblemKind,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

/// A positioned UPDATE's portal reference and lookup result in its session scope.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UpdateCursor {
    name: String,
    binding: Option<CursorInfo>,
}
impl UpdateCursor {
    pub(crate) fn new(name: &str, binding: Option<&CursorInfo>) -> Self {
        Self {
            name: name.to_owned(),
            binding: binding.cloned(),
        }
    }
    /// Portal name retained from CURRENT OF, after SQL identifier normalization.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Resolved static facts, or None for a missing current-session binding.
    /// A binding with unknown facts is still a resolved reference.
    pub const fn binding(&self) -> Option<&CursorInfo> {
        self.binding.as_ref()
    }
}

/// Predicate alternatives are exclusive, as in PostgreSQL's UPDATE grammar.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) enum UpdatePredicate {
    Expression(Box<SemScalarExpr>),
    CurrentOf(UpdateCursor),
}
impl UpdatePredicate {
    pub(crate) fn expression(value: &SemScalarExpr) -> Self {
        Self::Expression(Box::new(value.clone()))
    }
    pub(crate) fn current_of(value: &UpdateCursor) -> Self {
        Self::CurrentOf(value.clone())
    }
    pub(crate) fn as_expression(&self) -> Option<&SemScalarExpr> {
        match self {
            Self::Expression(value) => Some(value),
            Self::CurrentOf(_) => None,
        }
    }
    pub(crate) const fn as_cursor(&self) -> Option<&UpdateCursor> {
        match self {
            Self::CurrentOf(cursor) => Some(cursor),
            Self::Expression(_) => None,
        }
    }
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Expression(value) => {
                value.walk(visitor)?;
                visitor.predicate(value);
                Ok(())
            }
            Self::CurrentOf(_) => Ok(()),
        }
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        match self {
            Self::Expression(value) => {
                value.visit(visitor)?;
                visitor.predicate(value);
                Ok(())
            }
            Self::CurrentOf(_) => Ok(()),
        }
    }
}

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
