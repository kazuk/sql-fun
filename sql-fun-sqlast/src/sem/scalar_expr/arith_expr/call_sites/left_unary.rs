use crate::sem::{
    AnalysisError, OperatorName, ParseContext, SemScalarExpr, TypeReference,
    UnaryOperatorDefinition, scalar_expr::arith_expr::ArithExpr,
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LeftUnaryCallSite {
    operator_name: OperatorName,
    operator_def: UnaryOperatorDefinition,
    arg: SemScalarExpr,
}

impl LeftUnaryCallSite {
    pub fn op_left_unary<TParseContext>(
        context: TParseContext,
        name: &OperatorName,
        operator_def: UnaryOperatorDefinition,
        lexpr: SemScalarExpr,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::op_left_unary_resolved(context, name, operator_def, lexpr)
    }

    pub(crate) fn op_left_unary_resolved<TParseContext>(
        context: TParseContext,
        name: &OperatorName,
        operator_def: UnaryOperatorDefinition,
        lexpr: SemScalarExpr,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: crate::sem::resolution::ResolutionContext,
    {
        let call_site = Self {
            operator_name: name.clone(),
            operator_def,
            arg: lexpr,
        };
        Ok((
            SemScalarExpr::Arith(ArithExpr::OpLeftUnary(Box::new(call_site))),
            context,
        ))
    }

    pub fn is_not_null(&self) -> Option<bool> {
        self.arg.is_not_null()
    }

    pub const fn get_return_type(&self) -> &TypeReference {
        self.operator_def.get_return_type()
    }

    pub const fn is_strict(&self) -> bool {
        self.operator_def.is_strict()
    }
}

#[cfg(test)]
mod test_left_unary_call_site {
    use crate::sem::scalar_expr::arith_expr::call_sites::left_unary::LeftUnaryCallSite;
    use crate::{
        sem::{
            OperatorName, SemScalarExpr, TypeReference, UnaryOperatorDefinition,
            names::ValidOperatorName, scalar_expr::const_expr::ScalarConstExpr,
        },
        test_helpers::{TestParseContext, test_context},
    };

    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_op_left_unary(test_context: TestParseContext) -> TestResult {
        let type_ref = TypeReference::from_master_type_name("int4");

        let operator_def = UnaryOperatorDefinition::new(&type_ref, &type_ref, true);
        let expr = SemScalarExpr::new_const(ScalarConstExpr::new_integer(10));

        LeftUnaryCallSite::op_left_unary_resolved(
            test_context,
            &OperatorName::Valid(ValidOperatorName::new("+")),
            operator_def,
            expr.clone(),
        )?;
        Ok(())
    }
}
