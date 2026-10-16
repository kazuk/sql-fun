use crate::{
    sem::{
        AnalysisError, AnalysisProblem, BinaryOperatorDefinition, OperatorName, ParseContext,
        SemScalarExpr, TypeReference, scalar_expr::arith_expr::ArithExpr,
    },
    syn::AExprKind,
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BinaryOpCallSite {
    operator_name: OperatorName,
    #[serde(default)]
    binding: Option<Box<crate::sem::ResolvedOperator>>,
    left_arg: SemScalarExpr,
    left_arg_type: Option<TypeReference>,
    right_arg: SemScalarExpr,
    right_arg_type: Option<TypeReference>,
    return_type: TypeReference,
    strict: bool,
}

impl BinaryOpCallSite {
    pub fn is_not_null(&self) -> Option<bool> {
        super::binary_is_not_null(&self.left_arg, &self.right_arg)
    }

    pub const fn get_return_type(&self) -> &TypeReference {
        &self.return_type
    }

    pub const fn is_strict(&self) -> bool {
        self.strict
    }

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
            binding: None,
            left_arg_type: lexpr.get_type().clone(),
            right_arg_type: rexpr.get_type().clone(),
            left_arg: lexpr,
            right_arg: rexpr,
            return_type: operator_def.get_return_type().clone(),
            strict: operator_def.is_strict(),
        };

        Ok((
            SemScalarExpr::Arith(ArithExpr::OpBinary(Box::new(call_site))),
            context,
        ))
    }
}

#[cfg(test)]
mod test_binary_op_call_site {
    use crate::sem::scalar_expr::arith_expr::call_sites::binary::BinaryOpCallSite;
    use crate::{
        sem::{
            BinaryOperatorDefinition, OperatorName, SemScalarExpr, TypeReference,
            names::ValidOperatorName, scalar_expr::const_expr::ScalarConstExpr,
        },
        test_helpers::{TestParseContext, test_context},
    };

    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_from_operator_definition(test_context: TestParseContext) -> TestResult {
        let type_ref = TypeReference::from_master_type_name("int4");

        let operator_def = BinaryOperatorDefinition::new(&type_ref, &type_ref, &type_ref, true);
        let expr = SemScalarExpr::new_const(ScalarConstExpr::new_integer(10));

        BinaryOpCallSite::from_operator_definition_resolved(
            test_context,
            &OperatorName::Valid(ValidOperatorName::new("+")),
            operator_def,
            expr.clone(),
            expr.clone(),
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UndefinedOperatorCallSite {
    operator_name: OperatorName,
    #[serde(default)]
    binding: Option<Box<crate::sem::ResolvedOperator>>,
    larg: Option<SemScalarExpr>,
    rarg: Option<SemScalarExpr>,
}

impl UndefinedOperatorCallSite {
    pub fn inputs_is_not_null(&self) -> Option<bool> {
        match (&self.larg, &self.rarg) {
            (Some(l), Some(r)) => {
                let linn = l.is_not_null();
                if matches!(linn, Some(false)) {
                    return Some(false);
                }
                let rinn = r.is_not_null();
                if matches!(rinn, Some(false)) {
                    return Some(false);
                }
                if matches!(linn, Some(true)) && matches!(rinn, Some(true)) {
                    return Some(true);
                }
                None
            }
            (Some(l), None) => {
                let linn = l.is_not_null();
                if matches!(linn, Some(false)) {
                    return Some(false);
                }
                if matches!(linn, Some(true)) {
                    return Some(true);
                }
                None
            }
            (None, Some(r)) => {
                let rinn = r.is_not_null();
                if matches!(rinn, Some(false)) {
                    return Some(false);
                }
                if matches!(rinn, Some(true)) {
                    return Some(true);
                }
                None
            }
            _ => panic!("left and right both None. this is unexpected"),
        }
    }

    pub fn undefined_operator<TParseContext>(
        context: TParseContext,
        name: &OperatorName,
        lexpr: Option<SemScalarExpr>,
        rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::undefined_operator_resolved(context, name, lexpr, rexpr)
    }

    pub(crate) fn undefined_operator_resolved<TParseContext>(
        mut context: TParseContext,
        name: &OperatorName,
        lexpr: Option<SemScalarExpr>,
        rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: crate::sem::resolution::ResolutionContext,
    {
        context.report_problem(AnalysisProblem::undefined_operator(
            AExprKind::AexprOp,
            name,
            &lexpr,
            &rexpr,
        ))?;
        let call_site = Self {
            operator_name: name.clone(),
            binding: None,
            larg: lexpr,
            rarg: rexpr,
        };

        Ok((
            SemScalarExpr::Arith(ArithExpr::UndefinedOperator(Box::new(call_site))),
            context,
        ))
    }
}

impl BinaryOpCallSite {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.left_arg.walk(visitor)?;
        self.right_arg.walk(visitor)?;
        Ok(())
    }
}

impl UndefinedOperatorCallSite {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        crate::sem::traversal::optional(&mut self.larg, visitor)?;
        crate::sem::traversal::optional(&mut self.rarg, visitor)?;
        Ok(())
    }
}

impl BinaryOpCallSite {
    pub(crate) fn collect_constraints(
        &self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
        context: &mut crate::sem::Resolution<'_>,
        dialect: crate::ast::Dialect,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        if let Some(binding) = &self.binding {
            binding.infer(
                parameters,
                context,
                dialect,
                Some(&self.left_arg),
                Some(&self.right_arg),
            )
        } else {
            parameters.operator(
                context,
                dialect,
                &self.operator_name,
                Some(&self.left_arg),
                Some(&self.right_arg),
            )
        }
    }
}

impl UndefinedOperatorCallSite {
    pub(crate) fn collect_constraints(
        &self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
        context: &mut crate::sem::Resolution<'_>,
        dialect: crate::ast::Dialect,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        if let Some(binding) = &self.binding {
            binding.infer(
                parameters,
                context,
                dialect,
                self.larg.as_ref(),
                self.rarg.as_ref(),
            )
        } else {
            parameters.operator(
                context,
                dialect,
                &self.operator_name,
                self.larg.as_ref(),
                self.rarg.as_ref(),
            )
        }
    }
}

impl UndefinedOperatorCallSite {
    pub(crate) fn from_inputs(
        name: &OperatorName,
        left: Option<SemScalarExpr>,
        right: Option<SemScalarExpr>,
    ) -> Self {
        Self {
            operator_name: name.clone(),
            binding: None,
            larg: left,
            rarg: right,
        }
    }
}

impl BinaryOpCallSite {
    pub(crate) fn set_binding(&mut self, binding: crate::sem::ResolvedOperator) {
        self.binding = Some(Box::new(binding));
    }
    pub(crate) fn binding(&self) -> Option<&crate::sem::ResolvedOperator> {
        self.binding.as_deref()
    }
}

impl UndefinedOperatorCallSite {
    pub(crate) fn set_binding(&mut self, binding: crate::sem::ResolvedOperator) {
        self.binding = Some(Box::new(binding));
    }
    pub(crate) fn binding(&self) -> Option<&crate::sem::ResolvedOperator> {
        self.binding.as_deref()
    }
}
