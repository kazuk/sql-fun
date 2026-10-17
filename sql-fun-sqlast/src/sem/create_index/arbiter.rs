use crate::sem::{AnalysisError, CreateIndex, InsertConflict, SemScalarExpr};

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
            let expression = key.element.canonical_expression()?;
            if !keys.contains(&expression) {
                keys.push(expression);
            }
        }
        let mut inferred = Vec::new();
        for expression in conflict.inference() {
            let expression = canonical(expression)?;
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

impl super::IndexElement {
    fn canonical_expression(&self) -> Result<SemScalarExpr, AnalysisError> {
        match self {
            Self::ColumnName(name) => Ok(SemScalarExpr::index_column(name)),
            Self::Expr(expression) => canonical(expression),
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
