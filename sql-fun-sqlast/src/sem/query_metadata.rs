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
    /// Report unresolved application input bindings through the shared diagnostic owner.
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

#[cfg(test)]
mod unresolved_parameter_tests {
    use super::QueryMetadata;
    use crate::{
        sem::{
            InputTypeConstraint, ParameterConstraints, ParseContext, PgBuiltInType, TypeReference,
            parameter_constraints::ConstraintGraph,
        },
        test_helpers::TestParseContext,
    };

    #[rstest::rstest]
    #[case::unrestricted(None, true)]
    #[case::non_singleton(Some(2), true)]
    #[case::singleton(Some(1), false)]
    #[case::empty(Some(0), false)]
    fn finalization_preserves_domains_and_reports_only_missing_bindings(
        #[case] candidate_count: Option<usize>,
        #[case] unresolved: bool,
    ) -> Result<(), crate::sem::AnalysisError> {
        let types = [PgBuiltInType::int4(), PgBuiltInType::text()]
            .map(|ty| TypeReference::concrete_type_ref(ty.full_name(), false));
        let domain = if let Some(count) = candidate_count {
            InputTypeConstraint::types(types[..count].to_vec())
        } else {
            InputTypeConstraint::any()
        };
        let mut graph = ConstraintGraph::default();
        let parameter = graph.parameter(2);
        graph.restrict(parameter, &domain);
        let constraints = ParameterConstraints::from_graph(&graph);
        let mut context = TestParseContext::default();
        context.set_parameter_constraints(&constraints);

        QueryMetadata::report_unresolved_parameters(&mut context)?;

        assert_eq!(
            context
                .parameter_constraints()
                .parameter(2)
                .unwrap()
                .constraint(),
            &domain
        );
        assert_eq!(context.reported_problem_count(), usize::from(unresolved));
        if unresolved {
            assert!(context.has_reported_problem("parameter_type_unresolved"));
        }
        Ok(())
    }
}
