use crate::sem::{
    AnalysisError, AnalysisProblem, CommonType, InputTypeConstraint, ParameterConstraints,
    ParseContext, SemScalarExpr, TypeReference,
    parameter_resolution::ParameterResolution,
    problem::MinMaxProblem,
    scalar_expr::SemScalarExprNode,
    traversal::{SemanticVisitor, SemanticVisitorMut},
};

/// PostgreSQL GREATEST or LEAST, retaining analyzed operands and output constraints.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MinMaxExpr {
    operation: String,
    arguments: Vec<SemScalarExpr>,
    types: InputTypeConstraint,
    #[serde(skip)]
    node: Option<usize>,
}

impl MinMaxExpr {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        operation: &str,
        arguments: &[SemScalarExpr],
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut value = Self {
            operation: operation.to_owned(),
            arguments: arguments.to_vec(),
            types: InputTypeConstraint::any(),
            node: None,
        };
        if arguments.is_empty() {
            context.report_analysis_problem(AnalysisProblem::min_max(MinMaxProblem::empty()))?;
        } else if arguments
            .iter()
            .all(|arg| SemScalarExpr::common_type_input(arg).is_some())
        {
            let inputs: Vec<_> = arguments
                .iter()
                .filter_map(SemScalarExpr::common_type_input)
                .collect();
            match Self::common_type(context, &inputs) {
                Ok(ty) => {
                    value.types = InputTypeConstraint::exact(&ty);
                    value.coerce(&ty);
                }
                Err(problem) => {
                    context.report_analysis_problem(AnalysisProblem::min_max(problem))?
                }
            }
        }
        Ok(SemScalarExpr::min_max(&value))
    }

    /// Operation identity (`greatest` or `least`), independent of function lookup.
    pub fn operation(&self) -> &str {
        &self.operation
    }
    /// Ordered analyzed operands, including implicit coercions.
    pub fn arguments(&self) -> &[SemScalarExpr] {
        &self.arguments
    }
    /// SQL-derived output domain, including correlated parameter alternatives.
    pub const fn types(&self) -> &InputTypeConstraint {
        &self.types
    }
    pub(crate) const fn node(&self) -> Option<usize> {
        self.node
    }

    pub(crate) fn ordered(context: &impl ParseContext, ty: &TypeReference) -> Option<bool> {
        let base = CommonType::base_type(context, ty);
        if base.is_array() == Some(true) {
            return Self::ordered(context, &base.element_type());
        }
        let definition = if let Some(definition) = base.type_definition() {
            Some(definition)
        } else {
            context.get_type_impl(
                base.full_name()
                    .expect("owned definitions are handled before nominal lookup"),
            )
        };
        if definition.is_some_and(|definition| definition.has_structural_ordering()) {
            return Some(true);
        }
        // PostgreSQL's default B-tree classes for these concrete families
        // have polymorphic input types, not the concrete result type's name.
        let direct = if let Some(name) = base.full_name() {
            context.builtin_type_ordered(name)
        } else {
            None
        };
        if direct == Some(true) {
            return direct;
        }
        let (category, _) = CommonType::category(context, &base).ok()?;
        let range_family = category == 'R';
        let family = if range_family && base.is_range() {
            Some("anyrange")
        } else if range_family && base.is_multi_range() {
            Some("anymultirange")
        } else if base == TypeReference::from_master_type_name("int2vector") {
            // int2vector uses array comparison over int2 despite having a
            // distinct nominal spelling in the legacy type representation.
            Some("anyarray")
        } else {
            None
        };
        if let Some(family) = family {
            return context.builtin_type_ordered(
                TypeReference::from_master_type_name(family)
                    .full_name()
                    .expect("owned definitions are handled before nominal lookup"),
            );
        }
        direct
    }

    pub(crate) fn common_type(
        context: &impl ParseContext,
        inputs: &[TypeReference],
    ) -> Result<TypeReference, MinMaxProblem> {
        let target = CommonType::select_common_type(context, inputs)?;
        match Self::ordered(context, &target) {
            Some(true) => {}
            Some(false) => return Err(MinMaxProblem::not_ordered(&target)),
            None => {
                return Err(MinMaxProblem::missing_comparison_facts(&target));
            }
        }
        Ok(target)
    }
}

#[cfg(test)]
mod comparison_knowledge_tests {

    use crate::{
        sem::{AnalysisProblem, PgBuiltInType, scalar_expr::min_max::MinMaxExpr},
        test_helpers::TestParseContext,
    };

    #[test]
    fn absent_comparison_knowledge_reports_missing_metadata() {
        let context = TestParseContext::default();
        let problem = MinMaxExpr::common_type(&context, &[PgBuiltInType::int4()]).unwrap_err();
        assert_eq!(
            AnalysisProblem::min_max(problem).code(),
            "min_max_missing_comparison_facts"
        );
    }
}

impl MinMaxExpr {
    fn coerce(&mut self, target: &TypeReference) {
        for argument in &mut self.arguments {
            if argument.get_type().as_ref() != Some(target)
                || SemScalarExpr::common_type_input(argument) == Some(CommonType::unknown())
            {
                *argument = SemScalarExpr::wrap_implicit_cast(target, argument);
            }
        }
    }

    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for argument in &mut self.arguments {
            argument.walk(visitor)?;
        }
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        for argument in &self.arguments {
            argument.visit(visitor)?;
        }
        Ok(())
    }

    pub(crate) fn collect_constraints(
        &mut self,
        parameters: &mut ParameterResolution<'_>,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if self
            .arguments
            .iter()
            .any(|value| value.constraint_node().is_some())
        {
            let (node, types) = parameters.min_max(context, &self.operation, &self.arguments)?;
            self.node = Some(node);
            self.types = types;
        }
        Ok(())
    }
    pub(super) const fn canonicalize_grouping_key(&mut self) {
        self.node = None;
    }
    pub(crate) fn apply_constraints(&mut self, constraints: &ParameterConstraints) {
        if let Some(node) = self.node {
            self.types = constraints.node_domain(node).clone();
            if let Some(ty) = self.types.single_type() {
                self.coerce(&ty);
            }
        }
    }
}

impl SemScalarExprNode for MinMaxExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.types.single_type()
    }
    fn is_not_null(&self) -> Option<bool> {
        if self
            .arguments
            .iter()
            .any(|value| value.is_not_null() == Some(true))
        {
            Some(true)
        } else if self
            .arguments
            .iter()
            .all(|value| value.is_not_null().is_some())
        {
            Some(false)
        } else {
            None
        }
    }
}
