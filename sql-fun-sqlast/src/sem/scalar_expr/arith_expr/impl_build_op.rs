use crate::sem::{
    AnalysisError, OperatorName, ParseContext, SemScalarExpr,
    scalar_expr::arith_expr::UndefinedOperatorCallSite,
};

use super::ArithExpr;

impl ArithExpr {
    pub(super) fn build_op<TParseContext>(
        mut context: TParseContext,
        name: &OperatorName,
        lexpr: Option<SemScalarExpr>,
        rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        if let Some(operator_def) = context.get_operator_op(name, lexpr.is_some(), rexpr.is_some())
        {
            let operator_def = operator_def.clone();
            match (lexpr, rexpr) {
                (Some(l), Some(r)) => Self::build_binary_op(context, name, l, r, operator_def),
                (None, None) => AnalysisError::raise_unexpected_input("lexpr, rexpr both None")?,
                (Some(a), None) => {
                    let resolved = Self::resolve_overload_unary(&mut context, operator_def, a)?;
                    resolved.into_left_unary_call_site_expr(context, name)
                }
                (None, Some(a)) => {
                    let resolved = Self::resolve_overload_unary(&mut context, operator_def, a)?;
                    resolved.into_right_unary_call_site_expr(context, name)
                }
            }
        } else {
            tracing::trace!("operator not found in context");
            UndefinedOperatorCallSite::undefined_operator(context, name, lexpr, rexpr)
        }
    }
}

#[cfg(test)]
mod test_arith_expr_build_op {
    use crate::{
        sem::{
            BinaryOperatorDefinition, CastContext, CastDefinition, OperatorDefinition,
            OperatorName, SemScalarExpr, TypeReference, UnaryOperatorDefinition,
            names::ValidOperatorName,
            scalar_expr::{arith_expr::ArithExpr, const_expr::ScalarConstExpr},
        },
        test_helpers::{TestParseContext, test_context},
    };
    use testresult::TestResult;

    #[rstest::rstest]
    #[case("+", Some(10), Some(10))]
    #[case("~", None, Some(10))]
    #[case("^", Some(10), None)]
    fn test_build_op(
        mut test_context: TestParseContext,
        #[case] op_name: &str,
        #[case] left: Option<i32>,
        #[case] right: Option<i32>,
    ) -> TestResult {
        let name = OperatorName::Valid(ValidOperatorName::new(op_name));
        let ty_ref = TypeReference::from_master_type_name("int4");

        test_context.set_get_implicit_cast_result(
            &ty_ref,
            &ty_ref,
            Some(CastDefinition::new(CastContext::NoConversion)),
        );

        let op_def = match (left, right) {
            (Some(_), Some(_)) => {
                let binary = BinaryOperatorDefinition::new(&ty_ref, &ty_ref, &ty_ref, true);
                OperatorDefinition::Binary(vec![binary])
            }
            (Some(_), None) => {
                let unary = UnaryOperatorDefinition::new(&ty_ref, &ty_ref, true);
                OperatorDefinition::LeftUnary(vec![unary])
            }
            (None, Some(_)) => {
                let unary = UnaryOperatorDefinition::new(&ty_ref, &ty_ref, true);
                OperatorDefinition::RightUnary(vec![unary])
            }
            _ => unreachable!(),
        };

        test_context.set_get_operator_op_result(
            &name,
            left.is_some(),
            right.is_some(),
            Some(&op_def),
        );

        let left = left.map(|v| SemScalarExpr::new_const(ScalarConstExpr::new_integer(v)));
        let right = right.map(|v| SemScalarExpr::new_const(ScalarConstExpr::new_integer(v)));

        ArithExpr::build_op(test_context, &name, left, right)?;
        Ok(())
    }
}
