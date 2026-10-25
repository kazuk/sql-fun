use super::SemScalarExprNode;
use crate::sem::{
    AnalysisError, InputTypeConstraint, ParameterConstraints, ParseContext, PgBuiltInType,
    ResolvedOperator, SelectStatement, SemScalarExpr, TypeReference,
};

/// SQL combination rule for a retained special comparison.
/// Distinguishes array and query quantifiers from single-row comparisons.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SpecialOperatorKind {
    /// Scalar comparison against any array element.
    ArrayAny,
    /// Scalar comparison against all array elements.
    ArrayAll,
    /// Inclusive lower and upper comparisons.
    Between,
    /// Negation of inclusive lower and upper comparisons.
    NotBetween,
    /// Equality test returning NULL or the coerced first input.
    NullIf,
    /// Comparison against any query row.
    QueryAny,
    /// Comparison against every query row.
    QueryAll,
    /// Single-row subquery comparison.
    QueryRow,
}
impl SpecialOperatorKind {
    /// Compare against any array element.
    #[must_use]
    pub const fn array_any() -> Self {
        Self::ArrayAny
    }
    /// Compare against every array element.
    #[must_use]
    pub const fn array_all() -> Self {
        Self::ArrayAll
    }
    /// Combine inclusive lower and upper comparisons.
    #[must_use]
    pub const fn between() -> Self {
        Self::Between
    }
    /// Negate inclusive lower and upper comparisons.
    #[must_use]
    pub const fn not_between() -> Self {
        Self::NotBetween
    }
    /// Return NULL or the coerced first comparison input.
    #[must_use]
    pub const fn null_if() -> Self {
        Self::NullIf
    }
    /// Compare against any subquery row.
    #[must_use]
    pub const fn query_any() -> Self {
        Self::QueryAny
    }
    /// Compare against every subquery row.
    #[must_use]
    pub const fn query_all() -> Self {
        Self::QueryAll
    }
    /// Compare against a single subquery row.
    #[must_use]
    pub const fn query_row() -> Self {
        Self::QueryRow
    }
    pub(crate) const fn is_array(self) -> bool {
        matches!(self, Self::ArrayAny | Self::ArrayAll)
    }
    const fn returns_left(self) -> bool {
        matches!(self, Self::NullIf)
    }
    const fn is_between(self) -> bool {
        matches!(self, Self::Between | Self::NotBetween)
    }
}

/// Resolved comparison operator and correlated coercion targets.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SpecialComparison {
    binding: ResolvedOperator,
    targets: [InputTypeConstraint; 2],
    #[serde(skip)]
    nodes: Option<[usize; 2]>,
}
impl SpecialComparison {
    fn new(binding: &ResolvedOperator) -> Self {
        Self {
            binding: binding.clone(),
            targets: [InputTypeConstraint::any(), InputTypeConstraint::any()],
            nodes: None,
        }
    }
    /// Completed operator-name resolution, independent of overload selection.
    #[must_use]
    pub const fn binding(&self) -> &ResolvedOperator {
        &self.binding
    }
    /// Selected operand type domains; these are coercion targets, not source types.
    #[must_use]
    pub const fn targets(&self) -> &[InputTypeConstraint; 2] {
        &self.targets
    }
}

