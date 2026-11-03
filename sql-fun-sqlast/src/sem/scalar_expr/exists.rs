use crate::sem::{
    AnalysisError, PgBuiltInType, SelectStatement, TypeReference,
    scalar_expr::SemScalarExprNode,
    traversal::{SemanticVisitor, SemanticVisitorMut},
};

/// An EXISTS predicate whose result is non-null boolean, independently of its
/// query's projection types, column count, or number of rows.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExistsExpr {
    query: Box<SelectStatement>,
}

impl From<&SelectStatement> for ExistsExpr {
    fn from(query: &SelectStatement) -> Self {
        Self {
            query: Box::new(query.clone()),
        }
    }
}

impl ExistsExpr {
    /// The analyzed query, including its resolved references and predicates.
    pub fn query(&self) -> &SelectStatement {
        &self.query
    }
}

impl SemScalarExprNode for ExistsExpr {
    fn get_type(&self) -> Option<TypeReference> {
        Some(PgBuiltInType::bool())
    }

    fn is_not_null(&self) -> Option<bool> {
        Some(true)
    }
}

impl ExistsExpr {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.query.walk(visitor)
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        self.query.visit(visitor)
    }
}
