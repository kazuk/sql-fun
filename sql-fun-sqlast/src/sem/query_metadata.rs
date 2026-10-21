//! Read-only facts derived from the existing completed semantic traversal.
use crate::sem::{AnalysisError, FullName, SemAst};

/// Relation identities already established by semantic analysis.
/// This does not add CRUD attribution or repeat name lookup.
#[derive(Default)]
pub struct QueryMetadata {
    relations: Vec<FullName>,
}

impl QueryMetadata {
    /// Inspect an analyzed SELECT or DML statement without changing its model.
    pub fn from_statement(statement: &SemAst) -> Result<Self, AnalysisError> {
        let mut metadata = Self::default();
        if let Some(merge) = statement.as_merge() {
            merge.clone().walk(&mut metadata)?;
        } else if statement.result_columns().is_some() {
            statement.clone().walk_nested_command(&mut metadata)?;
        }
        Ok(metadata)
    }
    /// Distinct relation identities in semantic traversal order.
    #[must_use]
    pub fn relations(&self) -> &[FullName] {
        &self.relations
    }
}
impl crate::sem::traversal::SemanticVisitor for QueryMetadata {
    fn relation_identity(&mut self, name: &FullName) {
        if !self.relations.contains(name) {
            self.relations.push(name.clone());
        }
    }
}

impl QueryMetadata {
    /// Report unresolved parameter finalization through the shared diagnostic owner.
    /// This inspects existing inference results and does not solve constraints again.
    /// Contradictory (empty) domains retain their existing conflict diagnostics.
    pub fn report_unresolved_parameters(
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<(), AnalysisError> {
        let unresolved: Vec<_> = context
            .parameter_constraints()
            .iter()
            .filter(|parameter| {
                let domain = parameter.constraint();
                domain.single_type().is_none()
                    && domain
                        .admissible_types()
                        .is_none_or(|types| !types.is_empty())
            })
            .map(crate::sem::ParameterConstraint::position)
            .collect();
        for index in unresolved {
            context.report_problem(crate::sem::AnalysisProblem::parameter_type_unresolved(
                index,
            ))?;
        }
        Ok(())
    }
}
