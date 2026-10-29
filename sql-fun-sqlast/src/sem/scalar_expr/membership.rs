use crate::ast::Dialect;
use crate::sem::parameter_resolution::ParameterResolution;
use crate::sem::scalar_expr::SemScalarExprNode;
use crate::sem::traversal::SemanticVisitor;
use crate::sem::{
    AnalysisError, AnalysisProblem, ArithExpr, InputTypeConstraint, MinMaxExpr, OperatorName,
    ParameterConstraints, ParseContext, PgBuiltInType, ResolvedOperator, SelectStatement,
    SemScalarExpr, TypeReference, problem::MembershipProblem,
};

/// An analyzed IN/NOT IN predicate. Operands retain their bindings independently
/// of the comparisons and common-type projections derived from them.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MembershipExpr {
    left: SemScalarExpr,
    right: MembershipOperands,
    negated: bool,
    binding: Option<ResolvedOperator>,
    comparisons: Vec<MembershipComparison>,
    types: InputTypeConstraint,
    invalid: bool,
    comparison_not_null: Option<bool>,
    #[serde(skip)]
    node: Option<usize>,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
enum MembershipOperands {
    List(Vec<SemScalarExpr>),
    Query(Box<SelectStatement>),
}
impl MembershipOperands {
    fn list(values: &[SemScalarExpr]) -> Self {
        Self::List(values.to_vec())
    }
    fn query(query: &SelectStatement) -> Self {
        Self::Query(Box::new(query.clone()))
    }
    fn values(&self) -> Vec<SemScalarExpr> {
        match self {
            Self::List(values) => values.clone(),
            Self::Query(query) => query.result_columns().membership_operands(),
        }
    }
    fn as_list(&self) -> Option<&[SemScalarExpr]> {
        match self {
            Self::List(values) => Some(values),
            _ => None,
        }
    }
    fn as_query(&self) -> Option<&SelectStatement> {
        match self {
            Self::Query(query) => Some(query),
            _ => None,
        }
    }
    fn pending(&self) -> bool {
        self.as_query()
            .is_some_and(SelectStatement::has_pending_stars)
    }
    fn walk<V: SemanticVisitor>(&mut self, visitor: &mut V) -> Result<(), AnalysisError> {
        match self {
            Self::List(values) => {
                for value in values {
                    value.walk(visitor)?;
                }
                Ok(())
            }
            Self::Query(query) => query.walk(visitor),
        }
    }
    fn expand(&mut self, context: &mut impl ParseContext) -> Result<bool, AnalysisError> {
        match self {
            Self::Query(query) => query.expand_pending_stars(context),
            Self::List(_) => Ok(false),
        }
    }
}

/// One scalar comparison, including its resolved operator and argument coercions.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MembershipComparison {
    left: SemScalarExpr,
    right: SemScalarExpr,
    expression: Option<SemScalarExpr>,
}
impl MembershipComparison {
    /// Left comparison operand, including retained comparison coercions.
    pub fn left(&self) -> &SemScalarExpr {
        if let Some(expression) = &self.expression
            && let Some(argument) = expression.binary_argument(0)
        {
            argument
        } else {
            &self.left
        }
    }
    /// Right comparison operand, including retained comparison coercions.
    pub fn right(&self) -> &SemScalarExpr {
        if let Some(expression) = &self.expression
            && let Some(argument) = expression.binary_argument(1)
        {
            argument
        } else {
            &self.right
        }
    }
    /// Bound PostgreSQL comparison, or a graph-derived SQLite comparison.
    pub const fn expression(&self) -> Option<&SemScalarExpr> {
        self.expression.as_ref()
    }

