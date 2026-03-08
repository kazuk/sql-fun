use std::collections::{HashMap, HashSet};

use sql_fun_core::IVec;

use crate::{
    sem::{
        AnalysisError, AnalysisProblem, FromClause, ParseContext, SemScalarExpr, TypeReference,
        WithClause,
        scalar_expr::{AnalyzeScalarExpr, SemScalarExprNode},
    },
    syn::{ListOpt as _, Opt, ScanToken},
};

pub struct CaseExprAnalysisContext<'a> {
    with_clause: &'a WithClause,
    from_clause: &'a FromClause,
    case_arg: Option<SemScalarExpr>,
    nullabilities: Vec<Option<bool>>,
    when_exprs: Vec<CaseWhen>,
    result_types: HashSet<TypeReference>,
    default_expr: Option<SemScalarExpr>,
}

impl<'a> CaseExprAnalysisContext<'a> {
    fn new(with_clause: &'a WithClause, from_clause: &'a FromClause) -> Self {
        Self {
            with_clause,
            from_clause,
            case_arg: None,
            nullabilities: Default::default(),
            when_exprs: Default::default(),
            result_types: Default::default(),
            default_expr: None,
        }
    }
}

#[cfg(test)]
mod test_case_expr_analysis_context_new {
    use crate::sem::{FromClause, WithClause};

    #[test]
    pub fn test_case_expr_analysis_context_new() {
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let _ = super::CaseExprAnalysisContext::new(&with_clause, &from_clause);
    }
}

impl CaseExprAnalysisContext<'_> {
    fn analyze_case_arg<TParseContext>(
        &mut self,
        context: TParseContext,
        syn: &crate::syn::CaseExpr,
        tokens: &IVec<ScanToken>,
    ) -> Result<TParseContext, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        if let Some(arg) = syn.get_arg().as_inner() {
            let (arg_expr, new_context) = SemScalarExpr::analyze_scaler_expr(
                context,
                self.with_clause,
                self.from_clause,
                arg,
                tokens,
            )?;
            self.case_arg = Some(arg_expr);
            Ok(new_context)
        } else {
            Ok(context)
        }
    }
}

#[cfg(test)]
mod test_case_expr_analysis_context_analyze_case_arg {
    use sql_fun_core::IVec;
    use testresult::TestResult;

    use crate::{
        sem::{FromClause, WithClause},
        syn::{Node, NodeInner, NodeList},
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

    #[rstest::rstest]
    fn test_analyze_case_arg_none(test_context: TestParseContext) -> TestResult {
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let mut acontext = super::CaseExprAnalysisContext::new(&with_clause, &from_clause);

        let builder = SynBuilder::new();
        let case_expr = builder.case_expr(Node::none(), NodeList::default(), Node::none());
        let tokens = IVec::default();
        let _new_context = acontext.analyze_case_arg(test_context, &case_expr, &tokens)?;
        assert!(acontext.case_arg.is_none());
        Ok(())
    }

    #[rstest::rstest]
    fn test_analyze_case_arg_some(test_context: TestParseContext) -> TestResult {
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let mut acontext = super::CaseExprAnalysisContext::new(&with_clause, &from_clause);

        let builder = SynBuilder::new();
        let arg_expr = NodeInner::AConst(builder.const_int4(10));
        let case_expr = builder.case_expr(arg_expr.into(), NodeList::default(), Node::none());
        let tokens = IVec::default();
        let _new_context = acontext.analyze_case_arg(test_context, &case_expr, &tokens)?;
        assert!(acontext.case_arg.is_some());
        Ok(())
    }
}

impl CaseExprAnalysisContext<'_> {
    fn analyze_case_when<TParseContext>(
        &self,
        mut context: TParseContext,
        syn: crate::syn::CaseWhen,
        tokens: &IVec<ScanToken>,
    ) -> Result<(CaseWhen, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(when_value) = syn.get_expr().as_inner() else {
            AnalysisError::raise_unexpected_none("casewhen.expr")?
        };
        let Some(result) = syn.get_result().as_inner() else {
            AnalysisError::raise_unexpected_none("casewhen.result")?
        };
        let (when_value, new_context) = SemScalarExpr::analyze_scaler_expr(
            context,
            self.with_clause,
            self.from_clause,
            when_value,
            tokens,
        )?;
        context = new_context;
        let (result, new_context) = SemScalarExpr::analyze_scaler_expr(
            context,
            self.with_clause,
            self.from_clause,
            result,
            tokens,
        )?;
        context = new_context;
        Ok((CaseWhen::new(when_value, result), context))
    }
}

