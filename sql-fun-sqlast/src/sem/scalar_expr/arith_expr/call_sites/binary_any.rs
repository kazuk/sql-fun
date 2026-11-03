use crate::{
    names::OperatorName,
    sem::{
        AnalysisError, AnalysisProblem, BinaryOperatorDefinition, ParseContext, ResolvedOperator,
        SemScalarExpr, TypeReference,
        parameter_resolution::ParameterResolution,
        scalar_expr::arith_expr::ArithExpr,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
    syn::AExprKind,
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnyBinaryCallSite {
    operator_name: OperatorName,
    #[serde(default)]
    binding: Option<Box<ResolvedOperator>>,
    operator_def: BinaryOperatorDefinition,
    left_arg: SemScalarExpr,
    right_arg: SemScalarExpr,
}

impl AnyBinaryCallSite {
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
        TParseContext: ParseContext,
    {
        let call_site = Self {
            operator_name: name.clone(),
            binding: None,
            operator_def,
            left_arg: lexpr,
            right_arg: rexpr,
        };

        Ok((
            SemScalarExpr::Arith(ArithExpr::OpAnyBinary(Box::new(call_site))),
            context,
        ))
    }

    pub const fn is_not_null(&self) -> Option<bool> {
        // Empty arrays and NULL elements make scalar strictness insufficient.
        None
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
    #[serde(default)]
    binding: Option<Box<ResolvedOperator>>,
    larg: SemScalarExpr,
    rarg: SemScalarExpr,
}

impl UndefinedOperatorAnyCallSite {
    pub const fn is_not_null(&self) -> Option<bool> {
        None
    }

    pub fn undefined_operator_any<TParseContext>(
        mut context: TParseContext,
        name: &OperatorName,
        lexpr: SemScalarExpr,
        rexpr: SemScalarExpr,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (value, _) = Self::undefined_operator_any_resolved(&mut context, name, lexpr, rexpr)?;
        Ok((value, context))
    }

    pub(crate) fn undefined_operator_any_resolved<'context, TParseContext>(
        context: &'context mut TParseContext,
        name: &OperatorName,
        lexpr: SemScalarExpr,
        rexpr: SemScalarExpr,
    ) -> Result<(SemScalarExpr, &'context mut TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        context.report_analysis_problem(AnalysisProblem::undefined_operator(
            AExprKind::AexprOpAny,
            name,
            Some(&lexpr),
            Some(&rexpr),
        ))?;
        let call_site = Self {
            operator_name: name.clone(),
            binding: None,
            larg: lexpr,
            rarg: rexpr,
        };

        Ok((
            SemScalarExpr::Arith(ArithExpr::UndefinedOperatorAny(Box::new(call_site))),
            context,
        ))
    }
}

impl AnyBinaryCallSite {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.left_arg.walk(visitor)?;
        self.right_arg.walk(visitor)?;
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        self.left_arg.visit(visitor)?;
        self.right_arg.visit(visitor)?;
        Ok(())
    }
}

impl UndefinedOperatorAnyCallSite {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.larg.walk(visitor)?;
        self.rarg.walk(visitor)?;
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        self.larg.visit(visitor)?;
        self.rarg.visit(visitor)?;
        Ok(())
    }
}

impl AnyBinaryCallSite {
    pub(crate) fn collect_constraints(&self, parameters: &mut ParameterResolution<'_>) {
        if self.left_arg.constraint_node().is_some() || self.right_arg.constraint_node().is_some() {
            parameters.special_operator("ANY", &[&self.left_arg, &self.right_arg]);
        }
    }
}

impl UndefinedOperatorAnyCallSite {
    pub(crate) fn collect_constraints(&self, parameters: &mut ParameterResolution<'_>) {
        if self.larg.constraint_node().is_some() || self.rarg.constraint_node().is_some() {
            parameters.special_operator("ANY", &[&self.larg, &self.rarg]);
        }
    }
}

impl UndefinedOperatorAnyCallSite {
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

impl AnyBinaryCallSite {
    pub(crate) fn set_binding(&mut self, binding: ResolvedOperator) {
        self.binding = Some(Box::new(binding));
    }
    pub(crate) fn binding(&self) -> Option<&ResolvedOperator> {
        self.binding.as_deref()
    }
}

impl UndefinedOperatorAnyCallSite {
    pub(crate) fn set_binding(&mut self, binding: ResolvedOperator) {
        self.binding = Some(Box::new(binding));
    }
    pub(crate) fn binding(&self) -> Option<&ResolvedOperator> {
        self.binding.as_deref()
    }
}