    fn resolve(
        context: &mut impl ParseContext,
        left: &SemScalarExpr,
        right: &SemScalarExpr,
        binding: Option<&ResolvedOperator>,
        name: &OperatorName,
        mut parameters: Option<&mut ParameterResolution>,
    ) -> Result<Self, AnalysisError> {
        let (mut left, mut right) = if binding.is_some() {
            SemScalarExpr::comparison_literal_operands(context, left.clone(), right.clone())
        } else {
            (left.clone(), right.clone())
        };
        let expression = if let Some(binding) = binding {
            if !binding.resolution().is_resolved() {
                Some(Self::bound(context, name, binding, &left, &right)?)
            } else if let Some(parameters) = parameters.as_mut() {
                // A literal paired with an unresolved parameter gets its SQL
                // target from the comparison, not the literal's display type.
                for operand in [&mut left, &mut right] {
                    if MinMaxExpr::input_type(operand) == Some(MinMaxExpr::unknown()) {
                        *operand = parameters.operation(
                            "membership literal",
                            std::slice::from_ref(operand),
                            &InputTypeConstraint::any(),
                            operand.is_not_null(),
                        );
                    }
                }
                if let Some(mut result) =
                    binding.infer(parameters, context, Some(&left), Some(&right))?
                {
                    result.retain_operator_binding(binding);
                    Some(result)
                } else {
                    Some(Self::bound(context, name, binding, &left, &right)?)
                }
            } else if left.requires_argument_type() || right.requires_argument_type() {
                None
            } else {
                Some(Self::bound(context, name, binding, &left, &right)?)
            }
        } else {
            parameters.map(|parameters| {
                parameters.operation(
                    "SQLite membership comparison",
                    &[left.clone(), right.clone()],
                    &InputTypeConstraint::exact(&PgBuiltInType::int4()),
                    if left.is_not_null() == Some(true) && right.is_not_null() == Some(true) {
                        Some(true)
                    } else {
                        None
                    },
                )
            })
        };
        Ok(Self {
            left,
            right,
            expression,
        })
    }
    fn bound(
        context: &mut impl ParseContext,
        name: &OperatorName,
        binding: &ResolvedOperator,
        left: &SemScalarExpr,
        right: &SemScalarExpr,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut value =
            ArithExpr::build_op(context, name, Some(left.clone()), Some(right.clone()))?.0;
        value.set_operator_binding(binding.clone());
        Ok(value)
    }
    fn not_null(&self) -> Option<bool> {
        if let Some(expression) = &self.expression {
            expression.is_not_null()
        } else if self.left.is_not_null() == Some(true) && self.right.is_not_null() == Some(true) {
            Some(true)
        } else {
            None
        }
    }
    fn rename_relation(&mut self, old: &crate::sem::TableName, new: &crate::sem::TableName) {
        let mut visitor = crate::sem::traversal::RelationRenamer::new(old, new);
        self.left.walk(&mut visitor).expect("relation rename");
        self.right.walk(&mut visitor).expect("relation rename");
        if let Some(expression) = &mut self.expression {
            expression.walk(&mut visitor).expect("relation rename");
        }
    }
    fn apply_constraints(&mut self, constraints: &ParameterConstraints) {
        let mut visitor = ApplyConstraints { constraints };
        // This visitor has no fallible operations.
        self.left
            .walk(&mut visitor)
            .expect("constraint application");
        self.right
            .walk(&mut visitor)
            .expect("constraint application");
        if let Some(expression) = &mut self.expression {
            expression
                .walk(&mut visitor)
                .expect("constraint application");
        }
    }
}

struct ApplyConstraints<'a> {
    constraints: &'a ParameterConstraints,
}
impl SemanticVisitor for ApplyConstraints<'_> {
    fn expression(&mut self, value: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        value.apply_constraints(self.constraints);
        Ok(())
    }
}