#[cfg(test)]
mod test_case_expr_analysis_context_analyze_case_when {
    use sql_fun_core::IVec;
    use testresult::TestResult;

    use crate::{
        sem::{FromClause, ScalarConstExpr, SemScalarExpr, WithClause},
        syn::NodeInner,
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

    #[rstest::rstest]
    fn test_analyze_case_when(test_context: TestParseContext) -> TestResult {
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let acontext = super::CaseExprAnalysisContext::new(&with_clause, &from_clause);

        let builder = SynBuilder::new();
        let expr = NodeInner::AConst(builder.const_int4(10));
        let result = NodeInner::AConst(builder.const_int4(20));
        let case_when = builder.case_when(expr.into(), result.into());
        let tokens = IVec::default();
        let (sem_case_when, _) = acontext.analyze_case_when(test_context, case_when, &tokens)?;
        let SemScalarExpr::Const(ScalarConstExpr::Integer(expr_value)) = sem_case_when.when_value
        else {
            panic!("unexpected when_value {sem_case_when:?}")
        };
        assert_eq!(10, expr_value);
        let SemScalarExpr::Const(ScalarConstExpr::Integer(result_value)) = sem_case_when.result
        else {
            panic!("unexpected result {sem_case_when:?}")
        };
        assert_eq!(20, result_value);
        Ok(())
    }
}

impl CaseExprAnalysisContext<'_> {
    fn analyze_case_when_list<TParseContext>(
        &mut self,
        mut context: TParseContext,
        args: Vec<crate::syn::CaseWhen>,
        tokens: &IVec<ScanToken>,
    ) -> Result<TParseContext, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        for arg in args {
            let (when_expr, new_context) = self.analyze_case_when(context, arg, tokens)?;
            context = new_context;

            if let Some(result_type) = when_expr.result.get_type() {
                if result_type.is_dynamic() {
                    self.result_types.insert(result_type);
                } else if let Some(result_type) = context.canonicalize_type_ref(&result_type) {
                    self.result_types.insert(result_type);
                } else {
                    context.report_problem(AnalysisProblem::expression_type_not_known(
                        &when_expr.result,
                    ))?;
                }
            } else {
                context.report_problem(AnalysisProblem::case_when_type_unknown(
                    when_expr.result.clone(),
                ))?;
            }

            self.nullabilities.push(when_expr.result.is_not_null());
            self.when_exprs.push(when_expr);
        }
        Ok(context)
    }
}

#[cfg(test)]
mod test_case_expr_analysis_context_analyze_case_when_list {
    use crate::{
        sem::{FromClause, PgBuiltInType, WithClause},
        syn::NodeInner,
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };
    use sql_fun_core::IVec;
    use testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_case_when_list(mut test_context: TestParseContext) -> TestResult {
        test_context.setup_type(PgBuiltInType::int4());
        test_context.setup_type(PgBuiltInType::text());

        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let mut acontext = super::CaseExprAnalysisContext::new(&with_clause, &from_clause);

        let builder = SynBuilder::new();

        let expr = NodeInner::AConst(builder.const_int4(10));
        let result = NodeInner::AConst(builder.const_int4(20));
        let case_when_int = builder.case_when(expr.into(), result.into());

        let expr = NodeInner::AConst(builder.const_sval("a"));
        let result = NodeInner::AConst(builder.const_sval("b"));
        let case_when_sval = builder.case_when(expr.into(), result.into());

        let when_values = vec![case_when_int, case_when_sval];
        let tokens = IVec::default();

        let _ = acontext.analyze_case_when_list(test_context, when_values, &tokens)?;
        assert_eq!(acontext.result_types.len(), 2);
        assert_eq!(acontext.nullabilities.len(), 2);
        assert_eq!(acontext.when_exprs.len(), 2);
        Ok(())
    }
}

