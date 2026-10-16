use crate::sem::{
    AnalysisError, OperatorName, ParseContext, SemScalarExpr,
    scalar_expr::arith_expr::UndefinedOperatorAnyCallSite,
};

impl super::ArithExpr {
    /// create a instance
    pub fn build_op_any<TParseContext>(
        context: TParseContext,
        name: &OperatorName,
        lexpr: Option<SemScalarExpr>,
        rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::build_op_any_resolved(context, name, lexpr, rexpr)
    }

    pub(crate) fn build_op_any_resolved<TParseContext>(
        mut context: TParseContext,
        name: &OperatorName,
        lexpr: Option<SemScalarExpr>,
        rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: crate::sem::resolution::ResolutionContext,
    {
        let Some(lexpr) = lexpr else {
            AnalysisError::raise_unexpected_none("a_expr.lexpr")?
        };
        let Some(rexpr) = rexpr else {
            AnalysisError::raise_unexpected_none("a_expr.rexpr")?
        };

        if let Some(operator_def) = context.get_operator_op(name, true, true) {
            if lexpr.get_type().is_none() || rexpr.get_type().is_none() {
                return Ok((Self::deferred_any_operator(name, lexpr, rexpr), context));
            }
            let operator_def = operator_def.clone();

            let resolved_bin_op =
                Self::resolve_overload_binary(&mut context, operator_def, lexpr, rexpr)?;
            resolved_bin_op.into_any_call_site_expr(context, name)
        } else {
            UndefinedOperatorAnyCallSite::undefined_operator_any_resolved(
                context, name, lexpr, rexpr,
            )
        }
    }
}

#[cfg(test)]
mod test_arith_expr_build_op_any {
    use crate::{
        sem::{
            ArithExpr, BinaryOperatorDefinition, CastContext, CastDefinition, OperatorDefinition,
            OperatorName, SemScalarExpr, TypeReference, names::ValidOperatorName,
            scalar_expr::const_expr::ScalarConstExpr,
        },
        test_helpers::{TestParseContext, test_context},
    };

    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_build_op_any(mut test_context: TestParseContext) -> TestResult {
        let name = OperatorName::Valid(ValidOperatorName::new("+"));
        let type_ref = TypeReference::from_master_type_name("int4");
        test_context.set_get_implicit_cast_result(
            &type_ref,
            &type_ref,
            Some(CastDefinition::new(CastContext::NoConversion)),
        );
        let expr = SemScalarExpr::new_const(ScalarConstExpr::new_integer(10));
        let operator_def = BinaryOperatorDefinition::new(&type_ref, &type_ref, &type_ref, true);
        let operator_def = OperatorDefinition::Binary(vec![operator_def]);
        test_context.set_get_operator_op_result(&name, true, true, Some(&operator_def));

        ArithExpr::build_op_any_resolved(
            test_context,
            &name,
            Some(expr.clone()),
            Some(expr.clone()),
        )?;

        Ok(())
    }
}
