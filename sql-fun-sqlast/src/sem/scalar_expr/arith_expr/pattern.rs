use crate::{
    names::OperatorName,
    sem::{
        AnalysisError, InputTypeConstraint, MinMaxExpr, ParseContext, ResolvedOperator,
        SemScalarExpr,
        parameter_resolution::ParameterResolution,
        scalar_expr::arith_expr::ArithExpr,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

/// A parameter-dependent pattern call whose unknown literals still need SQL context.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PatternExpr {
    operands: [SemScalarExpr; 2],
    binding: ResolvedOperator,
}

impl PatternExpr {
    pub(super) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for operand in &mut self.operands {
            operand.walk(visitor)?;
        }
        Ok(())
    }
    pub(super) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        for operand in &self.operands {
            operand.visit(visitor)?;
        }
        Ok(())
    }
    pub(super) fn collect_constraints(
        &self,
        parameters: &mut ParameterResolution<'_>,
        context: &mut impl ParseContext,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        let mut operands = self.operands.clone();
        for operand in &mut operands {
            if MinMaxExpr::input_type(operand) == Some(MinMaxExpr::unknown()) {
                *operand = parameters.operation(
                    "unknown pattern literal",
                    std::slice::from_ref(operand),
                    &InputTypeConstraint::any(),
                    operand.is_not_null(),
                );
            }
        }
        let mut result =
            self.binding
                .infer(parameters, context, Some(&operands[0]), Some(&operands[1]))?;
        if let Some(value) = &mut result {
            value.retain_operator_binding(&self.binding);
        }
        Ok(result)
    }
    pub(super) const fn binding(&self) -> &ResolvedOperator {
        &self.binding
    }
    pub(super) fn set_binding(&mut self, binding: ResolvedOperator) {
        self.binding = binding;
    }
}

impl ArithExpr {
    pub(crate) fn pattern(
        context: &mut impl ParseContext,
        name: &OperatorName,
        left: Option<SemScalarExpr>,
        right: Option<SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let (Some(left), Some(right)) = (left, right) else {
            return AnalysisError::raise_unexpected_input("pattern operands");
        };
        let unknown = [&left, &right]
            .into_iter()
            .any(|value| MinMaxExpr::input_type(value) == Some(MinMaxExpr::unknown()));
        if unknown && (left.requires_argument_type() || right.requires_argument_type()) {
            let binding = ResolvedOperator::resolve(context, name, true, true);
            return Ok(SemScalarExpr::arithmetic(Self::Pattern(Box::new(
                PatternExpr {
                    operands: [left, right],
                    binding,
                },
            ))));
        }
        let overloads = if let Some(definition) = context.get_operator_op(name, true, true) {
            definition.binary_overloads()
        } else {
            None
        };
        let (left, right) =
            SemScalarExpr::comparison_literal_operands(context, overloads, left, right);
        Ok(Self::build_op(context, name, Some(left), Some(right))?.0)
    }
}
