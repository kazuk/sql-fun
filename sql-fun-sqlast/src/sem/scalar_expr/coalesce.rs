use crate::{
    ast::Dialect,
    context::ParseContext,
    names::ColumnName,
    sem::{
        AnalysisError, AnalysisProblem, InputTypeConstraint, MinMaxExpr, ParameterConstraints,
        SemScalarExpr, TypeReference,
        parameter_resolution::ParameterResolution,
        scalar_expr::SemScalarExprNode,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

/// COALESCE with ordered analyzed operands and its common result-type domain.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CoalesceExpr {
    arg_exprs: Vec<SemScalarExpr>,
    coalesce_type: Option<TypeReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    join_key_not_null: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    join_key_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    types: Option<InputTypeConstraint>,
    #[serde(skip)]
    node: Option<usize>,
}

impl SemScalarExprNode for CoalesceExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.coalesce_type.clone()
    }

    fn is_not_null(&self) -> Option<bool> {
        if let Some(not_null) = self.join_key_not_null {
            return Some(not_null);
        }
        // Preserve the existing conservative metadata; general nullability
        // analysis is separate from common-type constraint registration.
        Some(false)
    }
}

impl CoalesceExpr {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        arguments: &[SemScalarExpr],
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut value = Self {
            arg_exprs: arguments.to_vec(),
            coalesce_type: None,
            join_key_not_null: None,
            join_key_name: None,
            types: Some(InputTypeConstraint::any()),
            node: None,
        };
        let mut first_non_null = None;
        for argument in arguments {
            if let Some(first) = first_non_null {
                context.report_analysis_problem(AnalysisProblem::unreachable_coalesce_argument(
                    argument, first,
                ))?;
            } else if argument.is_not_null() == Some(true) {
                first_non_null = Some(argument);
            }
        }
        if context.dialect() == Dialect::sqlite() {
            if arguments.len() < 2 {
                context.report_analysis_problem(AnalysisProblem::parameter_constraint(
                    "SQLite COALESCE requires at least two arguments",
                ))?;
                value.types = Some(InputTypeConstraint::types([]));
            }
            // SQLite does not coerce all operands to a PostgreSQL common type.
            // Parameter-dependent and heterogeneous result typing stays explicit.
            return Ok(SemScalarExpr::coalesce(&value));
        }
        if arguments
            .iter()
            .all(|arg| MinMaxExpr::input_type(arg).is_some())
        {
            let inputs: Vec<_> = arguments
                .iter()
                .filter_map(MinMaxExpr::input_type)
                .collect();
            match MinMaxExpr::select_common_type(context, &inputs) {
                Ok(target) => {
                    value.coalesce_type = Some(target.clone());
                    value.types = Some(InputTypeConstraint::exact(&target));
                    value.coerce(&target);
                }
                Err(_) => {
                    value.types = Some(InputTypeConstraint::types([]));
                    context.report_analysis_problem(
                        AnalysisProblem::coalesce_expr_type_unknown(arguments.iter()),
                    )?;
                }
            }
        }
        Ok(SemScalarExpr::coalesce(&value))
    }
}

#[cfg(test)]
mod test_resolve {
    use crate::sem::SemAst;

    // Small-catalog semantic and failure-continuation contracts.

    use crate::test_helpers::coalesce as test_helpers;

    use crate::test_helpers::create_cache_context;

    use crate::{ast::Dialect, parse_statements, sem::ParseContext};

    use std::collections::BTreeMap;