impl CaseExprAnalysisContext<'_> {
    fn analyze_default_expr<TParseContext>(
        &mut self,
        mut context: TParseContext,
        syn: crate::syn::CaseExpr,
        tokens: &IVec<ScanToken>,
    ) -> Result<TParseContext, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(def_result) = syn.get_defresult().as_inner() else {
            return Ok(context);
        };

        let (default_expr, new_context) = SemScalarExpr::analyze_scaler_expr(
            context,
            self.with_clause,
            self.from_clause,
            def_result,
            tokens,
        )?;
        context = new_context;

        if let Some(default_result_type) = default_expr.get_type() {
            if default_result_type.is_dynamic() {
                self.result_types.insert(default_result_type);
            } else if let Some(default_result_type) =
                context.canonicalize_type_ref(&default_result_type)
            {
                self.result_types.insert(default_result_type);
            } else {
                context
                    .report_problem(AnalysisProblem::expression_type_not_known(&default_expr))?;
            }
        } else {
            context.report_problem(AnalysisProblem::case_when_type_unknown(
                default_expr.clone(),
            ))?;
        }
        self.nullabilities.push(default_expr.is_not_null());
        self.default_expr = Some(default_expr);
        Ok(context)
    }
}

#[cfg(test)]
mod test_case_expr_analysis_context_analyze_default_expr {
    use crate::{
        sem::{FromClause, PgBuiltInType, WithClause},
        syn::{Node, NodeInner, NodeList},
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };
    use sql_fun_core::IVec;
    use testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_default_expr_none(test_context: TestParseContext) -> TestResult {
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let mut acontext = super::CaseExprAnalysisContext::new(&with_clause, &from_clause);

        let builder = SynBuilder::new();
        let case_expr = builder.case_expr(Node::none(), NodeList::default(), Node::none());
        let tokens = IVec::default();
        let _new_context = acontext.analyze_default_expr(test_context, case_expr, &tokens)?;
        assert!(acontext.default_expr.is_none());
        Ok(())
    }

    #[rstest::rstest]
    fn test_analyze_default_expr_some(mut test_context: TestParseContext) -> TestResult {
        test_context.setup_type(PgBuiltInType::int4());
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let mut acontext = super::CaseExprAnalysisContext::new(&with_clause, &from_clause);
        let builder = SynBuilder::new();
        let expr = NodeInner::AConst(builder.const_int4(10));
        let case_expr = builder.case_expr(Node::none(), NodeList::default(), expr.into());
        let tokens = IVec::default();
        let _new_context = acontext.analyze_default_expr(test_context, case_expr, &tokens)?;

        assert!(acontext.default_expr.is_some());
        Ok(())
    }
}

impl CaseExprAnalysisContext<'_> {
    fn cast_args<TParseContext>(
        &mut self,
        context: &mut TParseContext,
        result_type: &TypeReference,
    ) -> Result<Option<TypeReference>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        for when_expr in &mut self.when_exprs {
            let Some(_ty) = when_expr.result.get_type() else {
                continue;
            };
            SemScalarExpr::implicit_cast_if_require(context, result_type, &mut when_expr.result)?;
        }
        if let Some(default) = self.default_expr.as_mut() {
            SemScalarExpr::implicit_cast_if_require(context, result_type, default)?;
        }

        Ok(Some(result_type.clone()))
    }
}

impl CaseExprAnalysisContext<'_> {
    fn type_fixup<TParseContext>(
        &mut self,
        context: &mut TParseContext,
    ) -> Result<Option<TypeReference>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        if self.result_types.len() == 1 {
            return Ok(self.result_types.iter().next().cloned());
        }

        let cast_candiates = self.enumerate_cast_candiates(context);
        if cast_candiates.is_empty() {
            context.report_problem(AnalysisProblem::incompatible_case_values(
                self.result_types.clone(),
            ))?;
            Ok(None)
        } else if cast_candiates.len() == 1 {
            let Some((result_type, _casts)) = cast_candiates.into_iter().next() else {
                AnalysisError::raise_unexpected_none("cast candiates")?
            };
            Ok(self.cast_args(context, &result_type)?)
        } else {
            let mut implicit_castable = Vec::new();
            for (ty, _casts) in cast_candiates {
                if self
                    .result_types
                    .iter()
                    .all(|t| context.get_implicit_cast(t, &ty).is_some())
                {
                    implicit_castable.push(ty);
                }
            }

            if implicit_castable.len() == 1 {
                Ok(implicit_castable.into_iter().next())
            } else {
                todo!("{implicit_castable:#?}");
            }
        }
    }
}

