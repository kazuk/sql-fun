use crate::sem::scalar_expr::arith_expr::call_sites::binary_is_not_null;
use crate::{
    sem::{
        AnalysisError, AnalysisProblem, BinaryOperatorDefinition, OperatorName, ParseContext,
        SemScalarExpr, TypeReference, scalar_expr::arith_expr::ArithExpr,
    },
    syn::AExprKind,
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AllBinaryCallSite {
    operator_name: OperatorName,
    #[serde(default)]
    binding: Option<Box<crate::sem::ResolvedOperator>>,
    operator_def: BinaryOperatorDefinition,
    left_arg: SemScalarExpr,
    right_arg: SemScalarExpr,
}

impl AllBinaryCallSite {
    pub fn from_operator_definition<TParseContext>(
        mut context: TParseContext,
        name: &OperatorName,
        operator_def: BinaryOperatorDefinition,
        lexpr: SemScalarExpr,
        rexpr: SemScalarExpr,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (value, _) = Self::from_operator_definition_resolved(
            &mut context,
            name,
            operator_def,
            lexpr,
            rexpr,
        )?;
        Ok((value, context))
    }

    pub(crate) fn from_operator_definition_resolved<'context, TParseContext>(
        context: &'context mut TParseContext,
        name: &OperatorName,
        operator_def: BinaryOperatorDefinition,
        lexpr: SemScalarExpr,
        rexpr: SemScalarExpr,
    ) -> Result<(SemScalarExpr, &'context mut TParseContext), AnalysisError>
    where
        TParseContext: crate::sem::ParseContext,
    {
        let call_site = Self {
            operator_name: name.clone(),
            binding: None,
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

    pub const fn get_return_type(&self) -> &TypeReference {
        self.operator_def.get_return_type()
    }

    pub const fn is_strict(&self) -> bool {
        self.operator_def.is_strict()
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UndefinedOperatorAllCallSite {
    operator_name: OperatorName,
    #[serde(default)]
    binding: Option<Box<crate::sem::ResolvedOperator>>,
    larg: SemScalarExpr,
    rarg: SemScalarExpr,
}

impl UndefinedOperatorAllCallSite {
    pub const fn is_not_null(&self) -> Option<bool> {
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
        let (value, _) = Self::undefined_operator_all_resolved(&mut context, name, lexpr, rexpr)?;
        Ok((value, context))
    }

    pub(crate) fn undefined_operator_all_resolved<'context, TParseContext>(
        context: &'context mut TParseContext,
        name: &OperatorName,
        lexpr: SemScalarExpr,
        rexpr: SemScalarExpr,
    ) -> Result<(SemScalarExpr, &'context mut TParseContext), AnalysisError>
    where
        TParseContext: crate::sem::ParseContext,
    {
        context.report_analysis_problem(AnalysisProblem::undefined_operator(
            AExprKind::AexprOp,
            name,
            &Some(lexpr.clone()),
            &Some(rexpr.clone()),
        ))?;
        let call_site = Self {
            operator_name: name.clone(),
            binding: None,
            larg: lexpr,
            rarg: rexpr,
        };

        Ok((
            SemScalarExpr::Arith(ArithExpr::UndefinedOperatorAll(Box::new(call_site))),
            context,
        ))
    }
}

impl AllBinaryCallSite {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.left_arg.walk(visitor)?;
        self.right_arg.walk(visitor)?;
        Ok(())
    }
}

impl UndefinedOperatorAllCallSite {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.larg.walk(visitor)?;
        self.rarg.walk(visitor)?;
        Ok(())
    }
}

impl AllBinaryCallSite {
    pub(crate) fn collect_constraints(
        &self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
    ) {
        if self.left_arg.constraint_node().is_some() || self.right_arg.constraint_node().is_some() {
            parameters.special_operator("ALL", &[&self.left_arg, &self.right_arg]);
        }
    }
}

impl UndefinedOperatorAllCallSite {
    pub(crate) fn collect_constraints(
        &self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
    ) {
        if self.larg.constraint_node().is_some() || self.rarg.constraint_node().is_some() {
            parameters.special_operator("ALL", &[&self.larg, &self.rarg]);
        }
    }
}

impl UndefinedOperatorAllCallSite {
    pub(crate) fn from_inputs(
        name: &OperatorName,
        left: SemScalarExpr,
        right: SemScalarExpr,
    ) -> Self {
        Self {
            operator_name: name.clone(),
            binding: None,
            larg: left,
            rarg: right,
        }
    }
}

impl AllBinaryCallSite {
    pub(crate) fn set_binding(&mut self, binding: crate::sem::ResolvedOperator) {
        self.binding = Some(Box::new(binding));
    }
    pub(crate) fn binding(&self) -> Option<&crate::sem::ResolvedOperator> {
        self.binding.as_deref()
    }
}

impl UndefinedOperatorAllCallSite {
    pub(crate) fn set_binding(&mut self, binding: crate::sem::ResolvedOperator) {
        self.binding = Some(Box::new(binding));
    }
    pub(crate) fn binding(&self) -> Option<&crate::sem::ResolvedOperator> {
        self.binding.as_deref()
    }
}
