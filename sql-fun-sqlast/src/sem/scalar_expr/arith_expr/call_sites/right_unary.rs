use crate::{
    names::OperatorName,
    sem::{
        AnalysisError, ParseContext, ResolvedOperator, SemScalarExpr, TypeReference,
        UnaryOperatorDefinition,
        parameter_resolution::ParameterResolution,
        scalar_expr::arith_expr::ArithExpr,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RightUnaryCallSite {
    operator_name: OperatorName,
    #[serde(default)]
    binding: Option<Box<ResolvedOperator>>,
    operator_def: UnaryOperatorDefinition,
    arg: SemScalarExpr,
}

impl RightUnaryCallSite {
    pub fn op_right_unary<TParseContext>(
        mut context: TParseContext,
        name: &OperatorName,
        operator_def: UnaryOperatorDefinition,
        rexpr: SemScalarExpr,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (value, _) = Self::op_right_unary_resolved(&mut context, name, operator_def, rexpr)?;
        Ok((value, context))
    }

    pub(crate) fn op_right_unary_resolved<'context, TParseContext>(
        context: &'context mut TParseContext,
        name: &OperatorName,
        operator_def: UnaryOperatorDefinition,
        rexpr: SemScalarExpr,
    ) -> Result<(SemScalarExpr, &'context mut TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
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
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.arg.walk(visitor)?;
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        self.arg.visit(visitor)?;
        Ok(())
    }
}

impl RightUnaryCallSite {
    pub(crate) fn collect_constraints(
        &self,
        parameters: &mut ParameterResolution<'_>,
        context: &mut impl ParseContext,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        if let Some(binding) = &self.binding {
            binding.infer(parameters, context, None, Some(&self.arg))
        } else {
            parameters.operator(context, &self.operator_name, None, Some(&self.arg))
        }
    }
}

impl RightUnaryCallSite {
    pub(crate) fn set_binding(&mut self, binding: ResolvedOperator) {
        self.binding = Some(Box::new(binding));
    }
    pub(crate) fn binding(&self) -> Option<&ResolvedOperator> {
        self.binding.as_deref()
    }
}
