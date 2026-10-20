use crate::sem::{
    AnalysisError, AnalysisProblem, InputTypeConstraint, MinMaxExpr, SemScalarExpr, TypeReference,
    scalar_expr::SemScalarExprNode,
};
#[cfg(test)]
use crate::{
    sem::{ParseContext, ScalarAnalysisContext},
    syn::{Opt, ScanToken},
};

use std::collections::{HashMap, HashSet};

#[cfg(test)]
use ::sql_fun_core::IVec;

pub(crate) struct CaseExprAnalysisContext {
    catalog_casts: bool,
    case_arg: Option<SemScalarExpr>,
    nullabilities: Vec<Option<bool>>,
    when_exprs: Vec<CaseWhen>,
    result_types: HashSet<TypeReference>,
    default_expr: Option<SemScalarExpr>,
}

impl CaseExprAnalysisContext {
    pub(crate) fn new() -> Self {
        Self {
            catalog_casts: true,
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

    #[test]
    pub fn test_case_expr_analysis_context_new() {
        let _ = super::CaseExprAnalysisContext::new();
    }
}

impl CaseExprAnalysisContext {
    fn collect_result<TContext: crate::sem::ParseContext>(
        &mut self,
        context: &mut TContext,
        result: &SemScalarExpr,
    ) -> Result<(), AnalysisError> {
        if let Some(result_type) = result.get_type() {
            if !self.catalog_casts || result_type.is_dynamic() {
                self.result_types.insert(result_type);
            } else if let Some(result_type) = context.canonicalize_type_ref(&result_type) {
                self.result_types.insert(result_type);
            } else {
                context
                    .report_analysis_problem(AnalysisProblem::expression_type_not_known(result))?;
            }
        } else {
            context
                .report_analysis_problem(AnalysisProblem::case_when_type_unknown(result.clone()))?;
        }
        self.nullabilities.push(result.is_not_null());
        Ok(())
    }

    #[cfg(test)]
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
                &ScalarAnalysisContext::default(),
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
    use crate::{
        syn::{Node, NodeInner, NodeList},
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

    use ::sql_fun_core::IVec;
    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_case_arg_none(test_context: TestParseContext) -> TestResult {
        let mut acontext = super::CaseExprAnalysisContext::new();

        let builder = SynBuilder::new();
        let case_expr = builder.case_expr(Node::none(), NodeList::default(), Node::none());
        let tokens = IVec::default();
        let _new_context = acontext.analyze_case_arg(test_context, &case_expr, &tokens)?;
        assert!(acontext.case_arg.is_none());
        Ok(())
    }

    #[rstest::rstest]
    fn test_analyze_case_arg_some(test_context: TestParseContext) -> TestResult {
        let mut acontext = super::CaseExprAnalysisContext::new();

        let builder = SynBuilder::new();
        let arg_expr = NodeInner::AConst(builder.const_int4(10));
        let case_expr = builder.case_expr(arg_expr.into(), NodeList::default(), Node::none());
        let tokens = IVec::default();
        let _new_context = acontext.analyze_case_arg(test_context, &case_expr, &tokens)?;
        assert!(acontext.case_arg.is_some());
        Ok(())
    }
}

impl CaseExprAnalysisContext {
    #[cfg(test)]
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
            &ScalarAnalysisContext::default(),
            when_value,
            tokens,
        )?;
        context = new_context;
        let (result, new_context) = SemScalarExpr::analyze_scaler_expr(
            context,
            &ScalarAnalysisContext::default(),
            result,
            tokens,
        )?;
        context = new_context;
        Ok((CaseWhen::new(when_value, result), context))
    }
}

#[cfg(test)]
mod test_case_expr_analysis_context_analyze_case_when {
    use crate::{
        sem::{ScalarConstExpr, SemScalarExpr},
        syn::NodeInner,
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

    use ::sql_fun_core::IVec;
    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_case_when(test_context: TestParseContext) -> TestResult {
        let acontext = super::CaseExprAnalysisContext::new();

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

impl CaseExprAnalysisContext {
    #[cfg(test)]
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

            self.collect_result(&mut context, &when_expr.result)?;
            self.when_exprs.push(when_expr);
        }
        Ok(context)
    }
}

#[cfg(test)]
mod test_case_expr_analysis_context_analyze_case_when_list {
    use crate::{
        sem::PgBuiltInType,
        syn::NodeInner,
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

    use ::sql_fun_core::IVec;
    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_case_when_list(mut test_context: TestParseContext) -> TestResult {
        test_context.setup_type(PgBuiltInType::int4());
        test_context.setup_type(PgBuiltInType::text());

        let mut acontext = super::CaseExprAnalysisContext::new();

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

impl CaseExprAnalysisContext {
    #[cfg(test)]
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
            &ScalarAnalysisContext::default(),
            def_result,
            tokens,
        )?;
        context = new_context;

        self.collect_result(&mut context, &default_expr)?;
        self.default_expr = Some(default_expr);
        Ok(context)
    }
}

#[cfg(test)]
mod test_case_expr_analysis_context_analyze_default_expr {
    use crate::{
        sem::PgBuiltInType,
        syn::{Node, NodeInner, NodeList},
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

    use ::sql_fun_core::IVec;
    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_default_expr_none(test_context: TestParseContext) -> TestResult {
        let mut acontext = super::CaseExprAnalysisContext::new();

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
        let mut acontext = super::CaseExprAnalysisContext::new();
        let builder = SynBuilder::new();
        let expr = NodeInner::AConst(builder.const_int4(10));
        let case_expr = builder.case_expr(Node::none(), NodeList::default(), expr.into());
        let tokens = IVec::default();
        let _new_context = acontext.analyze_default_expr(test_context, case_expr, &tokens)?;

        assert!(acontext.default_expr.is_some());
        Ok(())
    }
}

impl CaseExprAnalysisContext {
    fn cast_args<TParseContext>(
        &mut self,
        context: &mut TParseContext,
        result_type: &TypeReference,
    ) -> Result<Option<TypeReference>, AnalysisError>
    where
        TParseContext: crate::sem::ParseContext,
    {
        for when_expr in &mut self.when_exprs {
            let Some(_ty) = when_expr.result.get_type() else {
                continue;
            };
            SemScalarExpr::implicit_cast_resolved(context, result_type, &mut when_expr.result)?;
        }
        if let Some(default) = self.default_expr.as_mut() {
            SemScalarExpr::implicit_cast_resolved(context, result_type, default)?;
        }

        Ok(Some(result_type.clone()))
    }
}

impl CaseExprAnalysisContext {
    fn type_fixup<TParseContext>(
        &mut self,
        context: &mut TParseContext,
    ) -> Result<Option<TypeReference>, AnalysisError>
    where
        TParseContext: crate::sem::ParseContext,
    {
        if self.result_types.len() == 1 {
            return Ok(self.result_types.iter().next().cloned());
        }

        let cast_candidates = self.enumerate_cast_candidates(context);
        if cast_candidates.is_empty() {
            context.report_analysis_problem(AnalysisProblem::incompatible_case_values(
                self.result_types.clone(),
            ))?;
            Ok(None)
        } else if cast_candidates.len() == 1 {
            let Some((result_type, _casts)) = cast_candidates.into_iter().next() else {
                AnalysisError::raise_unexpected_none("cast candidates")?
            };
            Ok(self.cast_args(context, &result_type)?)
        } else {
            let mut implicit_castable = Vec::new();
            for (ty, _casts) in cast_candidates {
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
    use crate::sem::scalar_expr::case_expr::SemScalarExprNode;
    use crate::{
        sem::{CastContext, CastDefinition, PgBuiltInType, SemScalarExpr},
        syn::NodeInner,
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

    use ::sql_fun_core::IVec;
    use ::testresult::TestResult;

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

        let mut acontext = super::CaseExprAnalysisContext::new();

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

        let mut acontext = super::CaseExprAnalysisContext::new();
        acontext.result_types.insert(int4);
        acontext.result_types.insert(text);

        let result = acontext.type_fixup(&mut test_context)?;
        assert!(result.is_none());
        assert_eq!(1, test_context.reported_problem_count());
        Ok(())
    }
}

impl CaseExprAnalysisContext {
    fn enumerate_cast_candidates<TParseContext>(
        &mut self,
        context: &mut TParseContext,
    ) -> HashMap<TypeReference, Vec<crate::sem::CastDefinition>>
    where
        TParseContext: crate::sem::ParseContext,
    {
        let mut cast_candidates = HashMap::new();
        for ty1 in self.result_types.clone() {
            for ty2 in self.result_types.clone() {
                if ty1 == ty2 {
                    continue;
                }
                let Some(cast) = context.get_implicit_cast(&ty1, &ty2) else {
                    continue;
                };
                cast_candidates
                    .entry(ty2)
                    .and_modify(|v: &mut Vec<_>| v.push(cast.clone()))
                    .or_insert(vec![cast.clone()]);
            }
        }
        cast_candidates
    }
}

#[cfg(test)]
mod test_case_expr_analysis_context_enumerate_cast_candidates {
    use crate::{
        sem::{CastContext, CastDefinition, PgBuiltInType},
        test_helpers::{TestParseContext, test_context},
    };

    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_enumerate_cast_candidates_single(mut test_context: TestParseContext) -> TestResult {
        let int4 = PgBuiltInType::int4();
        let text = PgBuiltInType::text();
        test_context.setup_type(int4.clone());
        test_context.setup_type(text.clone());
        test_context.set_get_implicit_cast_result(
            &int4,
            &text,
            Some(CastDefinition::new(CastContext::Implicit)),
        );

        let mut acontext = super::CaseExprAnalysisContext::new();
        acontext.result_types.insert(int4);
        acontext.result_types.insert(text.clone());

        let cast_candidates = acontext.enumerate_cast_candidates(&mut test_context);
        let Some(casts) = cast_candidates.get(&text) else {
            panic!("expected cast candidates for text");
        };
        assert_eq!(1, casts.len());
        Ok(())
    }
}

impl CaseExprAnalysisContext {
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

    #[test]
    fn test_is_not_null_any_false() {
        let mut acontext = super::CaseExprAnalysisContext::new();

        acontext.nullabilities = vec![Some(true), Some(false)];
        assert_eq!(Some(false), acontext.is_not_null());
    }

    #[test]
    fn test_is_not_null_all_some() {
        let mut acontext = super::CaseExprAnalysisContext::new();

        acontext.nullabilities = vec![Some(true), Some(true)];
        assert_eq!(Some(true), acontext.is_not_null());
    }

    #[test]
    fn test_is_not_null_with_unknown() {
        let mut acontext = super::CaseExprAnalysisContext::new();

        acontext.nullabilities = vec![Some(true), None];
        assert_eq!(None, acontext.is_not_null());
    }
}

impl CaseExprAnalysisContext {
    fn into_case_expr<TParseContext>(
        mut self,
        context: &mut TParseContext,
    ) -> Result<CaseExpr, AnalysisError>
    where
        TParseContext: crate::sem::ParseContext,
    {
        let case_expr_type = self.type_fixup(context)?;
        let is_not_null = self.is_not_null();
        Ok(CaseExpr::new(
            Box::new(self.case_arg),
            self.when_exprs,
            Box::new(self.default_expr),
            case_expr_type,
            is_not_null,
        ))
    }
}

/// An ordered CASE arm with its condition, result, and simple-CASE comparison.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CaseWhen {
    when_value: SemScalarExpr,
    result: SemScalarExpr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    comparison: Option<SemScalarExpr>,
}

impl CaseWhen {
    /// Construct an arm before simple-CASE equality binding and type inference.
    pub const fn new(when_value: SemScalarExpr, result: SemScalarExpr) -> Self {
        Self {
            when_value,
            result,
            comparison: None,
        }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    types: Option<InputTypeConstraint>,
    #[serde(skip)]
    node: Option<usize>,
    #[serde(skip)]
    condition_nodes: Vec<usize>,
}

impl CaseExpr {
    /// create a instance
    #[must_use]
    pub const fn new(
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
            types: None,
            node: None,
            condition_nodes: Vec::new(),
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

impl CaseExprAnalysisContext {
    pub(crate) fn set_argument(&mut self, argument: &Option<SemScalarExpr>) {
        self.case_arg = argument.clone();
    }
    pub(crate) fn push_arm<C: crate::sem::ParseContext>(
        &mut self,
        context: &mut C,
        condition: &SemScalarExpr,
        result: &SemScalarExpr,
    ) -> Result<(), AnalysisError> {
        self.collect_result(context, result)?;
        self.when_exprs
            .push(CaseWhen::new(condition.clone(), result.clone()));
        Ok(())
    }
    pub(crate) fn set_default<C: crate::sem::ParseContext>(
        &mut self,
        context: &mut C,
        value: &Option<SemScalarExpr>,
    ) -> Result<(), AnalysisError> {
        if let Some(value) = value {
            self.collect_result(context, value)?;
        }
        self.default_expr = value.clone();
        Ok(())
    }
    pub(crate) fn finish<C: crate::sem::ParseContext>(
        self,
        context: &mut C,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut value = if !self.catalog_casts {
            let result_type = if self.result_types.len() == 1 {
                self.result_types.iter().next().cloned()
            } else {
                None
            };
            let nullable = self.is_not_null();
            CaseExpr::new(
                Box::new(self.case_arg),
                self.when_exprs,
                Box::new(self.default_expr),
                result_type,
                nullable,
            )
        } else {
            self.into_case_expr(context)?
        };
        value.bind_comparisons(context)?;
        Ok(SemScalarExpr::case(&value))
    }
}

impl CaseExprAnalysisContext {
    pub(crate) fn without_catalog_casts() -> Self {
        Self {
            catalog_casts: false,
            ..Self::new()
        }
    }
}

impl CaseExpr {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        crate::sem::traversal::optional(&mut self.arg_expr, visitor)?;
        for arm in &mut self.when_exprs {
            arm.walk(visitor)?;
        }
        crate::sem::traversal::optional(&mut self.default_expr, visitor)?;
        Ok(())
    }
}

impl CaseWhen {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.when_value.walk(visitor)?;
        self.result.walk(visitor)?;
        if let Some(comparison) = &mut self.comparison {
            comparison.walk(visitor)?;
        }
        Ok(())
    }
}

impl CaseExpr {
    pub(crate) fn collect_constraints(
        &mut self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<(), AnalysisError> {
        if self.types.is_none()
            && !self
                .arg_expr
                .as_ref()
                .as_ref()
                .is_some_and(|value| value.constraint_node().is_some())
            && !self
                .default_expr
                .as_ref()
                .as_ref()
                .is_some_and(|value| value.constraint_node().is_some())
            && !self.when_exprs.iter().any(CaseWhen::has_constraints)
        {
            return Ok(());
        }
        // SQLite's heterogeneous CASE results do not impose PostgreSQL cast or
        // boolean requirements. Preserve its existing unresolved result metadata.
        if context.dialect() == crate::ast::Dialect::sqlite() {
            return Ok(());
        }
        for arm in &self.when_exprs {
            let condition = arm.comparison.as_ref().unwrap_or(&arm.when_value);
            self.condition_nodes
                .push(parameters.case_condition(context, condition));
        }
        // PostgreSQL considers ELSE first for historical reasons. An absent
        // ELSE supplies an unknown NULL, not a parameter or an inferred value.
        let mut results = vec![
            self.default_expr
                .as_ref()
                .clone()
                .unwrap_or_else(crate::sem::ScalarConstExpr::null),
        ];
        for arm in &self.when_exprs {
            results.push(arm.result.clone());
        }
        let (node, types) = parameters.unify_case_types(context, &results)?;
        self.node = Some(node);
        self.case_expr_type = types.single_type();
        self.types = Some(types);
        Ok(())
    }

    pub(crate) const fn node(&self) -> Option<usize> {
        self.node
    }

    pub(crate) fn apply_constraints(&mut self, constraints: &crate::sem::ParameterConstraints) {
        let Some(node) = self.node else {
            return;
        };
        let mut types = constraints.node_domain(node).clone();
        if self
            .condition_nodes
            .iter()
            .any(|node| constraints.node_domain(*node).admissible_types() == Some(&[]))
        {
            types = InputTypeConstraint::types([]);
        }
        self.case_expr_type = types.single_type();
        self.types = Some(types);
        if let Some(target) = &self.case_expr_type {
            for arm in &mut self.when_exprs {
                Self::coerce(&mut arm.result, target);
            }
            if let Some(otherwise) = self.default_expr.as_mut() {
                Self::coerce(otherwise, target);
            }
        }
        for (arm, node) in self.when_exprs.iter_mut().zip(&self.condition_nodes) {
            if arm.comparison.is_none()
                && constraints.node_domain(*node).admissible_types() != Some(&[])
            {
                Self::coerce(&mut arm.when_value, &crate::sem::PgBuiltInType::bool());
            }
        }
        self.is_not_null = self.result_nullability();
    }

    fn coerce(value: &mut SemScalarExpr, target: &TypeReference) {
        if value.get_type().as_ref() != Some(target)
            || MinMaxExpr::input_type(value) == Some(MinMaxExpr::unknown())
        {
            *value = SemScalarExpr::wrap_implicit_cast(target, value);
        }
    }

    fn result_nullability(&self) -> Option<bool> {
        let Some(otherwise) = self.default_expr.as_ref() else {
            return Some(false);
        };
        if otherwise.is_not_null() == Some(false)
            || self
                .when_exprs
                .iter()
                .any(|arm| arm.result.is_not_null() == Some(false))
        {
            Some(false)
        } else if otherwise.is_not_null() == Some(true)
            && self
                .when_exprs
                .iter()
                .all(|arm| arm.result.is_not_null() == Some(true))
        {
            Some(true)
        } else {
            None
        }
    }

    pub(super) fn canonicalize_grouping_key(&mut self) {
        self.node = None;
        self.condition_nodes.clear();
    }

    /// Optional selector of a simple CASE; searched CASE has no selector.
    pub fn argument(&self) -> Option<&SemScalarExpr> {
        self.arg_expr.as_ref().as_ref()
    }

    /// Ordered WHEN conditions and THEN results, retaining inferred coercions.
    pub fn arms(&self) -> &[CaseWhen] {
        &self.when_exprs
    }

    /// Explicit ELSE expression. Absence retains SQL's implicit NULL result.
    pub fn otherwise(&self) -> Option<&SemScalarExpr> {
        self.default_expr.as_ref().as_ref()
    }
}

impl CaseWhen {
    const fn has_constraints(&self) -> bool {
        self.when_value.constraint_node().is_some() || self.result.constraint_node().is_some()
    }

    /// Analyzed WHEN operand (or predicate in a searched CASE).
    pub const fn condition(&self) -> &SemScalarExpr {
        &self.when_value
    }

    /// Analyzed THEN result, including its implicit coercion when determined.
    pub const fn result(&self) -> &SemScalarExpr {
        &self.result
    }

    /// Simple-CASE equality expression and its operator binding, if present.
    pub const fn comparison(&self) -> Option<&SemScalarExpr> {
        self.comparison.as_ref()
    }
}

impl CaseExpr {
    fn bind_comparisons(
        &mut self,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<(), AnalysisError> {
        if context.dialect() != crate::ast::Dialect::postgres() {
            return Ok(());
        }
        let Some(argument) = self.arg_expr.as_ref() else {
            return Ok(());
        };
        for arm in &mut self.when_exprs {
            let name = crate::sem::OperatorName::from_parts(None, "=");
            let binding = crate::sem::ResolvedOperator::resolve(context, &name, true, true);
            let (mut comparison, _) = crate::sem::ArithExpr::build_op(
                context,
                &name,
                Some(argument.clone()),
                Some(arm.when_value.clone()),
            )?;
            comparison.set_operator_binding(binding);
            arm.comparison = Some(comparison);
        }
        Ok(())
    }

    pub(crate) fn defer_untyped(
        context: &mut impl crate::sem::ParseContext,
        argument: &Option<SemScalarExpr>,
        arms: &[(SemScalarExpr, SemScalarExpr)],
        otherwise: &Option<SemScalarExpr>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        let unknown = |value: &SemScalarExpr| value.get_type().is_none();
        if !argument.as_ref().is_some_and(unknown)
            && !otherwise.as_ref().is_some_and(unknown)
            && !arms
                .iter()
                .any(|(condition, result)| unknown(condition) || unknown(result))
        {
            return Ok(None);
        }
        let mut when_exprs = Vec::new();
        for (condition, result) in arms {
            when_exprs.push(CaseWhen::new(condition.clone(), result.clone()));
        }
        let mut value = Self::new(
            Box::new(argument.clone()),
            when_exprs,
            Box::new(otherwise.clone()),
            None,
            None,
        );
        value.bind_comparisons(context)?;
        value.types = Some(InputTypeConstraint::any());
        value.is_not_null = value.result_nullability();
        Ok(Some(SemScalarExpr::case(&value)))
    }
}