impl MembershipExpr {
    fn new(
        context: &impl ParseContext,
        left: &SemScalarExpr,
        right: MembershipOperands,
        negated: bool,
    ) -> Self {
        let name = OperatorName::from_parts(None, if negated { "<>" } else { "=" });
        let binding = if context.dialect() == Dialect::postgres() {
            Some(ResolvedOperator::resolve_comparison(context, &name))
        } else {
            None
        };
        let result = if context.dialect() == Dialect::sqlite() {
            PgBuiltInType::int4()
        } else {
            PgBuiltInType::bool()
        };
        let comparison_not_null = if binding.is_none() {
            Some(true)
        } else {
            context.operator_result_is_not_null(&name)
        };
        Self {
            left: left.clone(),
            right,
            negated,
            binding,
            comparisons: Vec::new(),
            types: InputTypeConstraint::exact(&result),
            invalid: false,
            comparison_not_null,
            node: None,
        }
    }
    pub(crate) fn list(
        context: &mut impl ParseContext,
        left: &SemScalarExpr,
        values: &[SemScalarExpr],
        negated: bool,
    ) -> Result<Self, AnalysisError> {
        let mut value = Self::new(context, left, MembershipOperands::list(values), negated);
        value.analyze(context, None)?;
        Ok(value)
    }
    pub(crate) fn query(
        context: &mut impl ParseContext,
        left: &SemScalarExpr,
        query: &SelectStatement,
    ) -> Result<Self, AnalysisError> {
        let mut value = Self::new(context, left, MembershipOperands::query(query), false);
        value.right.walk(&mut UnknownOutput { context, depth: 0 })?;
        value.analyze(context, None)?;
        Ok(value)
    }
    pub(super) fn sqlite_negation(&self) -> Option<Self> {
        if self.binding.is_some() {
            return None;
        }
        let mut result = self.clone();
        result.negated = !result.negated;
        Some(result)
    }
    /// Analyzed left operand, including all row fields.
    pub const fn left(&self) -> &SemScalarExpr {
        &self.left
    }
    /// Analyzed list operands, when the RHS is a list.
    pub fn values(&self) -> Option<&[SemScalarExpr]> {
        self.right.as_list()
    }
    /// Analyzed child query, when the RHS is a subquery.
    pub fn subquery(&self) -> Option<&SelectStatement> {
        self.right.as_query()
    }
    /// Whether this node represents NOT IN. PostgreSQL query NOT IN uses a NOT wrapper.
    pub const fn negated(&self) -> bool {
        self.negated
    }
    /// Completed operator-name lookup, absent for SQLite's builtin comparison.
    pub const fn binding(&self) -> Option<&ResolvedOperator> {
        self.binding.as_ref()
    }
    /// Scalar comparisons, ordered by RHS operand and row-field position.
    pub fn comparisons(&self) -> &[MembershipComparison] {
        &self.comparisons
    }
    /// Predicate result domain, empty for an invalid membership operation.
    pub const fn types(&self) -> &InputTypeConstraint {
        &self.types
    }

    fn fail(
        &mut self,
        context: &mut impl ParseContext,
        problem: MembershipProblem,
    ) -> Result<(), AnalysisError> {
        self.invalid = true;
        self.types = InputTypeConstraint::types([]);
        context.report_analysis_problem(AnalysisProblem::membership(problem))
    }
    fn analyze(
        &mut self,
        context: &mut impl ParseContext,
        parameters: Option<&mut ParameterResolution>,
    ) -> Result<(), AnalysisError> {
        if self.invalid || self.right.pending() {
            return Ok(());
        }
        self.comparisons.clear();
        let values = self.right.values();
        if self.right.as_query().is_some() {
            return self.compare_query_outputs(context, &values, parameters);
        }
        self.analyze_list(context, &values, parameters)
    }

    fn compare_query_outputs(
        &mut self,
        context: &mut impl ParseContext,
        values: &[SemScalarExpr],
        mut parameters: Option<&mut ParameterResolution>,
    ) -> Result<(), AnalysisError> {
        let left_fields = self.left.membership_fields().to_vec();
        if left_fields.len() != values.len() {
            return self.fail(
                context,
                MembershipProblem::width(left_fields.len(), values.len()),
            );
        }
        for (left, right) in left_fields.iter().zip(values) {
            self.compare_scalar(context, left, right, parameters.as_deref_mut())?;
        }
        Ok(())
    }