#[cfg(test)]
mod test_case_expr_analysis_context_type_fixup {
    use sql_fun_core::IVec;
    use testresult::TestResult;

    use super::SemScalarExprNode;
    use crate::{
        sem::{CastContext, CastDefinition, FromClause, PgBuiltInType, SemScalarExpr, WithClause},
        syn::NodeInner,
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

    #[rstest::rstest]
    fn test_type_fixup_with_implicit_cast(mut test_context: TestParseContext) -> TestResult {
        test_context.setup_type(PgBuiltInType::int4());
        test_context.setup_type(PgBuiltInType::text());
        let int4 = PgBuiltInType::int4();
        let text = PgBuiltInType::text();
        test_context.set_get_implicit_cast_result(
            &int4,
            &text,
            Some(CastDefinition::new(CastContext::Implicit)),
        );

        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let mut acontext = super::CaseExprAnalysisContext::new(&with_clause, &from_clause);

        let builder = SynBuilder::new();
        let expr = NodeInner::AConst(builder.const_int4(1));
        let result = NodeInner::AConst(builder.const_int4(10));
        let when_int = builder.case_when(expr.into(), result.into());

        let expr = NodeInner::AConst(builder.const_sval("a"));
        let result = NodeInner::AConst(builder.const_sval("b"));
        let when_text = builder.case_when(expr.into(), result.into());

        let tokens = IVec::default();
        let mut context =
            acontext.analyze_case_when_list(test_context, vec![when_int, when_text], &tokens)?;
        let result_type = acontext.type_fixup(&mut context)?;
        assert_eq!(Some(text.clone()), result_type);

        match &acontext.when_exprs[0].result {
            SemScalarExpr::ImplicitCast(cast_expr) => {
                assert_eq!(Some(text.clone()), cast_expr.get_type());
            }
            other => panic!("expected implicit cast, got {other:?}"),
        }

        match &acontext.when_exprs[1].result {
            SemScalarExpr::Const(_) => {}
            other => panic!("expected const, got {other:?}"),
        }
        Ok(())
    }

    #[rstest::rstest]
    fn test_type_fixup_no_cast_candidates(mut test_context: TestParseContext) -> TestResult {
        let int4 = PgBuiltInType::int4();
        let text = PgBuiltInType::text();
        test_context.setup_type(int4.clone());
        test_context.setup_type(text.clone());

        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let mut acontext = super::CaseExprAnalysisContext::new(&with_clause, &from_clause);
        acontext.result_types.insert(int4);
        acontext.result_types.insert(text);

        let result = acontext.type_fixup(&mut test_context)?;
        assert!(result.is_none());
        assert_eq!(1, test_context.reported_problem_count());
        Ok(())
    }
}

impl CaseExprAnalysisContext<'_> {
    fn enumerate_cast_candiates<TParseContext>(
        &mut self,
        context: &mut TParseContext,
    ) -> HashMap<TypeReference, Vec<crate::sem::CastDefinition>>
    where
        TParseContext: ParseContext,
    {
        let mut cast_candiates = HashMap::new();
        for ty1 in self.result_types.clone() {
            for ty2 in self.result_types.clone() {
                if ty1 == ty2 {
                    continue;
                }
                let Some(cast) = context.get_implicit_cast(&ty1, &ty2) else {
                    continue;
                };
                cast_candiates
                    .entry(ty2)
                    .and_modify(|v: &mut Vec<_>| v.push(cast.clone()))
                    .or_insert(vec![cast.clone()]);
            }
        }
        cast_candiates
    }
}

#[cfg(test)]
mod test_case_expr_analysis_context_enumerate_cast_candiates {
    use testresult::TestResult;

    use crate::{
        sem::{CastContext, CastDefinition, FromClause, PgBuiltInType, WithClause},
        test_helpers::{TestParseContext, test_context},
    };

    #[rstest::rstest]
    fn test_enumerate_cast_candiates_single(mut test_context: TestParseContext) -> TestResult {
        let int4 = PgBuiltInType::int4();
        let text = PgBuiltInType::text();
        test_context.setup_type(int4.clone());
        test_context.setup_type(text.clone());
        test_context.set_get_implicit_cast_result(
            &int4,
            &text,
            Some(CastDefinition::new(CastContext::Implicit)),
        );

        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let mut acontext = super::CaseExprAnalysisContext::new(&with_clause, &from_clause);
        acontext.result_types.insert(int4);
        acontext.result_types.insert(text.clone());

        let cast_candidates = acontext.enumerate_cast_candiates(&mut test_context);
        let Some(casts) = cast_candidates.get(&text) else {
            panic!("expected cast candidates for text");
        };
        assert_eq!(1, casts.len());
        Ok(())
    }
}

