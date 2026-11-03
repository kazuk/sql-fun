use crate::sem::{
    AnalysisError, AnalysisProblem, InputTypeConstraint, MinMaxExpr, SemScalarExpr, TypeReference,
    scalar_expr::SemScalarExprNode,
};

use std::collections::HashSet;

/// Builder for fully typed SQLite CASE; PostgreSQL uses ordered common types.
pub(crate) struct SqliteCaseBuilder {
    case_arg: Option<SemScalarExpr>,
    nullabilities: Vec<Option<bool>>,
    when_exprs: Vec<CaseWhen>,
    result_types: HashSet<TypeReference>,
    default_expr: Option<SemScalarExpr>,
}

impl SqliteCaseBuilder {
    pub(crate) fn new() -> Self {
        Self {
            case_arg: None,
            nullabilities: Default::default(),
            when_exprs: Default::default(),
            result_types: Default::default(),
            default_expr: None,
        }
    }

    fn collect_result(&mut self, result: &SemScalarExpr) {
        if let Some(result_type) = result.get_type() {
            self.result_types.insert(result_type);
        }
        self.nullabilities.push(result.is_not_null());
    }
}

impl SqliteCaseBuilder {
    fn is_not_null(&self) -> Option<bool> {
        if self.nullabilities.iter().any(|n| n == &Some(false)) {
            Some(false)
        } else if self.nullabilities.iter().all(std::option::Option::is_some) {
            Some(true)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test_sqlite_case_builder_is_not_null {

    #[test]
    fn test_is_not_null_any_false() {
        let mut acontext = super::SqliteCaseBuilder::new();

        acontext.nullabilities = vec![Some(true), Some(false)];
        assert_eq!(Some(false), acontext.is_not_null());
    }

    #[test]
    fn test_is_not_null_all_some() {
        let mut acontext = super::SqliteCaseBuilder::new();

        acontext.nullabilities = vec![Some(true), Some(true)];
        assert_eq!(Some(true), acontext.is_not_null());
    }

    #[test]
    fn test_is_not_null_with_unknown() {
        let mut acontext = super::SqliteCaseBuilder::new();

        acontext.nullabilities = vec![Some(true), None];
        assert_eq!(None, acontext.is_not_null());
    }
}

/// An ordered CASE arm with its condition, result, and simple-CASE comparison.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CaseWhen {
    when_value: SemScalarExpr,
    result: SemScalarExpr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    comparison: Option<SemScalarExpr>,
}

impl CaseWhen {
    /// Construct an arm before simple-CASE equality binding and type inference.
    pub const fn new(when_value: SemScalarExpr, result: SemScalarExpr) -> Self {
        Self {
            when_value,
            result,
            comparison: None,
        }
    }
}

/// case expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CaseExpr {
    arg_expr: Box<Option<SemScalarExpr>>,
    when_exprs: Vec<CaseWhen>,
    default_expr: Box<Option<SemScalarExpr>>,
    case_expr_type: Option<TypeReference>,
    is_not_null: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    types: Option<InputTypeConstraint>,
    #[serde(skip)]
    node: Option<usize>,
    #[serde(skip)]
    condition_nodes: Vec<usize>,
}

impl CaseExpr {
    /// create a instance
    #[must_use]
    pub const fn new(
        arg_expr: Box<Option<SemScalarExpr>>,
        when_exprs: Vec<CaseWhen>,
        default_expr: Box<Option<SemScalarExpr>>,
        case_expr_type: Option<TypeReference>,
        is_not_null: Option<bool>,
    ) -> Self {
        Self {
            arg_expr,
            when_exprs,
            default_expr,
            case_expr_type,
            is_not_null,
            types: None,
            node: None,
            condition_nodes: Vec::new(),
        }
    }
}

impl SemScalarExprNode for CaseExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.case_expr_type.clone()
    }

    fn is_not_null(&self) -> Option<bool> {
        self.is_not_null
    }
}

impl SqliteCaseBuilder {
    pub(crate) fn set_argument(&mut self, argument: &Option<SemScalarExpr>) {
        self.case_arg = argument.clone();
    }
    pub(crate) fn push_arm(&mut self, condition: &SemScalarExpr, result: &SemScalarExpr) {
        self.collect_result(result);
        self.when_exprs
            .push(CaseWhen::new(condition.clone(), result.clone()));
    }
    pub(crate) fn set_default(&mut self, value: &Option<SemScalarExpr>) {
        if let Some(value) = value {
            self.collect_result(value);
        }
        self.default_expr = value.clone();
    }
    pub(crate) fn finish(self) -> SemScalarExpr {
        let result_type = if self.result_types.len() == 1 {
            self.result_types.iter().next().cloned()
        } else {
            None
        };
        let nullable = self.is_not_null();
        let value = CaseExpr::new(
            Box::new(self.case_arg),
            self.when_exprs,
            Box::new(self.default_expr),
            result_type,
            nullable,
        );
        SemScalarExpr::case(&value)
    }
}

