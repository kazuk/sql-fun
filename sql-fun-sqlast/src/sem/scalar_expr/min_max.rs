use crate::sem::scalar_expr::SemScalarExprNode;
use crate::sem::{
    AnalysisError, AnalysisProblem, InputTypeConstraint, ParseContext, PgBuiltInType,
    SemScalarExpr, TypeReference,
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
            context.report_analysis_problem(AnalysisProblem::min_max(
                crate::sem::problem::MinMaxProblem::empty(),
            ))?;
        } else if arguments.iter().all(|arg| Self::input_type(arg).is_some()) {
            let inputs: Vec<_> = arguments.iter().filter_map(Self::input_type).collect();
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

    pub(crate) fn input_type(value: &SemScalarExpr) -> Option<TypeReference> {
        // Bare PostgreSQL strings and NULL have type unknown, unlike typed
        // strings or the legacy generic constant accessor's text approximation.
        if value
            .as_const()
            .is_some_and(|constant| constant.is_null() || constant.as_string().is_some())
        {
            return Some(Self::unknown());
        }
        value.get_type()
    }
    pub(crate) fn unknown() -> TypeReference {
        TypeReference::from_master_type_name("unknown")
    }

    pub(crate) fn base_type(context: &impl ParseContext, ty: &TypeReference) -> TypeReference {
        // An array of domains is an array type, not itself a domain.
        if ty.is_array() == Some(true) {
            return ty.clone();
        }
        let mut base = ty.clone();
        let mut seen = std::collections::HashSet::new();
        while !base.is_dynamic() && base.is_array() != Some(true) && seen.insert(base.clone()) {
            let Some(definition) = context.get_type_impl(base.full_name()) else {
                break;
            };
            let Some(reference) = definition.domain_base() else {
                break;
            };
            base = reference.clone();
        }
        base
    }

    pub(crate) fn category(
        context: &impl ParseContext,
        ty: &TypeReference,
    ) -> Result<(char, bool), crate::sem::problem::MinMaxProblem> {
        if ty.is_array() == Some(true) {
            return Ok(('A', false));
        }
        let definition = if let Some(definition) = ty.type_definition() {
            Some(definition)
        } else {
            context.get_type_impl(ty.full_name())
        };
        let category = if let Some(definition) = definition {
            definition.category(context)
        } else {
            None
        };
        let category =
            category.ok_or_else(|| crate::sem::problem::MinMaxProblem::missing_type_facts(ty))?;
        if ty.is_dynamic() {
            return Ok((category.code(), false));
        }
        let preferred = context.cast_sources(ty, false).into_iter().any(|source| {
            context
                .get_implicit_cast(&source, ty)
                .is_some_and(|cast| cast.is_implicit_preferred())
        });
        Ok((category.code(), preferred))
    }

    pub(crate) fn can_coerce(
        context: &impl ParseContext,
        source: &TypeReference,
        target: &TypeReference,
    ) -> bool {
        if source == target || *source == Self::unknown() {
            return true;
        }
        let source = Self::base_type(context, source);
        if source == *target || source.implicit_match_context(target, context).is_some() {
            return true;
        }
        if source.is_array() == Some(true) && target.is_array() == Some(true) {
            return Self::can_coerce(context, &source.element_type(), &target.element_type());
        }
        false
    }

    // Candidate selection does not validate conversions yet: a later input
    // can change the candidate. The final target must accept every operand.
    pub(crate) fn next_candidate(
        context: &impl ParseContext,
        candidate: &TypeReference,
        input: &TypeReference,
    ) -> Result<TypeReference, crate::sem::problem::MinMaxProblem> {
        if *candidate == Self::unknown() {
            return Ok(input.clone());
        }
        if *input == Self::unknown() {
            return Ok(candidate.clone());
        }
        Self::select_candidate(
            candidate,
            input,
            Self::category(context, candidate)?,
            Self::category(context, input)?,
            Self::can_coerce(context, candidate, input),
            Self::can_coerce(context, input, candidate),
        )
    }

    pub(crate) fn select_candidate(
        candidate: &TypeReference,
        input: &TypeReference,
        (category, preferred): (char, bool),
        (input_category, _): (char, bool),
        forward: bool,
        backward: bool,
    ) -> Result<TypeReference, crate::sem::problem::MinMaxProblem> {
        if category != input_category {
            return Err(crate::sem::problem::MinMaxProblem::incompatible(
                candidate, input,
            ));
        }
        if !preferred && forward && !backward {
            return Ok(input.clone());
        }
        Ok(candidate.clone())
    }

    pub(crate) fn ordered(context: &impl ParseContext, ty: &TypeReference) -> Option<bool> {
        let base = Self::base_type(context, ty);
        if base.is_array() == Some(true) {
            return Self::ordered(context, &base.element_type());
        }
        let definition = if let Some(definition) = base.type_definition() {
            Some(definition)
        } else {
            context.get_type_impl(base.full_name())
        };
        if definition.is_some_and(|definition| definition.has_structural_ordering()) {
            return Some(true);
        }
        // PostgreSQL's default B-tree classes for these concrete families
        // have polymorphic input types, not the concrete result type's name.
        let direct = context.builtin_type_ordered(base.full_name());
        if direct == Some(true) {
            return direct;
        }
        let (category, _) = Self::category(context, &base).ok()?;
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
            return context
                .builtin_type_ordered(TypeReference::from_master_type_name(family).full_name());
        }
        direct
    }

    pub(crate) fn common_type(
        context: &impl ParseContext,
        inputs: &[TypeReference],
    ) -> Result<TypeReference, crate::sem::problem::MinMaxProblem> {
        let target = Self::select_common_type(context, inputs)?;
        match Self::ordered(context, &target) {
            Some(true) => {}
            Some(false) => return Err(crate::sem::problem::MinMaxProblem::not_ordered(&target)),
            None => {
                return Err(crate::sem::problem::MinMaxProblem::missing_comparison_facts(&target));
            }
        }
        Ok(target)
    }

    /// PostgreSQL ordered common-type selection, without comparison requirements.
    pub(crate) fn select_common_type(
        context: &impl ParseContext,
        inputs: &[TypeReference],
    ) -> Result<TypeReference, crate::sem::problem::MinMaxProblem> {
        let Some(first) = inputs.first() else {
            return Err(crate::sem::problem::MinMaxProblem::empty());
        };
        let target = if *first != Self::unknown() && inputs.iter().all(|ty| ty == first) {
            first.clone()
        } else {
            let mut candidate = Self::unknown();
            for input in inputs {
                candidate =
                    Self::next_candidate(context, &candidate, &Self::base_type(context, input))?;
            }
            if candidate == Self::unknown() {
                PgBuiltInType::text()
            } else {
                candidate
            }
        };
        for input in inputs {
            if !Self::can_coerce(context, input, &target) {
                return Err(crate::sem::problem::MinMaxProblem::incompatible(
                    input, &target,
                ));
            }
        }
        Ok(target)
    }

    fn coerce(&mut self, target: &TypeReference) {
        for argument in &mut self.arguments {
            if argument.get_type().as_ref() != Some(target)
                || Self::input_type(argument) == Some(Self::unknown())
            {
                *argument = SemScalarExpr::wrap_implicit_cast(target, argument);
            }
        }
    }

    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for argument in &mut self.arguments {
            argument.walk(visitor)?;
        }
        Ok(())
    }

    pub(crate) fn collect_constraints(
        &mut self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
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
    pub(crate) fn apply_constraints(&mut self, constraints: &crate::sem::ParameterConstraints) {
        if let Some(node) = self.node {
            self.types = constraints.node_domain(node).clone();
            if let Some(ty) = self.types.single_type() {
                self.coerce(&ty);
            }
        }
    }
}