/// A special operator retaining its actual operands, analyzed query, and type relationships.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SpecialOperatorExpr {
    kind: SpecialOperatorKind,
    operands: Vec<SemScalarExpr>,
    query: Option<Box<SelectStatement>>,
    comparisons: Vec<SpecialComparison>,
    types: InputTypeConstraint,
    invalid_shape: bool,
    #[serde(skip)]
    node: Option<usize>,
}
impl SpecialOperatorExpr {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        kind: SpecialOperatorKind,
        operands: &[SemScalarExpr],
        query: Option<&SelectStatement>,
        bindings: &[ResolvedOperator],
    ) -> Result<Self, AnalysisError> {
        let mut value = Self {
            kind,
            operands: operands.to_vec(),
            query: query.cloned().map(Box::new),
            comparisons: bindings.iter().map(SpecialComparison::new).collect(),
            types: if kind.returns_left() {
                InputTypeConstraint::any()
            } else {
                InputTypeConstraint::exact(&PgBuiltInType::bool())
            },
            invalid_shape: false,
            node: None,
        };
        if let Some(query) = &mut value.query {
            query.normalize_unknown_outputs(context)?;
        }
        value.validate_shape(context)?;
        value.resolve_known(context)?;
        Ok(value)
    }
    /// SQL combination rule, including array/query and row distinctions.
    #[must_use]
    pub const fn kind(&self) -> SpecialOperatorKind {
        self.kind
    }
    /// Outer analyzed operands; a query's result expressions remain in its query.
    #[must_use]
    pub fn operands(&self) -> &[SemScalarExpr] {
        &self.operands
    }
    /// Analyzed child query for quantified and single-row subquery comparisons.
    #[must_use]
    pub fn query(&self) -> Option<&SelectStatement> {
        self.query.as_deref()
    }
    /// Comparison bindings and their correlated coercion targets.
    #[must_use]
    pub fn comparisons(&self) -> &[SpecialComparison] {
        &self.comparisons
    }
    /// Admissible result types; an empty domain represents failure.
    #[must_use]
    pub const fn types(&self) -> &InputTypeConstraint {
        &self.types
    }
    pub(crate) fn comparison_binding(&self) -> Option<&ResolvedOperator> {
        self.comparisons.first().map(SpecialComparison::binding)
    }
    pub(crate) const fn node(&self) -> Option<usize> {
        self.node
    }

    fn query_values(&self) -> Vec<SemScalarExpr> {
        let Some(query) = &self.query else {
            return Vec::new();
        };
        query.result_columns().comparison_values()
    }
    fn validate_shape(&mut self, context: &mut impl ParseContext) -> Result<(), AnalysisError> {
        if let Some(query) = &self.query
            && !query.has_pending_stars()
            && !query.has_width(self.operands.len())
            && !self.invalid_shape
        {
            self.invalid_shape = true;
            self.types = InputTypeConstraint::types([]);
            context.report_analysis_problem(crate::sem::AnalysisProblem::comparison(
                crate::sem::problem::ComparisonProblem::columns(
                    self.operands.len(),
                    query.result_columns().len(),
                ),
            ))?;
        }
        Ok(())
    }
    pub(crate) fn expand_pending_stars(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        let changed = if let Some(query) = &mut self.query {
            query.expand_pending_stars(context)?
        } else {
            false
        };
        self.validate_shape(context)?;
        Ok(changed)
    }
    fn pairs(&self, query_values: &[SemScalarExpr]) -> Vec<(SemScalarExpr, SemScalarExpr)> {
        if self.query.is_some() {
            self.operands
                .iter()
                .cloned()
                .zip(query_values.iter().cloned())
                .collect()
        } else if self.kind.is_between() {
            vec![
                (self.operands[0].clone(), self.operands[1].clone()),
                (self.operands[0].clone(), self.operands[2].clone()),
            ]
        } else {
            vec![(self.operands[0].clone(), self.operands[1].clone())]
        }
    }
    fn resolve_known(&mut self, context: &mut impl ParseContext) -> Result<(), AnalysisError> {
        if self.invalid_shape
            || self
                .query
                .as_ref()
                .is_some_and(|query| query.has_pending_stars())
        {
            return Ok(());
        }
        let pairs = self.pairs(&self.query_values());
        for (comparison, (left, right)) in self.comparisons.iter_mut().zip(pairs) {
            let right_type = if self.kind.is_array() {
                let Some(array) = right.get_type() else {
                    continue;
                };
                let base = crate::sem::MinMaxExpr::base_type(context, &array);
                let Some(element) = context.get_array_item_type(&base) else {
                    right.require_array_resolved(context)?;
                    self.types = InputTypeConstraint::types([]);
                    continue;
                };
                element
            } else {
                let Some(ty) = crate::sem::MinMaxExpr::input_type(&right) else {
                    continue;
                };
                ty
            };
            let Some(left_type) = crate::sem::MinMaxExpr::input_type(&left) else {
                continue;
            };
            if let Some(targets) =
                comparison
                    .binding
                    .known_comparison(context, &left_type, &right_type)?
            {
                comparison.targets = targets.map(|ty| InputTypeConstraint::exact(&ty));
                if self.kind.returns_left() {
                    self.types = comparison.targets[0].clone();
                }
            } else {
                self.types = InputTypeConstraint::types([]);
            }
        }
        Ok(())
    }
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for operand in &mut self.operands {
            operand.walk(visitor)?;
        }
        if let Some(query) = &mut self.query {
            query.walk(visitor)?;
        }
        visitor.special_operator(self)?;
        Ok(())
    }
    pub(crate) fn collect_constraints(
        &mut self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        self.validate_shape(context)?;
        let query_values = if self.query.is_some() {
            parameters.completed_values()
        } else {
            Vec::new()
        };
        let node = parameters.result_node(self.node, &self.types);
        self.node = Some(node);
        if self.invalid_shape
            || self
                .query
                .as_ref()
                .is_some_and(|query| query.has_pending_stars())
        {
            return Ok(());
        }
        let pairs = self.pairs(&query_values);
        for (comparison, (left, right)) in self.comparisons.iter_mut().zip(pairs) {
            let right = if self.kind.is_array() {
                parameters.array_element(context, &right)
            } else {
                right
            };
            let (result, targets) = parameters.comparison(
                context,
                &comparison.binding,
                &left,
                &right,
                self.kind.returns_left(),
            );
            for (target, domain) in targets.into_iter().zip(&comparison.targets) {
                parameters.result_node(Some(target), domain);
            }
            comparison.nodes = Some(targets);
            parameters.equal_nodes(node, result);
        }
        Ok(())
    }
    pub(crate) fn apply_constraints(&mut self, constraints: &ParameterConstraints) {
        if let Some(node) = self.node {
            self.types = constraints.node_domain(node).clone();
        }
        for comparison in &mut self.comparisons {
            if let Some(nodes) = comparison.nodes {
                comparison.targets = nodes.map(|node| constraints.node_domain(node).clone());
            }
        }
    }
    pub(crate) fn canonicalize_grouping_key(&mut self) {
        self.node = None;
        for comparison in &mut self.comparisons {
            comparison.nodes = None;
            comparison.binding.canonicalize_grouping_key();
        }
    }
}
impl SemScalarExprNode for SpecialOperatorExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.types.single_type()
    }
    fn is_not_null(&self) -> Option<bool> {
        if self.kind.returns_left() {
            Some(false)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests;
