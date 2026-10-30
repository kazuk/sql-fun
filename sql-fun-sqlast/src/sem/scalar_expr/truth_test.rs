use crate::sem::scalar_expr::SemScalarExprNode;
use crate::sem::{
    AnalysisError, AnalysisProblem, InputTypeConstraint, MinMaxExpr, ParameterConstraints,
    ParseContext, PgBuiltInType, SemScalarExpr, TypeReference, problem::TruthTestProblem,
};

/// A SQL truth predicate with its analyzed operand and boolean coercion.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TruthTestExpr {
    argument: Box<SemScalarExpr>,
    value: Option<bool>,
    negated: bool,
    types: InputTypeConstraint,
    #[serde(skip)]
    node: Option<usize>,
}

impl TruthTestExpr {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        argument: &SemScalarExpr,
        value: Option<bool>,
        negated: bool,
    ) -> Result<Self, AnalysisError> {
        let mut result = Self {
            argument: Box::new(argument.clone()),
            value,
            negated,
            types: InputTypeConstraint::exact(&PgBuiltInType::bool()),
            node: None,
        };
        if let Some(ty) = MinMaxExpr::input_type(argument) {
            if ty == MinMaxExpr::unknown() {
                if let Some(literal) = argument.as_const()
                    && let Some(text) = literal.as_string()
                    && !Self::valid_boolean_input(text)
                {
                    result.fail(context, TruthTestProblem::invalid_literal(text))?;
                } else {
                    // Normalize unknown literals before graph collection: their
                    // apparent text type is not a typed SQL text operand.
                    result.coerce();
                }
            } else if !Self::accepts_type(context, &ty) {
                result.fail(context, TruthTestProblem::operand_type(&ty))?;
            }
        }
        if argument
            .as_truth_test()
            .is_some_and(|test| test.types.admissible_types() == Some(&[]))
        {
            result.types = InputTypeConstraint::types([]);
        }
        // Parameter-free analyses intentionally skip the inference pass.
        if !context.parameter_inference_needed() && result.types.single_type().is_some() {
            result.coerce();
        }
        Ok(result)
    }

    fn accepts_type(context: &impl ParseContext, ty: &TypeReference) -> bool {
        let canonical = if ty.is_dynamic() {
            ty.clone()
        } else {
            context
                .canonicalize_type_ref(ty)
                .unwrap_or_else(|| ty.clone())
        };
        MinMaxExpr::base_type(context, &canonical)
            .implicit_match_context(&PgBuiltInType::bool(), context)
            .is_some_and(|cast| cast.use_in_implicit())
    }

    pub(crate) fn input_types(context: &impl ParseContext) -> InputTypeConstraint {
        let target = PgBuiltInType::bool();
        let mut sources = context.cast_sources(&target, false);
        // As in common-type resolution, domain inputs are checked through
        // their base type, while the admissible parameter identity stays nominal.
        for ty in context.type_references() {
            if Self::accepts_type(context, &ty) {
                if !ty.is_dynamic()
                    && let Some(canonical) = context.canonicalize_type_ref(&ty)
                {
                    sources.insert(canonical);
                }
                sources.insert(ty);
            }
        }
        InputTypeConstraint::types(sources)
    }

    fn fail(
        &mut self,
        context: &mut impl ParseContext,
        problem: TruthTestProblem,
    ) -> Result<(), AnalysisError> {
        self.types = InputTypeConstraint::types([]);
        context.report_analysis_problem(AnalysisProblem::truth_test(problem))
    }

    // PostgreSQL boolin accepts unique case-insensitive prefixes of these
    // words, plus 0/1, with ASCII whitespace at either end. "o" is ambiguous.
    fn valid_boolean_input(text: &str) -> bool {
        let text = text.trim_matches([' ', '\t', '\n', '\r', '\u{000b}', '\u{000c}']);
        matches!(
            text.to_ascii_lowercase().as_str(),
            "t" | "tr"
                | "tru"
                | "true"
                | "f"
                | "fa"
                | "fal"
                | "fals"
                | "false"
                | "y"
                | "ye"
                | "yes"
                | "n"
                | "no"
                | "on"
                | "of"
                | "off"
                | "1"
                | "0"
        )
    }

    /// Analyzed operand, including its boolean coercion when valid.
    #[must_use]
    pub fn argument(&self) -> &SemScalarExpr {
        &self.argument
    }

    /// Tested truth value; `None` denotes SQL UNKNOWN.
    #[must_use]
    pub const fn value(&self) -> Option<bool> {
        self.value
    }

    /// Whether this is an IS NOT truth predicate.
    #[must_use]
    pub const fn negated(&self) -> bool {
        self.negated
    }

    /// Boolean result domain, or an empty domain for impossible inputs.
    #[must_use]
    pub const fn types(&self) -> &InputTypeConstraint {
        &self.types
    }

    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.argument.walk(visitor)
    }

    pub(crate) fn collect_constraints(
        &mut self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution<'_>,
        context: &impl ParseContext,
    ) {
        let (node, types) = parameters.truth_test(context, &self.argument, &self.types);
        self.node = Some(node);
        self.types = types;
    }

    pub(crate) const fn node(&self) -> Option<usize> {
        self.node
    }

    pub(crate) fn apply_constraints(&mut self, constraints: &ParameterConstraints) {
        if let Some(node) = self.node {
            self.types = constraints.node_domain(node).clone();
        }
        if self.types.single_type().is_some() {
            self.coerce();
        }
    }

    fn coerce(&mut self) {
        if self.argument.get_type() != Some(PgBuiltInType::bool()) {
            *self.argument =
                SemScalarExpr::wrap_implicit_cast(&PgBuiltInType::bool(), &self.argument);
        }
    }

    pub(super) const fn canonicalize_grouping_key(&mut self) {
        self.node = None;
    }
}

#[cfg(test)]
mod tests;

impl SemScalarExprNode for TruthTestExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.types.single_type()
    }

    fn is_not_null(&self) -> Option<bool> {
        if self.types.single_type().is_some() {
            Some(true)
        } else {
            None
        }
    }
}
