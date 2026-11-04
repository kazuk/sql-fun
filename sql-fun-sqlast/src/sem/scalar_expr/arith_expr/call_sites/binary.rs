use crate::{
    names::OperatorName,
    sem::{
        AnalysisError, AnalysisProblem, BinaryOperatorDefinition, ParseContext, ResolvedOperator,
        SemScalarExpr, TypeReference,
        parameter_resolution::ParameterResolution,
        scalar_expr::arith_expr::{ArithExpr, call_sites::binary_is_not_null},
        traversal::{SemanticVisitor, SemanticVisitorMut, optional, visit_optional},
    },
    syn::AExprKind,
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BinaryOpCallSite {
    operator_name: OperatorName,
    #[serde(default)]
    binding: Option<Box<ResolvedOperator>>,
    left_arg: SemScalarExpr,
    left_arg_type: Option<TypeReference>,
    right_arg: SemScalarExpr,
    right_arg_type: Option<TypeReference>,
    return_type: TypeReference,
    strict: bool,
}

impl BinaryOpCallSite {
    pub(crate) const fn argument(&self, index: usize) -> Option<&SemScalarExpr> {
        match index {
            0 => Some(&self.left_arg),
            1 => Some(&self.right_arg),
            _ => None,
        }
    }

    pub fn is_not_null(&self) -> Option<bool> {
        binary_is_not_null(&self.left_arg, &self.right_arg)
    }

    pub const fn get_return_type(&self) -> &TypeReference {
        &self.return_type
    }

    pub const fn is_strict(&self) -> bool {
        self.strict
    }

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
}

#[cfg(test)]
mod test_binary_op_call_site {
    use crate::{
        names::{OperatorName, ValidOperatorName},
        sem::{
            BinaryOperatorDefinition, SemScalarExpr, TypeReference,
            scalar_expr::{
                arith_expr::call_sites::binary::BinaryOpCallSite, const_expr::ScalarConstExpr,
            },
        },
        test_helpers::{TestParseContext, test_context},
    };

    #[rstest::rstest]
    fn test_from_operator_definition(test_context: TestParseContext) -> ::testresult::TestResult {
        let type_ref = TypeReference::from_master_type_name("int4");

        let operator_def = BinaryOperatorDefinition::new(&type_ref, &type_ref, &type_ref, true);
        let expr = SemScalarExpr::new_const(ScalarConstExpr::new_integer(10));

        BinaryOpCallSite::from_operator_definition(
            test_context,
            &OperatorName::Valid(ValidOperatorName::new("+")),
            operator_def,
            expr.clone(),
            expr.clone(),
        )?;
        Ok(())
    }
}

impl BinaryOpCallSite {
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

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UndefinedOperatorCallSite {
    operator_name: OperatorName,
    #[serde(default)]
    binding: Option<Box<ResolvedOperator>>,
    larg: Option<SemScalarExpr>,
    rarg: Option<SemScalarExpr>,
}

impl UndefinedOperatorCallSite {
    pub(crate) const fn argument(&self, index: usize) -> Option<&SemScalarExpr> {
        match index {
            0 => self.larg.as_ref(),
            1 => self.rarg.as_ref(),
            _ => None,
        }
    }

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
        mut context: TParseContext,
        name: &OperatorName,
        lexpr: Option<SemScalarExpr>,
        rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (value, _) = Self::undefined_operator_resolved(&mut context, name, lexpr, rexpr)?;
        Ok((value, context))
    }

    pub(crate) fn undefined_operator_resolved<'context, TParseContext>(
        context: &'context mut TParseContext,
        name: &OperatorName,
        lexpr: Option<SemScalarExpr>,
        rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, &'context mut TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        context.report_analysis_problem(AnalysisProblem::undefined_operator(
            AExprKind::AexprOp,
            name,
            lexpr.as_ref(),
            rexpr.as_ref(),
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

impl UndefinedOperatorCallSite {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        optional(&mut self.larg, visitor)?;
        optional(&mut self.rarg, visitor)?;
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        visit_optional(&self.larg, visitor)?;
        visit_optional(&self.rarg, visitor)?;
        Ok(())
    }
}

impl BinaryOpCallSite {
    pub(crate) fn collect_constraints(
        &self,
        parameters: &mut ParameterResolution<'_>,
        context: &mut impl ParseContext,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        if let Some(binding) = &self.binding {
            binding.infer(
                parameters,
                context,
                Some(&self.left_arg),
                Some(&self.right_arg),
            )
        } else {
            parameters.operator(
                context,
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
        parameters: &mut ParameterResolution<'_>,
        context: &mut impl ParseContext,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        if let Some(binding) = &self.binding {
            binding.infer(parameters, context, self.larg.as_ref(), self.rarg.as_ref())
        } else {
            parameters.operator(
                context,
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
    pub(crate) fn set_binding(&mut self, binding: ResolvedOperator) {
        self.binding = Some(Box::new(binding));
    }
    pub(crate) fn binding(&self) -> Option<&ResolvedOperator> {
        self.binding.as_deref()
    }
}

impl UndefinedOperatorCallSite {
    pub(crate) fn set_binding(&mut self, binding: ResolvedOperator) {
        self.binding = Some(Box::new(binding));
    }
    pub(crate) fn binding(&self) -> Option<&ResolvedOperator> {
        self.binding.as_deref()
    }
}