    fn analyze_list(
        &mut self,
        context: &mut impl ParseContext,
        values: &[SemScalarExpr],
        mut parameters: Option<&mut ParameterResolution>,
    ) -> Result<(), AnalysisError> {
        // PostgreSQL groups multiple non-Var scalar RHS values under one
        // common-type selection, including the LHS as its first type input.
        let common_indices =
            if context.dialect() == Dialect::postgres() && !self.left.is_row_constructor() {
                values
                    .iter()
                    .enumerate()
                    .filter_map(|(index, value)| {
                        if !contains_variable(value) {
                            Some(index)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
        let mut target = None;
        if common_indices.len() > 1 {
            let mut inputs = vec![self.left.clone()];
            for index in &common_indices {
                inputs.push(values[*index].clone());
            }
            let types: Option<Vec<_>> = inputs.iter().map(MinMaxExpr::input_type).collect();
            if let Some(types) = types {
                match MinMaxExpr::select_common_type(context, &types) {
                    Ok(ty) if !ty.is_record() => target = Some(ty),
                    Ok(_) => {}
                    Err(problem) => {
                        return self.fail(context, MembershipProblem::common_type(problem));
                    }
                }
            } else if let Some(parameters) = parameters.as_deref_mut() {
                let common = parameters.membership_common(context, &inputs)?;
                // The type projection retains its source operands and graph
                // relationships; it is not a synthetic SQL value or binding.
                let left = self.left.clone();
                self.compare(context, &left, &common, Some(parameters))?;
                for (index, right) in values.iter().enumerate() {
                    if !common_indices.contains(&index) {
                        self.compare(context, &left, right, Some(&mut *parameters))?;
                    }
                }
                return Ok(());
            }
        }
        let left = self.left.clone();
        for (index, value) in values.iter().enumerate() {
            let right = if common_indices.contains(&index)
                && let Some(target) = &target
            {
                SemScalarExpr::wrap_implicit_cast(target, value)
            } else {
                value.clone()
            };
            self.compare(context, &left, &right, parameters.as_deref_mut())?;
        }
        Ok(())
    }
    fn compare(
        &mut self,
        context: &mut impl ParseContext,
        left: &SemScalarExpr,
        right: &SemScalarExpr,
        parameters: Option<&mut ParameterResolution>,
    ) -> Result<(), AnalysisError> {
        // Row comparisons use semantic field operations, retaining the
        // original rows on the membership node.
        if left.is_membership_row() && right.is_membership_row() {
            let left_fields = left.membership_fields();
            let right_fields = right.membership_fields();
            if left_fields.len() != right_fields.len() {
                return self.fail(
                    context,
                    MembershipProblem::width(left_fields.len(), right_fields.len()),
                );
            }
            let mut parameters = parameters;
            for (left, right) in left_fields.iter().zip(right_fields) {
                // A nested row is a composite field here. Its comparison uses
                // record equality rather than another SQL row-constructor test.
                self.compare_scalar(context, left, right, parameters.as_deref_mut())?;
            }
            return Ok(());
        }
        self.compare_scalar(context, left, right, parameters)
    }
    fn compare_scalar(
        &mut self,
        context: &mut impl ParseContext,
        left: &SemScalarExpr,
        right: &SemScalarExpr,
        parameters: Option<&mut ParameterResolution>,
    ) -> Result<(), AnalysisError> {
        let name = OperatorName::from_parts(None, if self.negated { "<>" } else { "=" });
        let comparison = MembershipComparison::resolve(
            context,
            left,
            right,
            self.binding.as_ref(),
            &name,
            parameters,
        )?;
        if self.binding.is_some()
            && let Some(expression) = &comparison.expression
            && let Some(ty) = expression.get_type()
            && ty != PgBuiltInType::bool()
        {
            self.fail(context, MembershipProblem::comparison_result(&ty))?;
        }
        if self
            .binding
            .as_ref()
            .is_some_and(|binding| !binding.resolution().is_resolved())
            || (self.binding.is_some()
                && comparison
                    .expression
                    .as_ref()
                    .is_some_and(|value| value.get_type().is_none())
                && !left.requires_argument_type()
                && !right.requires_argument_type())
        {
            self.invalid = true;
            self.types = InputTypeConstraint::types([]);
        }
        self.comparisons.push(comparison);
        Ok(())
    }
    pub(crate) fn walk<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.left.walk(visitor)?;
        self.right.walk(visitor)?;
        visitor.membership(self)
    }
    pub(crate) fn expand_pending_stars(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        self.right.expand(context)
    }
    pub(crate) fn collect_constraints(
        &mut self,
        parameters: &mut ParameterResolution,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        self.analyze(context, Some(parameters))?;
        let expressions: Vec<_> = self
            .comparisons
            .iter()
            .filter_map(|value| value.expression.clone())
            .collect();
        let result = parameters.membership_result(
            &expressions,
            self.binding.is_some(),
            &self.types,
            self.node,
        );
        self.node = Some(result);
        Ok(())
    }
    pub(crate) fn rename_relation(
        &mut self,
        old: &crate::sem::TableName,
        new: &crate::sem::TableName,
    ) {
        for comparison in &mut self.comparisons {
            comparison.rename_relation(old, new);
        }
    }
    pub(crate) const fn node(&self) -> Option<usize> {
        self.node
    }
    pub(crate) fn apply_constraints(&mut self, constraints: &ParameterConstraints) {
        if let Some(node) = self.node {
            self.types = constraints.node_domain(node).clone();
        }
        for comparison in &mut self.comparisons {
            comparison.apply_constraints(constraints);
        }
    }
    pub(super) fn canonicalize_grouping_key(&mut self) {
        self.node = None;
        self.comparisons.clear();
        if let Some(binding) = &mut self.binding {
            binding.canonicalize_grouping_key();
        }
    }
}
impl SemScalarExprNode for MembershipExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.types.single_type()
    }
    fn is_not_null(&self) -> Option<bool> {
        if self.types.single_type().is_none() || self.invalid || self.right.pending() {
            return None;
        }
        if self
            .comparisons
            .iter()
            .all(|value| value.not_null() == Some(true))
        {
            if self.comparisons.is_empty() {
                Some(true)
            } else {
                self.comparison_not_null
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests;

// PostgreSQL's common-list grouping distinguishes references to the current
// query from references local to nested queries. Reuse completed source aliases;
// this inspection does not perform name lookup again.
fn contains_variable(value: &SemScalarExpr) -> bool {
    #[derive(Default)]
    struct Variables {
        scopes: Vec<std::collections::HashSet<String>>,
        found: bool,
    }
    impl Variables {
        fn relation(&mut self, alias: &str) {
            if let Some(scope) = self.scopes.last_mut() {
                scope.insert(alias.to_owned());
            }
        }
    }
    impl SemanticVisitor for Variables {
        fn enter_query(&mut self) {
            self.scopes.push(Default::default());
        }
        fn leave_query(&mut self) {
            self.scopes.pop();
        }
        fn relation(&mut self, _name: Option<&str>, alias: &str, _columns: &[String]) {
            self.relation(alias);
        }
        fn derived(&mut self, alias: &str, _columns: &[String]) {
            self.relation(alias);
        }
        fn expression(&mut self, value: &mut SemScalarExpr) -> Result<(), AnalysisError> {
            if let Some(alias) = value.membership_variable_alias() {
                self.found |= !self.scopes.iter().any(|scope| scope.contains(alias));
            }
            Ok(())
        }
    }
    let mut visitor = Variables::default();
    value
        .clone()
        .walk(&mut visitor)
        .expect("variable inspection");
    visitor.found
}

impl SemScalarExpr {
    fn membership_variable_alias(&self) -> Option<&str> {
        match self {
            Self::WholeRow(value) => value.alias().map(crate::sem::AliasName::as_str),
            _ => self.constraint_binding().map(|(alias, _)| alias),
        }
    }
}

struct UnknownOutput<'a, C: ParseContext> {
    context: &'a C,
    depth: usize,
}
impl<C: ParseContext> SemanticVisitor for UnknownOutput<'_, C> {
    fn enter_query(&mut self) {
        self.depth += 1;
    }
    fn leave_query(&mut self) {
        self.depth -= 1;
    }
    fn output(&mut self, _name: &str, value: &mut SemScalarExpr) {
        if self.context.dialect() == Dialect::postgres()
            && self.depth == 1
            && MinMaxExpr::input_type(value) == Some(MinMaxExpr::unknown())
        {
            *value = SemScalarExpr::wrap_implicit_cast(&PgBuiltInType::text(), value);
        }
    }
}
