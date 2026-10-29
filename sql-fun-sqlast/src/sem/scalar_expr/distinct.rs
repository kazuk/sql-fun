use crate::ast::Dialect;
use crate::sem::scalar_expr::SemScalarExprNode;
use crate::sem::{
    AnalysisError, AnalysisProblem, ArithExpr, InputTypeConstraint, MinMaxExpr, OperatorName,
    ParameterConstraints, ParseContext, PgBuiltInType, ResolvedOperator, SemScalarExpr,
    TypeReference, problem::DistinctProblem,
};

/// A null-safe comparison, distinct from the equality operation it uses.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DistinctExpr {
    comparison: DistinctComparison,
    negated: bool,
    types: InputTypeConstraint,
    not_null: Option<bool>,
    #[serde(skip)]
    node: Option<usize>,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
enum DistinctComparison {
    Equality(Box<SemScalarExpr>),
    Deferred(Box<DeferredEquality>),
    // Bare NULL and SQLite comparisons do not require catalog equality.
    Operands(Vec<SemScalarExpr>),
    Rows(Vec<SemScalarExpr>),
    Invalid(Vec<SemScalarExpr>),
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
struct DeferredEquality {
    operands: [SemScalarExpr; 2],
    binding: ResolvedOperator,
}

impl DeferredEquality {
    fn infer(
        &self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
        context: &mut impl ParseContext,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut operands = self.operands.clone();
        for operand in &mut operands {
            if MinMaxExpr::input_type(operand) == Some(MinMaxExpr::unknown()) {
                *operand = parameters.operation(
                    "unknown comparison literal",
                    std::slice::from_ref(operand),
                    &InputTypeConstraint::any(),
                    operand.is_not_null(),
                );
            }
        }
        let mut equality = self
            .binding
            .infer(parameters, context, Some(&operands[0]), Some(&operands[1]))?
            .expect("deferred comparison has constraint inputs");
        equality.retain_operator_binding(&self.binding);
        Ok(equality)
    }
}

impl DistinctComparison {
    fn equality(value: SemScalarExpr) -> Self {
        Self::Equality(Box::new(value))
    }
    fn operands(left: SemScalarExpr, right: SemScalarExpr) -> Self {
        Self::Operands(vec![left, right])
    }
    fn invalid(left: SemScalarExpr, right: SemScalarExpr) -> Self {
        Self::Invalid(vec![left, right])
    }
    const fn rows(values: Vec<SemScalarExpr>) -> Self {
        Self::Rows(values)
    }
    fn deferred(left: SemScalarExpr, right: SemScalarExpr, binding: ResolvedOperator) -> Self {
        Self::Deferred(Box::new(DeferredEquality {
            operands: [left, right],
            binding,
        }))
    }
    fn operand(&self, index: usize) -> Option<&SemScalarExpr> {
        match self {
            Self::Equality(value) => value.binary_argument(index),
            Self::Deferred(value) => value.operands.get(index),
            Self::Operands(values) | Self::Invalid(values) => values.get(index),
            Self::Rows(_) => None,
        }
    }
    fn as_equality(&self) -> Option<&SemScalarExpr> {
        if let Self::Equality(value) = self {
            Some(value)
        } else {
            None
        }
    }
    fn infer(
        &mut self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if let Self::Deferred(value) = self {
            *self = Self::equality(value.infer(parameters, context)?);
        }
        Ok(())
    }
    fn row_nullability(&self) -> Option<Option<bool>> {
        if let Self::Rows(values) = self {
            Some(
                if values.iter().all(|value| value.is_not_null() == Some(true)) {
                    Some(true)
                } else {
                    None
                },
            )
        } else {
            None
        }
    }
    fn has_impossible_field(&self) -> bool {
        if let Self::Rows(values) = self {
            values.iter().any(|value| {
                value
                    .as_distinct()
                    .is_some_and(|value| value.types.admissible_types() == Some(&[]))
            })
        } else {
            false
        }
    }

    fn expressions(&self) -> &[SemScalarExpr] {
        match self {
            Self::Equality(value) => std::slice::from_ref(value),
            Self::Deferred(value) => &value.operands,
            Self::Operands(values) | Self::Rows(values) | Self::Invalid(values) => values,
        }
    }

    fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Equality(value) => value.walk(visitor),
            Self::Deferred(value) => {
                for operand in &mut value.operands {
                    operand.walk(visitor)?;
                }
                Ok(())
            }
            Self::Operands(values) | Self::Rows(values) | Self::Invalid(values) => {
                for value in values {
                    value.walk(visitor)?;
                }
                Ok(())
            }
        }
    }

    const fn requires_boolean(&self) -> bool {
        matches!(self, Self::Equality(_) | Self::Deferred(_) | Self::Rows(_))
    }
}

