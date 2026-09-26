use crate::sem::{AnalysisError, OperatorDefinition, OperatorName, ParseContext, SemScalarExpr};

impl super::ArithExpr {
    /// create a instance
    pub fn build_binary_op<TParseContext>(
        context: TParseContext,
        name: &OperatorName,
        lexpr: SemScalarExpr,
        rexpr: SemScalarExpr,
        operator_def: OperatorDefinition,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::build_binary_op_resolved(context, name, lexpr, rexpr, operator_def)
    }

    pub(crate) fn build_binary_op_resolved<TParseContext>(
        mut context: TParseContext,
        name: &OperatorName,
        lexpr: SemScalarExpr,
        rexpr: SemScalarExpr,
        operator_def: OperatorDefinition,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: crate::sem::resolution::ResolutionContext,
    {
        let resolved_bin_op =
            Self::resolve_overload_binary(&mut context, operator_def, lexpr, rexpr)?;
        resolved_bin_op.into_call_site_expr(context, name)
    }
}

#[cfg(test)]
mod test_arith_expr_build_binary_op {
    use crate::{
        sem::{
            BinaryOperatorDefinition, CastContext, CastDefinition, OperatorDefinition,
            OperatorName, SemScalarExpr, TypeReference,
            names::ValidOperatorName,
            scalar_expr::{arith_expr::ArithExpr, const_expr::ScalarConstExpr},
        },
        test_helpers::{TestParseContext, test_context},
    };

    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_arith_expr_build_binary_op(mut test_context: TestParseContext) -> TestResult {
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

        let (_expr, _context) = ArithExpr::build_binary_op_resolved(
            test_context,
            &name,
            expr.clone(),
            expr.clone(),
            operator_def,
        )?;

        Ok(())
    }
}
