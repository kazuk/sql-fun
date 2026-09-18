use crate::sem::{
    AnalysisError, AnalysisProblem, CastContext, OperatorDefinition, OperatorName, ParseContext,
    SemScalarExpr, TypeReference, UnaryOperatorDefinition,
};

struct UnaryCastCandidate {
    arg: CastContext,
    priority: i32,
    operator: UnaryOperatorDefinition,
}

impl UnaryCastCandidate {
    const fn new(arg: CastContext, priority: i32, operator: UnaryOperatorDefinition) -> Self {
        Self {
            arg,
            priority,
            operator,
        }
    }

    fn enumerate_from_context<TParseContext>(
        context: &TParseContext,
        overloads: &[UnaryOperatorDefinition],
        arg_type: &TypeReference,
    ) -> Vec<Self>
    where
        TParseContext: ParseContext,
    {
        let mut candidates = Vec::new();
        for overload in overloads {
            if let Some(arg_cast) = context.get_implicit_cast(arg_type, overload.arg_type()) {
                let priority = arg_cast.context().overload_priority_score();
                candidates.push(Self::new(arg_cast.context(), priority, overload.clone()));
            }
        }
        candidates
    }

    fn wrap_implicit_cast(&self, arg_expr: SemScalarExpr) -> SemScalarExpr {
        if self.arg.is_implicit() {
            SemScalarExpr::wrap_implicit_cast(self.operator.arg_type(), &arg_expr)
        } else {
            arg_expr
        }
    }

    fn filter_best_candidate(candidates: Vec<Self>) -> Vec<Self> {
        let priority = candidates
            .iter()
            .map(|v| v.priority)
            .max()
            .expect("get max priority");
        candidates
            .into_iter()
            .filter(|v| v.priority == priority)
            .collect()
    }
}

pub(super) struct ResolvedUnaryOperator {
    operator_def: Option<UnaryOperatorDefinition>,
    arg_expr: SemScalarExpr,
}

impl ResolvedUnaryOperator {
    const fn new(operator_def: Option<UnaryOperatorDefinition>, arg_expr: SemScalarExpr) -> Self {
        Self {
            operator_def,
            arg_expr,
        }
    }

    pub(super) fn into_left_unary_call_site_expr<TParseContext>(
        self,
        context: TParseContext,
        name: &OperatorName,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        if let Some(operator) = self.operator_def {
            super::LeftUnaryCallSite::op_left_unary(context, name, operator, self.arg_expr)
        } else {
            super::UndefinedOperatorCallSite::undefined_operator(
                context,
                name,
                None,
                Some(self.arg_expr),
            )
        }
    }

    pub(super) fn into_right_unary_call_site_expr<TParseContext>(
        self,
        context: TParseContext,
        name: &OperatorName,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        if let Some(operator) = self.operator_def {
            super::RightUnaryCallSite::op_right_unary(context, name, operator, self.arg_expr)
        } else {
            super::UndefinedOperatorCallSite::undefined_operator(
                context,
                name,
                None,
                Some(self.arg_expr),
            )
        }
    }
}

impl super::ArithExpr {
    pub(super) fn resolve_overload_unary<TParseContext>(
        context: &mut TParseContext,
        operator_def: OperatorDefinition,
        input: SemScalarExpr,
    ) -> Result<ResolvedUnaryOperator, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let overloads = match &operator_def {
            OperatorDefinition::LeftUnary(u) => u,
            OperatorDefinition::RightUnary(u) => u,
            _ => AnalysisError::raise_unexpected_input("resolve_overload_unary with non unary op")?,
        };

        if let Some(arg_type) = input.get_type() {
            let candidates =
                UnaryCastCandidate::enumerate_from_context(context, overloads, &arg_type);
            if candidates.is_empty() {
                context.report_problem(
                    AnalysisProblem::unary_operator_overload_resolution_failed(
                        &operator_def,
                        &arg_type,
                    ),
                )?;
                return Ok(ResolvedUnaryOperator::new(None, input));
            }
            let filtered = UnaryCastCandidate::filter_best_candidate(candidates);
            if filtered.len() > 1 {
                context.report_problem(AnalysisProblem::unary_operator_overload_ambiguous(
                    &operator_def,
                    &arg_type,
                ))?;
            }
            let candidate = &filtered[0];
            let arg_expr = candidate.wrap_implicit_cast(input);
            Ok(ResolvedUnaryOperator::new(
                Some(candidate.operator.clone()),
                arg_expr,
            ))
        } else {
            Ok(ResolvedUnaryOperator::new(None, input))
        }
    }
}
