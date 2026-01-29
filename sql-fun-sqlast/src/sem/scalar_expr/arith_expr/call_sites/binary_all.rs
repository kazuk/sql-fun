use crate::{
    sem::{
        AnalysisError, AnalysisProblem, BinaryOperatorDefinition, OperatorName, ParseContext,
        SemScalarExpr, TypeReference, scalar_expr::arith_expr::ArithExpr,
    },
    syn::AExprKind,
};

use super::binary_is_not_null;

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AllBinaryCallSite {
    operator_name: OperatorName,
    operator_def: BinaryOperatorDefinition,
    left_arg: SemScalarExpr,
    right_arg: SemScalarExpr,
}

impl AllBinaryCallSite {
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
        let call_site = Self {
            operator_name: name.clone(),
            operator_def,
            left_arg: lexpr,
            right_arg: rexpr,
        };

        Ok((
            SemScalarExpr::Arith(ArithExpr::OpAllBinary(Box::new(call_site))),
            context,
        ))
    }

    pub fn is_not_null(&self) -> Option<bool> {
        binary_is_not_null(&self.left_arg, &self.right_arg)
    }

    pub fn get_return_type(&self) -> &TypeReference {
        self.operator_def.get_return_type()
    }

    pub fn is_strict(&self) -> bool {
        self.operator_def.is_strict()
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UndefinedOperatorAllCallSite {
    operator_name: OperatorName,
    larg: SemScalarExpr,
    rarg: SemScalarExpr,
}

impl UndefinedOperatorAllCallSite {
    pub fn is_not_null(&self) -> Option<bool> {
        None
    }

    pub fn undefined_operator_all<TParseContext>(
        mut context: TParseContext,
        name: &OperatorName,
        lexpr: SemScalarExpr,
        rexpr: SemScalarExpr,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        context.report_problem(AnalysisProblem::undefined_operator(
            AExprKind::AexprOp,
            name,
            &Some(lexpr.clone()),
            &Some(rexpr.clone()),
        ))?;
        let call_site = UndefinedOperatorAllCallSite {
            operator_name: name.clone(),
            larg: lexpr,
            rarg: rexpr,
        };

        Ok((
            SemScalarExpr::Arith(ArithExpr::UndefinedOperatorAll(Box::new(call_site))),
            context,
        ))
    }
}
