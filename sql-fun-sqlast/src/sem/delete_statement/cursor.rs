mod ast;

use crate::sem::{
    AnalysisError, CursorInfo, SemScalarExpr,
    traversal::{SemanticVisitor, SemanticVisitorMut},
};

/// A positioned DELETE's portal reference and lookup result in its session scope.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeleteCursor {
    name: String,
    binding: Option<CursorInfo>,
    parameter: Option<i32>,
}
impl DeleteCursor {
    pub(crate) fn new(name: &str, binding: Option<&CursorInfo>) -> Self {
        Self {
            name: name.to_owned(),
            binding: binding.cloned(),
            parameter: None,
        }
    }
    pub(crate) const fn parameter(number: i32) -> Self {
        Self {
            name: String::new(),
            binding: None,
            parameter: Some(number),
        }
    }
    /// Procedural cursor parameter number, when static name lookup is unavailable.
    pub const fn parameter_number(&self) -> Option<i32> {
        self.parameter
    }
    /// Portal name retained from CURRENT OF, after SQL identifier normalization.
    /// Empty for a procedural parameter; inspect `parameter_number` in that case.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Resolved static facts, or None for a missing current-session binding.
    /// A binding with unknown facts is still a resolved reference.
    pub const fn binding(&self) -> Option<&CursorInfo> {
        self.binding.as_ref()
    }
}

/// Predicate alternatives are exclusive, as in PostgreSQL's DELETE grammar.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) enum DeletePredicate {
    Expression(Box<SemScalarExpr>),
    CurrentOf(DeleteCursor),
}
impl DeletePredicate {
    pub(crate) fn expression(value: &SemScalarExpr) -> Self {
        Self::Expression(Box::new(value.clone()))
    }
    pub(crate) fn current_of(value: &DeleteCursor) -> Self {
        Self::CurrentOf(value.clone())
    }
    pub(crate) fn as_expression(&self) -> Option<&SemScalarExpr> {
        match self {
            Self::Expression(value) => Some(value),
            Self::CurrentOf(_) => None,
        }
    }
    pub(crate) const fn as_cursor(&self) -> Option<&DeleteCursor> {
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
