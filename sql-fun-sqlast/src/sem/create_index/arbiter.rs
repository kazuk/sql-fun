use crate::sem::{AnalysisError, ColumnName, CreateIndex, InsertConflict, SemScalarExpr};

impl CreateIndex {
    pub(crate) fn matches_inference(
        &self,
        conflict: &InsertConflict,
    ) -> Result<Option<bool>, AnalysisError> {
        if !self.unique {
            return Ok(Some(false));
        }
        let mut keys = Vec::new();
        for key in &self.key_columns.0 {
            let expression = key.element.comparison_key()?;
            if !keys.contains(&expression) {
                keys.push(expression);
            }
        }
        let mut inferred = Vec::new();
        for expression in conflict.inference() {
            let expression = IndexKey::from_expression(expression)?;
            if !inferred.contains(&expression) {
                inferred.push(expression);
            }
        }
        if keys.len() != inferred.len() || !keys.iter().all(|key| inferred.contains(key)) {
            return Ok(Some(false));
        }
        if let Some(predicate) = &self.predicate {
            let Some(inferred) = conflict.predicate() else {
                return Ok(Some(false));
            };
            // Exact predicates are sufficient for both dialects. More general
            // PostgreSQL implication rules need a predicate prover.
            if canonical(predicate)? != canonical(inferred)? {
                return Ok(None);
            }
        }
        Ok(Some(true))
    }
}

/// A comparison key belongs to index inference, not to the retained SQL tree.
#[derive(PartialEq, Eq)]
enum IndexKey {
    Column(ColumnName),
    Expression(Box<SemScalarExpr>),
}
impl IndexKey {
    fn column(name: &ColumnName) -> Self {
        Self::Column(name.clone())
    }
    fn from_expression(expression: &SemScalarExpr) -> Result<Self, AnalysisError> {
        if let Some(name) = expression.index_column_name() {
            Ok(Self::column(name))
        } else {
            Ok(Self::Expression(Box::new(canonical(expression)?)))
        }
    }
}

impl super::IndexElement {
    fn comparison_key(&self) -> Result<IndexKey, AnalysisError> {
        match self {
            Self::ColumnName(name) => Ok(IndexKey::column(name)),
            Self::Expr(expression) => IndexKey::from_expression(expression),
        }
    }
}

fn canonical(expression: &SemScalarExpr) -> Result<SemScalarExpr, AnalysisError> {
    let mut expression = expression.clone();
    expression.walk(&mut Canonicalizer)?;
    Ok(expression)
}

struct Canonicalizer;
impl crate::sem::traversal::SemanticVisitor for Canonicalizer {
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        expression.canonicalize_index_binding();
        Ok(())
    }
}
