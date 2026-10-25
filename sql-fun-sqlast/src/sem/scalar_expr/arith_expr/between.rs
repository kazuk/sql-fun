use super::ArithExpr;
use crate::sem::{AnalysisError, OperatorName, ParseContext, ResolvedOperator, SemScalarExpr};

impl ArithExpr {
    pub(crate) fn symmetric_between(
        context: &mut impl ParseContext,
        value: &SemScalarExpr,
        start: &SemScalarExpr,
        end: &SemScalarExpr,
        negated: bool,
    ) -> Result<SemScalarExpr, AnalysisError> {
        // Match PostgreSQL's transformAExprBetween. In particular, the negative
        // predicate binds </> independently of the positive predicate's >=/<=.
        let forward = Self::between_comparisons(context, value, start, end, negated)?;
        let reverse = Self::between_comparisons(context, value, end, start, negated)?;
        Ok(if negated {
            SemScalarExpr::logical_and(&[forward, reverse])
        } else {
            SemScalarExpr::logical_or(&[forward, reverse])
        })
    }

    fn between_comparisons(
        context: &mut impl ParseContext,
        value: &SemScalarExpr,
        start: &SemScalarExpr,
        end: &SemScalarExpr,
        negated: bool,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let (lower, upper) = if negated { ("<", ">") } else { (">=", "<=") };
        let lower = Self::between_comparison(context, lower, value, start)?;
        let upper = Self::between_comparison(context, upper, value, end)?;
        Ok(if negated {
            SemScalarExpr::logical_or(&[lower, upper])
        } else {
            SemScalarExpr::logical_and(&[lower, upper])
        })
    }

    fn between_comparison(
        context: &mut impl ParseContext,
        name: &str,
        left: &SemScalarExpr,
        right: &SemScalarExpr,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let name = OperatorName::from_parts(None, name);
        let binding = ResolvedOperator::resolve(context, &name, true, true);
        let (mut expression, _) =
            Self::build_op(context, &name, Some(left.clone()), Some(right.clone()))?;
        expression.set_operator_binding(binding);
        Ok(expression)
    }
}

#[cfg(test)]
mod tests;
