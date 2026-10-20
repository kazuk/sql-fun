use crate::sem::scalar_expr::SemScalarExprNode;
use crate::sem::{
    AnalysisError, AnalysisProblem, InputTypeConstraint, MinMaxExpr, ParseContext, SemScalarExpr,
    TypeReference,
};

/// COALESCE with ordered analyzed operands and its common result-type domain.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CoalesceExpr {
    arg_exprs: Vec<SemScalarExpr>,
    coalesce_type: Option<TypeReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    types: Option<InputTypeConstraint>,
    #[serde(skip)]
    node: Option<usize>,
}

impl SemScalarExprNode for CoalesceExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.coalesce_type.clone()
    }

    fn is_not_null(&self) -> Option<bool> {
        // Preserve the existing conservative metadata; general nullability
        // analysis is separate from common-type constraint registration.
        Some(false)
    }
}

impl CoalesceExpr {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        arguments: &[SemScalarExpr],
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut value = Self {
            arg_exprs: arguments.to_vec(),
            coalesce_type: None,
            types: Some(InputTypeConstraint::any()),
            node: None,
        };
        let mut first_non_null = None;
        for argument in arguments {
            if let Some(first) = first_non_null {
                context.report_analysis_problem(AnalysisProblem::unreachable_coalesce_argument(
                    argument, first,
                ))?;
            } else if argument.is_not_null() == Some(true) {
                first_non_null = Some(argument);
            }
        }
        if context.dialect() == crate::ast::Dialect::sqlite() {
            if arguments.len() < 2 {
                context.report_analysis_problem(AnalysisProblem::parameter_constraint(
                    "SQLite COALESCE requires at least two arguments",
                ))?;
                value.types = Some(InputTypeConstraint::types([]));
            }
            // SQLite does not coerce all operands to a PostgreSQL common type.
            // Parameter-dependent and heterogeneous result typing stays explicit.
            return Ok(SemScalarExpr::coalesce(&value));
        }
        if arguments
            .iter()
            .all(|arg| MinMaxExpr::input_type(arg).is_some())
        {
            let inputs: Vec<_> = arguments
                .iter()
                .filter_map(MinMaxExpr::input_type)
                .collect();
            match MinMaxExpr::select_common_type(context, &inputs) {
                Ok(target) => {
                    value.coalesce_type = Some(target.clone());
                    value.types = Some(InputTypeConstraint::exact(&target));
                    value.coerce(&target);
                }
                Err(_) => {
                    value.types = Some(InputTypeConstraint::types([]));
                    context.report_analysis_problem(
                        AnalysisProblem::coalesce_expr_type_unknown(&arguments),
                    )?;
                }
            }
        }
        Ok(SemScalarExpr::coalesce(&value))
    }

    /// Operands in source order, including any determined implicit coercions.
    pub fn arguments(&self) -> &[SemScalarExpr] {
        &self.arg_exprs
    }

    /// Inferred output domain, when retained by semantic analysis.
    pub const fn types(&self) -> Option<&InputTypeConstraint> {
        self.types.as_ref()
    }

    pub(crate) const fn node(&self) -> Option<usize> {
        self.node
    }

    pub(super) const fn canonicalize_grouping_key(&mut self) {
        self.node = None;
    }

    fn coerce(&mut self, target: &TypeReference) {
        for argument in &mut self.arg_exprs {
            if argument.get_type().as_ref() != Some(target)
                || MinMaxExpr::input_type(argument) == Some(MinMaxExpr::unknown())
            {
                *argument = SemScalarExpr::wrap_implicit_cast(target, argument);
            }
        }
    }

    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for value in &mut self.arg_exprs {
            value.walk(visitor)?;
        }
        Ok(())
    }

    pub(crate) fn collect_constraints(
        &mut self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if context.dialect() == crate::ast::Dialect::sqlite() {
            return Ok(());
        }
        if self
            .arg_exprs
            .iter()
            .any(|value| value.constraint_node().is_some())
            || self
                .types
                .as_ref()
                .is_some_and(|types| types.admissible_types() == Some(&[]))
        {
            let (node, types) = parameters.unify_coalesce_types(context, &self.arg_exprs)?;
            self.node = Some(node);
            self.coalesce_type = types.single_type();
            self.types = Some(types);
        }
        Ok(())
    }

    pub(crate) fn apply_constraints(&mut self, constraints: &crate::sem::ParameterConstraints) {
        let Some(node) = self.node else {
            return;
        };
        let types = constraints.node_domain(node).clone();
        self.coalesce_type = types.single_type();
        self.types = Some(types);
        if let Some(target) = self.coalesce_type.clone() {
            self.coerce(&target);
        }
    }
}

#[cfg(test)]
mod tests;