impl CaseExpr {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        crate::sem::traversal::optional(&mut self.arg_expr, visitor)?;
        for arm in &mut self.when_exprs {
            arm.walk(visitor)?;
        }
        crate::sem::traversal::optional(&mut self.default_expr, visitor)?;
        Ok(())
    }
    pub(crate) fn visit<V: crate::sem::traversal::SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        crate::sem::traversal::visit_optional(&self.arg_expr, visitor)?;
        for arm in &self.when_exprs {
            arm.visit(visitor)?;
        }
        crate::sem::traversal::visit_optional(&self.default_expr, visitor)?;
        Ok(())
    }
}

impl CaseWhen {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.when_value.walk(visitor)?;
        self.result.walk(visitor)?;
        if let Some(comparison) = &mut self.comparison {
            comparison.walk(visitor)?;
        }
        Ok(())
    }
    pub(crate) fn visit<V: crate::sem::traversal::SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.when_value.visit(visitor)?;
        self.result.visit(visitor)?;
        if let Some(comparison) = &self.comparison {
            comparison.visit(visitor)?;
        }
        Ok(())
    }
}

impl CaseExpr {
    pub(crate) fn collect_constraints(
        &mut self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution<'_>,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<(), AnalysisError> {
        if self.types.is_none()
            && self
                .arg_expr
                .as_ref()
                .as_ref()
                .is_some_and(|value| value.constraint_node().is_none())
            && self
                .default_expr
                .as_ref()
                .as_ref()
                .is_some_and(|value| value.constraint_node().is_none())
            && !self.when_exprs.iter().any(CaseWhen::has_constraints)
        {
            return Ok(());
        }
        // SQLite's heterogeneous CASE results do not impose PostgreSQL cast or
        // boolean requirements. Preserve its existing unresolved result metadata.
        if context.dialect() == crate::ast::Dialect::sqlite() {
            return Ok(());
        }
        if self
            .types
            .as_ref()
            .is_some_and(|types| types.admissible_types() == Some(&[]))
        {
            return Ok(());
        }
        for arm in &self.when_exprs {
            let condition = arm.comparison.as_ref().unwrap_or(&arm.when_value);
            self.condition_nodes
                .push(parameters.case_condition(context, condition));
        }
        let results = self.ordered_results();
        let (node, types) = parameters.unify_case_types(context, &results)?;
        self.node = Some(node);
        self.case_expr_type = types.single_type();
        self.types = Some(types);
        Ok(())
    }

    pub(crate) const fn node(&self) -> Option<usize> {
        self.node
    }

    pub(crate) fn apply_constraints(&mut self, constraints: &crate::sem::ParameterConstraints) {
        let Some(node) = self.node else {
            return;
        };
        let mut types = constraints.node_domain(node).clone();
        if self
            .condition_nodes
            .iter()
            .any(|node| constraints.node_domain(*node).admissible_types() == Some(&[]))
        {
            types = InputTypeConstraint::types([]);
        }
        self.case_expr_type = types.single_type();
        self.types = Some(types);
        if let Some(target) = &self.case_expr_type {
            let target = target.clone();
            self.coerce_results(&target);
        }
        for (arm, node) in self.when_exprs.iter_mut().zip(&self.condition_nodes) {
            if arm.comparison.is_none()
                && constraints.node_domain(*node).admissible_types() != Some(&[])
            {
                Self::coerce(&mut arm.when_value, &crate::sem::PgBuiltInType::bool());
            }
        }
        self.is_not_null = self.result_nullability();
    }

    // PostgreSQL considers ELSE first; omission supplies unknown NULL.
    fn ordered_results(&self) -> Vec<SemScalarExpr> {
        let mut results = vec![
            self.default_expr
                .as_ref()
                .clone()
                .unwrap_or_else(crate::sem::ScalarConstExpr::null),
        ];
        for arm in &self.when_exprs {
            results.push(arm.result.clone());
        }
        results
    }

    fn coerce_results(&mut self, target: &TypeReference) {
        for arm in &mut self.when_exprs {
            Self::coerce(&mut arm.result, target);
        }
        if let Some(otherwise) = self.default_expr.as_mut() {
            Self::coerce(otherwise, target);
        }
    }

    fn coerce(value: &mut SemScalarExpr, target: &TypeReference) {
        if value.get_type().as_ref() != Some(target)
            || MinMaxExpr::input_type(value) == Some(MinMaxExpr::unknown())
        {
            *value = SemScalarExpr::wrap_implicit_cast(target, value);
        }
    }

    fn result_nullability(&self) -> Option<bool> {
        let Some(otherwise) = self.default_expr.as_ref() else {
            return Some(false);
        };
        if otherwise.is_not_null() == Some(false)
            || self
                .when_exprs
                .iter()
                .any(|arm| arm.result.is_not_null() == Some(false))
        {
            Some(false)
        } else if otherwise.is_not_null() == Some(true)
            && self
                .when_exprs
                .iter()
                .all(|arm| arm.result.is_not_null() == Some(true))
        {
            Some(true)
        } else {
            None
        }
    }

