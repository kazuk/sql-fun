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
        while seen.insert(base.clone()) {
            let Some(definition) = context.get_type_impl(base.full_name()) else {
                break;
            };
            let Some(domain_base) = definition.domain_base() else {
                break;
            };
            let reference = TypeReference::from_full_name(domain_base.clone());
            base = context
                .canonicalize_type_ref(&reference)
                .unwrap_or(reference);
        }
        base
    }

    fn category(
        context: &impl ParseContext,
        ty: &TypeReference,
    ) -> Result<(char, bool), crate::sem::problem::MinMaxProblem> {
        if ty.is_array() == Some(true) {
            return Ok(('A', false));
        }
        if let Some(definition) = context.get_type_impl(ty.full_name())
            && let Some(category) = definition.user_type_category()
        {
            return Ok((category, false));
        }
        context
            .builtin_type_category(ty.full_name())
            .ok_or_else(|| crate::sem::problem::MinMaxProblem::missing_type_facts(ty))
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
        if source == *target || context.get_implicit_cast(&source, target).is_some() {
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
        let (category, preferred) = Self::category(context, candidate)?;
        if category != Self::category(context, input)?.0 {
            return Err(crate::sem::problem::MinMaxProblem::incompatible(
                candidate, input,
            ));
        }
        if !preferred
            && Self::can_coerce(context, candidate, input)
            && !Self::can_coerce(context, input, candidate)
        {
            return Ok(input.clone());
        }
        Ok(candidate.clone())
    }

    pub(crate) fn ordered(context: &impl ParseContext, ty: &TypeReference) -> bool {
        let base = Self::base_type(context, ty);
        if base.is_array() == Some(true) {
            return Self::ordered(context, &base.element_type());
        }
        if context
            .get_type_impl(base.full_name())
            .is_some_and(|definition| definition.has_structural_ordering())
        {
            return true;
        }
        context.builtin_type_ordered(base.full_name())
    }

    pub(crate) fn common_type(
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
        if !Self::ordered(context, &target) {
            return Err(crate::sem::problem::MinMaxProblem::not_ordered(&target));
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