impl CaseExprAnalysisContext<'_> {
    fn is_not_null(&self) -> Option<bool> {
        if self.nullabilities.iter().any(|n| n == &Some(false)) {
            Some(false)
        } else if self.nullabilities.iter().all(std::option::Option::is_some) {
            Some(true)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test_case_expr_analysis_context_is_not_null {
    use crate::sem::{FromClause, WithClause};

    #[test]
    fn test_is_not_null_any_false() {
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let mut acontext = super::CaseExprAnalysisContext::new(&with_clause, &from_clause);

        acontext.nullabilities = vec![Some(true), Some(false)];
        assert_eq!(Some(false), acontext.is_not_null());
    }

    #[test]
    fn test_is_not_null_all_some() {
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let mut acontext = super::CaseExprAnalysisContext::new(&with_clause, &from_clause);

        acontext.nullabilities = vec![Some(true), Some(true)];
        assert_eq!(Some(true), acontext.is_not_null());
    }

    #[test]
    fn test_is_not_null_with_unknown() {
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let mut acontext = super::CaseExprAnalysisContext::new(&with_clause, &from_clause);

        acontext.nullabilities = vec![Some(true), None];
        assert_eq!(None, acontext.is_not_null());
    }
}

impl CaseExprAnalysisContext<'_> {
    fn into_case_expr<TParseContext>(
        mut self,
        context: &mut TParseContext,
    ) -> Result<CaseExpr, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let case_expr_type = self.type_fixup(context)?;
        let is_not_null = self.is_not_null();
        Ok(CaseExpr {
            arg_expr: Box::new(self.case_arg),
            default_expr: Box::new(self.default_expr),
            when_exprs: self.when_exprs,
            case_expr_type,
            is_not_null,
        })
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CaseWhen {
    when_value: SemScalarExpr,
    result: SemScalarExpr,
}

impl CaseWhen {
    pub fn new(when_value: SemScalarExpr, result: SemScalarExpr) -> Self {
        Self { when_value, result }
    }
}

/// case expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CaseExpr {
    arg_expr: Box<Option<SemScalarExpr>>,
    when_exprs: Vec<CaseWhen>,
    default_expr: Box<Option<SemScalarExpr>>,
    case_expr_type: Option<TypeReference>,
    is_not_null: Option<bool>,
}

impl CaseExpr {
    /// create a instance
    #[must_use]
    pub fn new(
        arg_expr: Box<Option<SemScalarExpr>>,
        when_exprs: Vec<CaseWhen>,
        default_expr: Box<Option<SemScalarExpr>>,
        case_expr_type: Option<TypeReference>,
        is_not_null: Option<bool>,
    ) -> Self {
        Self {
            arg_expr,
            when_exprs,
            default_expr,
            case_expr_type,
            is_not_null,
        }
    }
}

impl SemScalarExprNode for CaseExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.case_expr_type.clone()
    }

    fn is_not_null(&self) -> Option<bool> {
        self.is_not_null
    }
}

impl<TParseContext> AnalyzeScalarExpr<TParseContext, crate::syn::CaseExpr> for CaseExpr
where
    TParseContext: ParseContext,
{
    fn analyze_scalar_expr(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        syn: crate::syn::CaseExpr,
        tokens: &IVec<ScanToken>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        let mut analisys_context = CaseExprAnalysisContext::new(with_clause, from_clause);

        context = analisys_context.analyze_case_arg(context, &syn, tokens)?;
        let Some(args) = syn.get_args().map(|v| v.as_case_when()) else {
            AnalysisError::raise_unexpected_none("caseexpr.args")?
        };
        context = analisys_context.analyze_case_when_list(context, args, tokens)?;
        context = analisys_context.analyze_default_expr(context, syn, tokens)?;
        let case = analisys_context.into_case_expr(&mut context)?;
        Ok((SemScalarExpr::Case(case), context))
    }
}