impl DistinctExpr {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        left: SemScalarExpr,
        right: SemScalarExpr,
        negated: bool,
    ) -> Result<Self, AnalysisError> {
        let mut result = Self {
            comparison: DistinctComparison::rows(Vec::new()),
            negated,
            types: InputTypeConstraint::exact(&PgBuiltInType::bool()),
            node: None,
            not_null: Some(true),
        };
        result.comparison = if context.dialect() == Dialect::sqlite()
            || left.as_const().is_some_and(super::ScalarConstExpr::is_null)
            || right
                .as_const()
                .is_some_and(super::ScalarConstExpr::is_null)
        {
            DistinctComparison::operands(left, right)
        } else if left.is_row_constructor() && right.is_row_constructor() {
            if let Some(comparisons) = left.distinct_row_comparisons(context, &right)? {
                DistinctComparison::rows(comparisons)
            } else {
                result.types = InputTypeConstraint::types([]);
                DistinctComparison::invalid(left, right)
            }
        } else {
            result.resolve_equality(context, left, right)?
        };
        if let Some(not_null) = result.comparison.row_nullability() {
            result.not_null = not_null;
        }
        if result.comparison.has_impossible_field() {
            result.types = InputTypeConstraint::types([]);
        }
        Ok(result)
    }

    fn resolve_equality(
        &mut self,
        context: &mut impl ParseContext,
        left: SemScalarExpr,
        right: SemScalarExpr,
    ) -> Result<DistinctComparison, AnalysisError> {
        let name = OperatorName::from_parts(None, "=");
        self.not_null = context.operator_result_is_not_null(&name);
        let binding = ResolvedOperator::resolve_comparison(context, &name);
        if !binding.resolution().is_resolved() {
            self.types = InputTypeConstraint::types([]);
        }
        let has_unknown = [&left, &right]
            .into_iter()
            .any(|value| MinMaxExpr::input_type(value) == Some(MinMaxExpr::unknown()));
        let deferred = left.requires_argument_type() || right.requires_argument_type();
        if has_unknown && deferred {
            return Ok(DistinctComparison::deferred(left, right, binding));
        }

        let (left, right) = SemScalarExpr::comparison_literal_operands(context, left, right);
        let mut equality = ArithExpr::build_op(context, &name, Some(left), Some(right))?.0;
        equality.set_operator_binding(binding);
        if let Some(ty) = equality.get_type() {
            if ty != PgBuiltInType::bool() {
                context.report_analysis_problem(AnalysisProblem::distinct(
                    DistinctProblem::equality_result(&ty),
                ))?;
                self.types = InputTypeConstraint::types([]);
            }
        } else if !deferred {
            self.types = InputTypeConstraint::types([]);
        }
        Ok(DistinctComparison::equality(equality))
    }

    /// Scalar left operand, including equality coercions after analysis.
    /// Explicit row pairs expose ordered field predicates through `comparisons`.
    #[must_use]
    pub fn left(&self) -> Option<&SemScalarExpr> {
        self.comparison.operand(0)
    }
    /// Scalar right operand, including equality coercions after analysis.
    /// Explicit row pairs expose ordered field predicates through `comparisons`.
    #[must_use]
    pub fn right(&self) -> Option<&SemScalarExpr> {
        self.comparison.operand(1)
    }

    /// Underlying PostgreSQL equality, including its binding and coercions.
    /// Bare NULL, explicit row pairs, and SQLite have no scalar equality call.
    #[must_use]
    pub fn equality(&self) -> Option<&SemScalarExpr> {
        self.comparison.as_equality()
    }

    /// Retained comparison expressions: one equality, two direct operands,
    /// or ordered field predicates for a pair of explicit rows.
    #[must_use]
    pub fn comparisons(&self) -> &[SemScalarExpr] {
        self.comparison.expressions()
    }

    /// Whether this is IS NOT DISTINCT FROM (SQLite IS).
    #[must_use]
    pub const fn negated(&self) -> bool {
        self.negated
    }

    /// Admissible boolean result domain; empty when inputs are impossible.
    #[must_use]
    pub const fn types(&self) -> &InputTypeConstraint {
        &self.types
    }

    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.comparison.walk(visitor)
    }

    pub(crate) fn collect_constraints(
        &mut self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        self.comparison.infer(parameters, context)?;
        self.node = Some(parameters.distinct_result(
            self.comparison.expressions(),
            self.comparison.requires_boolean(),
            &self.types,
        ));
        Ok(())
    }

    pub(crate) const fn node(&self) -> Option<usize> {
        self.node
    }

    pub(crate) fn apply_constraints(&mut self, constraints: &ParameterConstraints) {
        if let Some(node) = self.node {
            self.types = constraints.node_domain(node).clone();
        }
    }

    pub(super) const fn canonicalize_grouping_key(&mut self) {
        self.node = None;
    }
}

impl SemScalarExprNode for DistinctExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.types.single_type()
    }
    fn is_not_null(&self) -> Option<bool> {
        if self.types.single_type().is_some() {
            self.not_null
        } else {
            None
        }
    }
}

impl super::RowExpr {
    pub(super) fn distinct_comparisons(
        &self,
        context: &mut impl ParseContext,
        other: &Self,
    ) -> Result<Option<Vec<SemScalarExpr>>, AnalysisError> {
        if self.0.len() != other.0.len() {
            context.report_analysis_problem(AnalysisProblem::distinct(
                crate::sem::problem::DistinctProblem::row_arity(self.0.len(), other.0.len()),
            ))?;
            return Ok(None);
        }
        let mut comparisons = Vec::new();
        for (left, right) in self.0.iter().zip(&other.0) {
            comparisons.push(SemScalarExpr::distinct(DistinctExpr::resolve(
                context,
                left.clone(),
                right.clone(),
                false,
            )?));
        }
        Ok(Some(comparisons))
    }
}

#[cfg(test)]
mod tests;
