use crate::sem::{
    AnalysisError, OperatorName, ParseContext, SemScalarExpr, TypeReference,
    UnaryOperatorDefinition, scalar_expr::arith_expr::ArithExpr,
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RightUnaryCallSite {
    operator_name: OperatorName,
    operator_def: UnaryOperatorDefinition,
    arg: SemScalarExpr,
}

impl RightUnaryCallSite {
    pub fn op_right_unary<TParseContext>(
        context: TParseContext,
        name: &OperatorName,
        operator_def: UnaryOperatorDefinition,
        rexpr: SemScalarExpr,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let call_site = RightUnaryCallSite {
            operator_name: name.clone(),
            operator_def,
            arg: rexpr,
        };
        Ok((
            SemScalarExpr::Arith(ArithExpr::OpRightUnary(Box::new(call_site))),
            context,
        ))
    }

    pub fn get_return_type(&self) -> &TypeReference {
        self.operator_def.get_return_type()
    }

    pub fn is_strict(&self) -> bool {
        self.operator_def.is_strict()
    }

    pub fn is_not_null(&self) -> Option<bool> {
        self.arg.is_not_null()
    }
}