    #[rstest::rstest]
    #[case::mixed("SELECT COALESCE($1,1,true)")]
    #[case::unreachable_mixed("SELECT COALESCE(1,$1,true)")]
    #[case::contradictory("SELECT COALESCE($1,1) WHERE $1")]
    fn failures_retain_operands_and_allow_next_statement(
        #[case] sql: &str,
    ) -> testresult::TestResult {
        let mut cache_context = create_cache_context();
        let analyzed = SemAst::from_ast(
            &parse_statements(sql, Dialect::postgres()).next().unwrap()?,
            test_helpers::context(),
            &mut cache_context,
        )?;
        assert!(
            analyzed
                .context()
                .problems()
                .iter()
                .any(|p| p.code() == "parameter_constraint"),
            "{:?}",
            analyzed.context().problems()
        );
        assert!(
            !analyzed
                .context()
                .parameter_constraints()
                .allows(&BTreeMap::new())
        );
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let coalesce = query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap()
            .as_coalesce()
            .unwrap();
        assert!(coalesce.arguments().len() >= 2);
        assert_eq!(
            coalesce.types().unwrap().admissible_types(),
            Some([].as_slice())
        );
        let next = SemAst::from_ast(
            &parse_statements("SELECT 1", Dialect::postgres())
                .next()
                .unwrap()?,
            analyzed.into_context(),
            &mut cache_context,
        )?;
        assert!(
            next.context()
                .parameter_constraints()
                .allows(&BTreeMap::new())
        );
        Ok(())
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::heterogeneous("SELECT COALESCE(?1,1,'text')")]
    #[case::case_insensitive("SELECT coalesce(?1,1)")]
    #[case::null("SELECT COALESCE(?1,NULL)")]
    fn sqlite_does_not_impose_postgres_common_types(#[case] sql: &str) -> testresult::TestResult {
        let mut cache_context = create_cache_context();
        let analyzed = SemAst::from_ast(
            &parse_statements(sql, Dialect::sqlite()).next().unwrap()?,
            test_helpers::sqlite_context(),
            &mut cache_context,
        )?;
        assert!(
            analyzed
                .context()
                .problems()
                .iter()
                .all(|p| p.code() == "unreachable_coalesce_argument"),
            "{:?}",
            analyzed.context().problems()
        );
        assert!(
            analyzed
                .context()
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
                .is_any()
        );
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        assert!(
            query
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .col_type()
                .is_none()
        );
        assert_eq!(
            query
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .expression()
                .unwrap()
                .as_coalesce()
                .unwrap()
                .arguments()
                .len(),
            if sql.contains("text") { 3 } else { 2 }
        );
        Ok(())
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_requires_two_arguments_without_losing_the_expression() -> testresult::TestResult {
        let mut cache_context = create_cache_context();
        let analyzed = SemAst::from_ast(
            &parse_statements("SELECT COALESCE(?1)", Dialect::sqlite())
                .next()
                .unwrap()?,
            test_helpers::sqlite_context(),
            &mut cache_context,
        )?;
        assert!(
            analyzed
                .context()
                .problems()
                .iter()
                .any(|p| p.code() == "parameter_constraint")
        );
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        assert_eq!(
            query
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .expression()
                .unwrap()
                .as_coalesce()
                .unwrap()
                .types()
                .unwrap()
                .admissible_types(),
            Some([].as_slice())
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::multiple("SELECT COALESCE($1,$2)", 2)]
    #[case::single("SELECT COALESCE($1)", 1)]
    #[case::null("SELECT COALESCE($1,NULL)", 2)]
    fn untyped_operands_retain_the_semantic_expression(
        #[case] sql: &str,
        #[case] count: usize,
    ) -> testresult::TestResult {
        let mut cache_context = create_cache_context();
        let analyzed = SemAst::from_ast(
            &parse_statements(sql, Dialect::postgres()).next().unwrap()?,
            test_helpers::context(),
            &mut cache_context,
        )?;
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let coalesce = query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap()
            .as_coalesce()
            .unwrap();
        assert_eq!(coalesce.arguments().len(), count);
        assert!(
            analyzed
                .context()
                .parameter_constraints()
                .parameter(1)
                .is_some()
        );
        // Do not assert the premature text narrowing tracked separately in #676.
        Ok(())
    }

    #[test]
    fn incompatible_concrete_operands_retain_a_failed_type_domain() -> testresult::TestResult {
        let mut cache_context = create_cache_context();
        let analyzed = SemAst::from_ast(
            &parse_statements("SELECT COALESCE(1,true)", Dialect::postgres())
                .next()
                .unwrap()?,
            test_helpers::context(),
            &mut cache_context,
        )?;
        let failure = analyzed
            .context()
            .problems()
            .iter()
            .find(|p| p.code() == "coalesce_expr_type_unknown")
            .unwrap();
        assert!(failure.render_metadata().is_some());
        let payload = ::serde_json::to_value(failure)?;
        assert_eq!(payload["CoalesceExprTypeUnknown"][0]["kind"], "literal");
        assert_eq!(
            payload["CoalesceExprTypeUnknown"][0]["type_reference"],
            "pg_catalog.int4"
        );
        assert_eq!(
            payload["CoalesceExprTypeUnknown"][1]["type_reference"],
            "pg_catalog.bool"
        );
        assert_eq!(payload["CoalesceExprTypeUnknown"][1]["not_null"], true);
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let coalesce = query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap()
            .as_coalesce()
            .unwrap();
        assert_eq!(
            coalesce.types().unwrap().admissible_types(),
            Some([].as_slice())
        );
        assert_eq!(coalesce.arguments().len(), 2);
        Ok(())
    }
}

impl CoalesceExpr {
    pub(crate) fn resolve_join_key(
        context: &mut impl ParseContext,
        arguments: &[SemScalarExpr],
        name: &ColumnName,
        not_null: bool,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let sqlite_type = if context.dialect() == Dialect::sqlite() {
            if let Some(first) = arguments.first() {
                first.get_type()
            } else {
                None
            }
        } else {
            None
        };
        let mut expression = Self::resolve(context, arguments)?;
        if let SemScalarExpr::Coalesce(value) = &mut expression {
            value.join_key_not_null = Some(not_null);
            value.join_key_name = Some(name.as_str().to_owned());
            if let Some(ty) = sqlite_type {
                value.coalesce_type = Some(ty);
            }
        }
        Ok(expression)
    }

    pub(crate) fn join_key_name(&self) -> Option<&str> {
        self.join_key_name.as_deref()
    }

    /// Operands in source order, including any determined implicit coercions.
    pub fn arguments(&self) -> &[SemScalarExpr] {
        &self.arg_exprs
    }

    /// Inferred output domain, when retained by semantic analysis.
    pub const fn types(&self) -> Option<&InputTypeConstraint> {
        self.types.as_ref()
    }

    pub(crate) const fn node(&self) -> Option<usize> {
        self.node
    }

    pub(super) const fn canonicalize_grouping_key(&mut self) {
        self.node = None;
    }

    fn coerce(&mut self, target: &TypeReference) {
        for argument in &mut self.arg_exprs {
            if argument.get_type().as_ref() != Some(target)
                || MinMaxExpr::input_type(argument) == Some(MinMaxExpr::unknown())
            {
                *argument = SemScalarExpr::wrap_implicit_cast(target, argument);
            }
        }
    }

    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for value in &mut self.arg_exprs {
            value.walk(visitor)?;
        }
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        for value in &self.arg_exprs {
            value.visit(visitor)?;
        }
        Ok(())
    }

    pub(crate) fn collect_constraints(
        &mut self,
        parameters: &mut ParameterResolution<'_>,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            return Ok(());
        }
        if self
            .arg_exprs
            .iter()
            .any(|value| value.constraint_node().is_some())
            || self
                .types
                .as_ref()
                .is_some_and(|types| types.admissible_types() == Some(&[]))
        {
            let (node, types) = parameters.unify_coalesce_types(context, &self.arg_exprs)?;
            self.node = Some(node);
            self.coalesce_type = types.single_type();
            self.types = Some(types);
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_parameter_constraints {
    use crate::sem::SemAst;

    // Small-catalog semantic and failure-continuation contracts.

    use crate::test_helpers::coalesce as test_helpers;

    use crate::test_helpers::create_cache_context;

    use crate::{
        ast::Dialect,
        parse_statements,
        sem::{ParseContext, PgBuiltInType},
    };

    use std::collections::BTreeMap;

    #[test]
    fn an_unreachable_parameter_still_constrains_the_result() -> testresult::TestResult {
        let mut cache_context = create_cache_context();
        let analyzed = SemAst::from_ast(
            &parse_statements("SELECT COALESCE(1,$1)", Dialect::postgres())
                .next()
                .unwrap()?,
            test_helpers::context(),
            &mut cache_context,
        )?;
        assert!(
            analyzed
                .context()
                .parameter_constraints()
                .allows(&BTreeMap::from([(1, PgBuiltInType::int4())]))
        );
        assert!(
            !analyzed
                .context()
                .parameter_constraints()
                .allows(&BTreeMap::from([(1, PgBuiltInType::bool())]))
        );
        let warning = analyzed
            .context()
            .problems()
            .iter()
            .find(|p| p.code() == "unreachable_coalesce_argument")
            .unwrap();
        assert_eq!(
            warning.severity(),
            ::sql_fun_core::analysis_problem::DiagnosticSeverity::Warning
        );
        assert!(warning.render_metadata().is_some());
        Ok(())
    }
}

impl CoalesceExpr {
    pub(crate) fn apply_constraints(&mut self, constraints: &ParameterConstraints) {
        let Some(node) = self.node else {
            return;
        };
        let types = constraints.node_domain(node).clone();
        self.coalesce_type = types.single_type();
        self.types = Some(types);
        if let Some(target) = self.coalesce_type.clone() {
            self.coerce(&target);
        }
    }
}
