use crate::sem::{
    AnalysisError, OperatorName, ParseContext, SemScalarExpr, TypeReference,
    UnaryOperatorDefinition, scalar_expr::arith_expr::ArithExpr,
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RightUnaryCallSite {
    operator_name: OperatorName,
    #[serde(default)]
    binding: Option<Box<crate::sem::ResolvedOperator>>,
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
        Self::op_right_unary_resolved(context, name, operator_def, rexpr)
    }

    pub(crate) fn op_right_unary_resolved<TParseContext>(
        context: TParseContext,
        name: &OperatorName,
        operator_def: UnaryOperatorDefinition,
        rexpr: SemScalarExpr,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: crate::sem::resolution::ResolutionContext,
    {
        let call_site = Self {
            operator_name: name.clone(),
            binding: None,
            operator_def,
            arg: rexpr,
        };
        Ok((
            SemScalarExpr::Arith(ArithExpr::OpRightUnary(Box::new(call_site))),
            context,
        ))
    }

    pub const fn get_return_type(&self) -> &TypeReference {
        self.operator_def.get_return_type()
    }

    pub const fn is_strict(&self) -> bool {
        self.operator_def.is_strict()
    }

    pub fn is_not_null(&self) -> Option<bool> {
        self.arg.is_not_null()
    }
}

impl RightUnaryCallSite {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.arg.walk(visitor)?;
        Ok(())
    }
}

impl RightUnaryCallSite {
    pub(crate) fn collect_constraints(
        &self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
        context: &mut crate::sem::Resolution<'_>,
        dialect: crate::ast::Dialect,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        if let Some(binding) = &self.binding {
            binding.infer(parameters, context, dialect, None, Some(&self.arg))
        } else {
            parameters.operator(context, dialect, &self.operator_name, None, Some(&self.arg))
        }
    }
}

impl RightUnaryCallSite {
    pub(crate) fn set_binding(&mut self, binding: crate::sem::ResolvedOperator) {
        self.binding = Some(Box::new(binding));
    }
    pub(crate) fn binding(&self) -> Option<&crate::sem::ResolvedOperator> {
        self.binding.as_deref()
    }
}
