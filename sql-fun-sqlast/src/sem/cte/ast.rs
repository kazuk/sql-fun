use crate::{
    ast::{CteBody, With, validation::ContainsInvalidCteClause},
    context::ParseContext,
    sem::{
        AnalysisError, AnalysisProblem, CreateRule as SemCreateRule, CteStatement, DeleteStatement,
        EngineFeatureValidation, FromClause, InsertStatement, MergeStatement, QueryCte,
        ResolutionFailure, RuleIdentity, ScalarAnalysisContext, SelectStatement, UpdateStatement,
        WithClause,
        problem::{QueryProblem, RuleProblem},
    },
};

impl CteStatement {
    pub(crate) fn ast_require_data_modifying_placement(
        ast: &CteBody,
        context: &mut impl ParseContext,
        top_level: bool,
    ) -> Result<(), AnalysisError> {
        if (matches!(
            ast,
            CteBody::Insert(_) | CteBody::Update(_) | CteBody::Delete(_) | CteBody::Merge(_)
        ) && !top_level)
            || ast.contains_invalid_cte_clause()
        {
            context.report_analysis_problem(AnalysisProblem::query(
                QueryProblem::invalid_cte_placement(),
            ))?;
        }
        Ok(())
    }
}

impl WithClause {
    pub(crate) fn ast_local_definitions(ast: &With, mut visible: Self) -> Vec<QueryCte> {
        let mut local = Vec::new();
        for entry in ast.entries() {
            if let Some(statement) = visible.take(entry.name()) {
                local.push(QueryCte::new(
                    entry.name(),
                    entry.column_names(),
                    entry.materialized(),
                    statement,
                ));
            }
        }
        local
    }
}

impl WithClause {
    pub(crate) fn from_ast(
        ast: &With,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        let mut with = outer
            .map(ScalarAnalysisContext::inherited_ctes)
            .unwrap_or_default();
        for entry in ast.entries() {
            if let Some(scope) = outer
                && let Some(identity) = scope.direct_rule_identity()
                && matches!(entry.name().as_str(), "old" | "new")
            {
                context.report_analysis_problem(AnalysisProblem::rule(
                    &RuleProblem::definition(identity, "CTE conflicts with rule OLD/NEW namespace"),
                ))?;
            }
            CteStatement::ast_require_data_modifying_placement(
                entry.body(),
                context,
                outer.is_none()
                    || outer.is_some_and(|scope| scope.direct_rule_identity().is_some()),
            )?;
            let from = FromClause::default();
            let restricted = outer.map(ScalarAnalysisContext::without_rule_rows);
            let scope = ScalarAnalysisContext::nested(&with, &from, restricted.as_ref());
            let body = CteStatement::from_ast_cte_body(
                entry.body(),
                context,
                &scope,
                &entry.column_names(),
            )?;
            with.insert_resolved(context, entry.name(), body);
        }
        Ok(with)
    }
}

impl CteStatement {
    pub(crate) fn from_ast_cte_body(
        ast: &CteBody,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
    ) -> Result<Self, AnalysisError> {
        if !EngineFeatureValidation::cte_body(ast, context)? {
            return Ok(Self::unresolved());
        }
        match ast {
            CteBody::Query(query) => {
                SelectStatement::from_ast_cte_body(context, scope, columns, query)
            }
            CteBody::Insert(insert) => {
                InsertStatement::from_ast_cte_body(context, scope, columns, insert)
            }
            CteBody::Update(update) => {
                UpdateStatement::from_ast_cte_body(context, scope, columns, update)
            }
            CteBody::Delete(delete) => {
                DeleteStatement::from_ast_cte_body(context, scope, columns, delete)
            }
            CteBody::Merge(merge) => {
                MergeStatement::from_ast_cte_body(context, scope, columns, merge)
            }
        }
    }
}

impl WithClause {
    pub(crate) fn from_ast_top_level(
        ast: &With,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast(ast, context, None);
        context.finish_analysis(result)
    }
}

impl CteStatement {
    pub(crate) fn validate_ast_select_rule_ctes(
        ast: &CteBody,
        identity: &RuleIdentity,
    ) -> Result<(), ResolutionFailure> {
        match ast {
            CteBody::Query(query) => {
                SelectStatement::validate_ast_select_rule_ctes(query.as_ref(), identity)
            }
            _ => SemCreateRule::ast_invalid(
                identity,
                "ON SELECT rule cannot contain data-modifying CTEs",
            ),
        }
    }
}