#[cfg(test)]
mod comparison_knowledge_tests {
    use super::MinMaxExpr;
    use crate::sem::{AnalysisProblem, PgBuiltInType, ScalarConstExpr, SemScalarExpr};
    use crate::test_helpers::TestParseContext;

    #[test]
    fn absent_comparison_knowledge_reports_missing_metadata() {
        let context = TestParseContext::default();
        let problem = MinMaxExpr::common_type(&context, &[PgBuiltInType::int4()]).unwrap_err();
        assert_eq!(
            AnalysisProblem::min_max(problem).code(),
            "min_max_missing_comparison_facts"
        );
    }

    #[test]
    fn absent_comparison_knowledge_does_not_eliminate_parameter_types() {
        let mut context = TestParseContext::default();
        let mut parameters = crate::sem::parameter_resolution::ParameterResolution::default();
        let arguments = [
            parameters.parameter(1),
            SemScalarExpr::new_const(ScalarConstExpr::new_integer(1)),
        ];
        let (_, types) = parameters
            .min_max(&mut context, "greatest", &arguments)
            .unwrap();
        assert_eq!(types.single_type(), Some(PgBuiltInType::int4()));
        assert_eq!(context.reported_problems().len(), 1);
        assert_eq!(
            context.reported_problems()[0].code(),
            "min_max_missing_comparison_facts"
        );
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