    pub(super) fn canonicalize_grouping_key(&mut self) {
        self.node = None;
        self.condition_nodes.clear();
    }

    /// Optional selector of a simple CASE; searched CASE has no selector.
    pub fn argument(&self) -> Option<&SemScalarExpr> {
        self.arg_expr.as_ref().as_ref()
    }

    /// Ordered WHEN conditions and THEN results, retaining inferred coercions.
    pub fn arms(&self) -> &[CaseWhen] {
        &self.when_exprs
    }

    /// Explicit ELSE expression. Absence retains SQL's implicit NULL result.
    pub fn otherwise(&self) -> Option<&SemScalarExpr> {
        self.default_expr.as_ref().as_ref()
    }
}

impl CaseWhen {
    const fn has_constraints(&self) -> bool {
        self.when_value.constraint_node().is_some() || self.result.constraint_node().is_some()
    }

    /// Analyzed WHEN operand (or predicate in a searched CASE).
    pub const fn condition(&self) -> &SemScalarExpr {
        &self.when_value
    }

    /// Analyzed THEN result, including its implicit coercion when determined.
    pub const fn result(&self) -> &SemScalarExpr {
        &self.result
    }

    /// Simple-CASE equality expression and its operator binding, if present.
    pub const fn comparison(&self) -> Option<&SemScalarExpr> {
        self.comparison.as_ref()
    }
}

impl CaseExpr {
    fn bind_comparisons(
        &mut self,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<(), AnalysisError> {
        if context.dialect() != crate::ast::Dialect::postgres() {
            return Ok(());
        }
        let Some(argument) = self.arg_expr.as_ref() else {
            return Ok(());
        };
        for arm in &mut self.when_exprs {
            let name = crate::sem::OperatorName::from_parts(None, "=");
            let binding = crate::sem::ResolvedOperator::resolve(context, &name, true, true);
            let (mut comparison, _) = crate::sem::ArithExpr::build_op(
                context,
                &name,
                Some(argument.clone()),
                Some(arm.when_value.clone()),
            )?;
            comparison.set_operator_binding(binding);
            arm.comparison = Some(comparison);
        }
        Ok(())
    }

    // SQLite's fully typed expressions retain their dialect-specific resolver.
    pub(crate) fn resolve_postgres_or_untyped(
        context: &mut impl crate::sem::ParseContext,
        argument: &Option<SemScalarExpr>,
        arms: &[(SemScalarExpr, SemScalarExpr)],
        otherwise: &Option<SemScalarExpr>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        let unknown = |value: &SemScalarExpr| value.get_type().is_none();
        let untyped = argument.as_ref().is_some_and(unknown)
            || otherwise.as_ref().is_some_and(unknown)
            || arms
                .iter()
                .any(|(condition, result)| unknown(condition) || unknown(result));
        if context.dialect() != crate::ast::Dialect::postgres() && !untyped {
            return Ok(None);
        }
        let mut when_exprs = Vec::new();
        for (condition, result) in arms {
            when_exprs.push(CaseWhen::new(condition.clone(), result.clone()));
        }
        let mut value = Self::new(
            Box::new(argument.clone()),
            when_exprs,
            Box::new(otherwise.clone()),
            None,
            None,
        );
        value.bind_comparisons(context)?;
        value.types = Some(InputTypeConstraint::any());
        if context.dialect() == crate::ast::Dialect::postgres() {
            // Eager typing keeps the result available to enclosing resolvers.
            // Parameter-dependent errors retain the existing graph diagnostic.
            let pending = |value: &SemScalarExpr| {
                MinMaxExpr::input_type(value).is_none() || value.constraint_node().is_some()
            };
            let dependent = argument.as_ref().is_some_and(pending)
                || otherwise
                    .as_ref()
                    .is_some_and(|value| value.constraint_node().is_some())
                || value
                    .when_exprs
                    .iter()
                    .any(|arm| pending(&arm.when_value) || arm.result.constraint_node().is_some());
            let results = value.ordered_results();
            let inputs: Option<Vec<_>> = results.iter().map(MinMaxExpr::input_type).collect();
            if let Some(inputs) = inputs {
                match MinMaxExpr::select_common_type(context, &inputs) {
                    Ok(target) => {
                        value.case_expr_type = Some(target.clone());
                        value.types = if dependent {
                            Some(InputTypeConstraint::exact(&target))
                        } else {
                            None
                        };
                        value.coerce_results(&target);
                    }
                    Err(_) if !dependent => {
                        context.report_analysis_problem(
                            AnalysisProblem::incompatible_case_values(inputs.iter()),
                        )?;
                        value.types = Some(InputTypeConstraint::types([]));
                    }
                    Err(_) => {}
                }
            }
        }
        value.is_not_null = value.result_nullability();
        Ok(Some(SemScalarExpr::case(&value)))
    }
}
