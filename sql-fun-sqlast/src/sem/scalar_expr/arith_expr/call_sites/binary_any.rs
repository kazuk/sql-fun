use crate::{
    sem::{
        AnalysisError, AnalysisProblem, BinaryOperatorDefinition, OperatorName, ParseContext,
        SemScalarExpr, TypeReference, scalar_expr::arith_expr::ArithExpr,
    },
    syn::AExprKind,
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnyBinaryCallSite {
    operator_name: OperatorName,
    operator_def: BinaryOperatorDefinition,
    left_arg: SemScalarExpr,
    right_arg: SemScalarExpr,
}

impl AnyBinaryCallSite {
    pub fn from_operator_definition<TParseContext>(
        context: TParseContext,
        name: &OperatorName,
        operator_def: BinaryOperatorDefinition,
        lexpr: SemScalarExpr,
        rexpr: SemScalarExpr,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::from_operator_definition_resolved(context, name, operator_def, lexpr, rexpr)
    }

    pub(crate) fn from_operator_definition_resolved<TParseContext>(
        context: TParseContext,
        name: &OperatorName,
        operator_def: BinaryOperatorDefinition,
        lexpr: SemScalarExpr,
        rexpr: SemScalarExpr,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: crate::sem::resolution::ResolutionContext,
    {
        let call_site = Self {
            operator_name: name.clone(),
            operator_def,
            left_arg: lexpr,
            right_arg: rexpr,
        };

        Ok((
            SemScalarExpr::Arith(ArithExpr::OpAnyBinary(Box::new(call_site))),
            context,
        ))
    }

    pub fn is_not_null(&self) -> Option<bool> {
        super::binary_is_not_null(&self.left_arg, &self.right_arg)
    }

    pub const fn get_return_type(&self) -> &TypeReference {
        self.operator_def.get_return_type()
    }

    pub const fn is_strict(&self) -> bool {
        self.operator_def.is_strict()
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UndefinedOperatorAnyCallSite {
    operator_name: OperatorName,
    larg: SemScalarExpr,
    rarg: SemScalarExpr,
}

impl UndefinedOperatorAnyCallSite {
    pub const fn is_not_null(&self) -> Option<bool> {
        None
    }

    pub fn undefined_operator_any<TParseContext>(
        context: TParseContext,
        name: &OperatorName,
        lexpr: SemScalarExpr,
        rexpr: SemScalarExpr,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::undefined_operator_any_resolved(context, name, lexpr, rexpr)
    }

    pub(crate) fn undefined_operator_any_resolved<TParseContext>(
        mut context: TParseContext,
        name: &OperatorName,
        lexpr: SemScalarExpr,
        rexpr: SemScalarExpr,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: crate::sem::resolution::ResolutionContext,
    {
        context.report_problem(AnalysisProblem::undefined_operator(
            AExprKind::AexprOp,
            name,
            &Some(lexpr.clone()),
            &Some(rexpr.clone()),
        ))?;
        let call_site = Self {
            operator_name: name.clone(),
            larg: lexpr,
            rarg: rexpr,
        };

        Ok((
            SemScalarExpr::Arith(ArithExpr::UndefinedOperatorAny(Box::new(call_site))),
            context,
        ))
    }
}
