use crate::names::AliasName;
use crate::names::FullName;
use crate::sem::ArithExpr;
use crate::sem::DeleteStatement;
use crate::sem::EngineFeatureValidation;
use crate::sem::FuncCallExpr;
use crate::sem::InsertStatement;
use crate::sem::MergeStatement;
use crate::sem::QueryLock;
use crate::sem::QueryOrder;
use crate::sem::TypeReference;
use crate::sem::UpdateStatement;
use crate::sem::WithTypmod;
mod clauses;

use crate::{
    CacheContext,
    ast::{Cast, OperatorKind, SqlValue, TableSample},
    sem::{SetOperationKind, TableSampleSource, with_stack},
};

use self::clauses::ClauseBinding;

use crate::{
    ast::{
        Alias, Call, CteBody, Delete, DerivedTable, Dialect, Expr, FromGroup, FromItem, Insert,
        Into, Join, JoinKind, Merge, Name, Query, QueryBody, Relation, Select, SetOperation,
        SetQuery, TableFunctions, Target, TypedColumn, Update, With,
    },
    context::ParseContext,
    names::ColumnName,
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, CteStatement, DataSource,
        DataSourceRowsFromFunc, FromClause, FunctionModifiers, JoinKind as SemanticJoinKind,
        JoinUsingClause, QueryCte, QueryGrouping, QueryWindow, ResultColumnCollection,
        ScalarAnalysisContext, SelectClauses, SelectDestination, SelectStatement,
        SelectStatementNode, SemScalarExpr, SemanticQuery, TableFunctionOutput, WithClause,
        problem::{
            DeleteProblemKind, InsertProblemKind, QueryProblem, RuleProblem, UpdateProblemKind,
        },
    },
};

impl SelectStatement {
    #[cfg(test)]
    pub(crate) fn from_ast_test_in(
        ast: &Query,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        Self::from_ast_in(ast, context, outer)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast(
        ast: &Query,
        context: &mut impl ParseContext,
        cache: &mut CacheContext,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_in(ast, context, None);
        let result = context.infer_query_parameter_types(result, cache);
        context.finish_analysis(result)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_in(
        ast: &Query,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        with_stack(|| Self::from_ast_in_body(ast, context, outer))
    }
}

#[cfg(test)]
mod test_name_resolution_phase {
    use crate::sem::SelectStatement;

    use crate::{
        parse_statements,
        sem::parameter_resolution::inference_runs as parameter_resolution_inference_runs,
    };

    use crate::{
        ast::{Dialect, StatementNode},
        context::ParseContext,
        names::OperatorName,
        sem::{BinaryOperatorDefinition, OperatorDefinition, PgBuiltInType},
        test_helpers::TestParseContext,
    };

    #[rstest::rstest]
    #[case("SELECT CASE WHEN true THEN $1 ELSE 1 END")]
    #[case("SELECT COALESCE($1, 1)")]
    #[case("SELECT ($1)[$2]")]
    #[case("SELECT $1 = ANY($2)")]
    #[case("SELECT $1 = ALL($2)")]
    fn untyped_operands_do_not_prevent_name_resolution(#[case] sql: &str) {
        let dialect = Dialect::postgres();
        let statement = parse_statements(sql, dialect).next().unwrap().unwrap();
        let StatementNode::Query(query) = statement.node else {
            panic!("query fixture expected");
        };
        let mut catalog = TestParseContext::default();
        let int = PgBuiltInType::int4();
        let operator = OperatorDefinition::binary(&[BinaryOperatorDefinition::new(
            &int,
            &int,
            &PgBuiltInType::bool(),
            true,
        )]);
        catalog.set_get_operator_op_result(
            &OperatorName::from_parts(None, "="),
            true,
            true,
            Some(&operator),
        );
        let before = parameter_resolution_inference_runs();
        catalog.begin_parameter_type_analysis();
        let result = SelectStatement::from_ast_in(&query, &mut catalog, None);
        catalog.finish_analysis(result).unwrap();
        assert_eq!(catalog.reported_problem_count(), 0);
        assert_eq!(crate::sem::parameter_resolution::inference_runs(), before);
    }
}

impl SelectStatement {
    #[inline(never)]
    pub(crate) fn from_ast_in_body(
        ast: &Query,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        Self::analyze_ast_query(ast, context, outer)
    }
}

#[cfg(test)]
mod grouping_tests;
#[cfg(test)]
mod tests;

impl SelectStatement {
    pub(crate) fn analyze_ast_query(
        ast: &Query,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        let with = WithClause::from_ast(&ast.with, context, outer)?;
        let restricted = if Self::ast_is_compound(&ast.body)
            && outer.is_some_and(|scope| scope.direct_rule_identity().is_some())
        {
            outer.map(ScalarAnalysisContext::without_rule_rows)
        } else {
            None
        };
        let query_id = context.analysis_state_mut().grouping_mut().new_query();
        let body = Self::from_ast_body(
            &ast.body,
            context,
            &with,
            restricted.as_ref().or(outer),
            query_id,
        )?;
        if body.select_clauses().is_none() {
            context.analysis_state_mut().grouping_mut().set_keys(
                query_id,
                &[],
                body.result_columns(),
            )?;
        }
        let empty = FromClause::default();
        let order_outer = if Self::ast_is_compound(&ast.body) {
            None
        } else {
            outer
        };
        let mut scope = ScalarAnalysisContext::nested(
            &with,
            body.source_scope().unwrap_or(&empty),
            order_outer,
        );
        scope.set_grouping_query(query_id);
        scope.set_windows(Self::ast_window_names(&ast.body));
        if context.dialect() == Dialect::sqlite() {
            scope.set_output_aliases(body.result_columns());
        }
        let binding = QueryOrder::ast_order_binding(&ast.body);
        let order = QueryOrder::from_ast_for_query(
            &ast.order,
            &ast.body,
            context,
            &scope,
            body.result_columns(),
            binding,
        )?;
        let row_scope = ScalarAnalysisContext::nested(&with, &empty, outer);
        let row_scope = row_scope.with_grouping_placement("LIMIT/OFFSET");
        let limit = SemScalarExpr::from_ast_optional(&ast.limit, context, &row_scope)?;
        let offset = SemScalarExpr::from_ast_optional(&ast.offset, context, &row_scope)?;
        let locks = QueryLock::from_ast_list(&ast.locks, context, &scope)?;
        let local = WithClause::ast_local_definitions(&ast.with, with);
        let mut query = SemanticQuery::new(local, ast.with.recursive, body, order, limit, offset);
        query.set_modifiers(ast.with_ties, Self::ast_set_all(&ast.body), locks);
        let mut query = Self::query(query);
        query.complete_grouping(context)?;
        Ok(query)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_body(
        ast: &QueryBody,
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
        query_id: usize,
    ) -> Result<Self, AnalysisError> {
        match ast {
            QueryBody::Select(select) => {
                Self::from_ast_select(select.as_ref(), context, with, outer, query_id)
            }
            QueryBody::Set(set) => Self::from_ast_set(set.as_ref(), context, with, outer),
            QueryBody::Values(rows) => Self::from_ast_values(rows, context, with, outer, query_id),
        }
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_values(
        rows: &[Vec<Expr>],
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
        query_id: usize,
    ) -> Result<Self, AnalysisError> {
        let from = FromClause::default();
        let mut scope = ScalarAnalysisContext::nested(with, &from, outer);
        scope.set_grouping_query(query_id);
        let scope = scope.with_grouping_placement("VALUES");
        let mut resolved = Vec::new();
        for row in rows {
            resolved.push(SemScalarExpr::from_ast_list(row, context, &scope)?);
        }
        Ok(Self::values(resolved))
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_set_operation(
        ast: &SetOperation,
        context: &mut impl ParseContext,
        left: Self,
        right: Self,
    ) -> Result<Self, AnalysisError> {
        let kind = match ast {
            SetOperation::Union => SetOperationKind::union(),
            SetOperation::Intersect => SetOperationKind::intersect(),
            SetOperation::Except => SetOperationKind::except(),
        };
        Self::set_operation(context, kind, left, right)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_select(
        ast: &Select,
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
        query_id: usize,
    ) -> Result<Self, AnalysisError> {
        Self::validate_ast_rule_destination(ast, context, outer)?;
        let from = Self::from_ast_from(ast, context, with, outer, query_id)?;
        let mut scope = Self::ast_scope(ast, with, &from, outer, query_id);
        let windows = Self::from_ast_windows(ast, context, &scope)?;
        let predicate = Self::from_ast_where(ast, context, &scope)?;
        let mut results = Self::from_ast_targets(ast, context, with, &from, &scope)?;
        let distinct = Self::from_ast_distinct(ast, context, &scope, &results)?;
        let group = Self::from_ast_grouping(ast, context, &mut scope, &mut results, query_id)?;
        let having = Self::from_ast_having(ast, context, &scope)?;
        let clauses = Self::from_ast_clauses(ast, distinct, group, having, windows)?;
        Ok(Self::ast_build_result(results, from, predicate, clauses))
    }
}

impl SelectStatement {
    pub(crate) fn validate_ast_rule_destination(
        ast: &Select,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<(), AnalysisError> {
        if ast.into.is_some()
            && let Some(scope) = outer
            && let Some(identity) = scope.rule_identity()
        {
            context.report_analysis_problem(AnalysisProblem::rule(&RuleProblem::definition(
                identity,
                "SELECT INTO is not allowed in rule actions",
            )))?;
        }
        Ok(())
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_from(
        ast: &Select,
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
        query_id: usize,
    ) -> Result<FromClause, AnalysisError> {
        let empty = FromClause::default();
        let mut from_scope = ScalarAnalysisContext::nested(with, &empty, outer);
        from_scope.set_grouping_query(query_id);
        let from_scope = from_scope.with_grouping_placement("FROM clause of its own query level");
        let from = DataSource::from_ast_list_in_scope(&ast.from, context, with, &from_scope)?;
        if let Some(scope) = outer {
            scope.validate_rule_sources(context, &from)?;
        }
        Ok(from)
    }
}

impl SelectStatement {
    pub(crate) fn ast_scope<'a>(
        ast: &Select,
        with: &'a WithClause,
        from: &'a FromClause,
        outer: Option<&ScalarAnalysisContext<'a>>,
        query_id: usize,
    ) -> ScalarAnalysisContext<'a> {
        let mut scope = ScalarAnalysisContext::nested(with, from, outer);
        scope.set_grouping_query(query_id);
        scope.set_windows(QueryWindow::ast_names(&ast.windows));
        scope
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_windows(
        ast: &Select,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Vec<QueryWindow>, AnalysisError> {
        QueryWindow::from_ast_definitions(&ast.windows, context, scope)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_where(
        ast: &Select,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        SemScalarExpr::from_ast_optional(
            &ast.predicate,
            context,
            &scope.with_grouping_placement("WHERE"),
        )
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_targets(
        ast: &Select,
        context: &mut impl ParseContext,
        with: &WithClause,
        from: &FromClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<ResultColumnCollection, AnalysisError> {
        let mut results = ResultColumnCollection::default();
        for target in &ast.targets {
            ResultColumnCollection::ast_append_to_results(
                target,
                context,
                with,
                from,
                scope,
                &mut results,
            )?;
        }
        Ok(results)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_distinct(
        ast: &Select,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        results: &ResultColumnCollection,
    ) -> Result<Option<Vec<SemScalarExpr>>, AnalysisError> {
        let distinct = if let Some(distinct) = &ast.distinct {
            let mut resolved = Vec::new();
            for expression in distinct {
                resolved.push(SemScalarExpr::from_ast_clause(
                    expression,
                    context,
                    scope,
                    results,
                    ClauseBinding::Order,
                )?);
            }
            Some(resolved)
        } else {
            None
        };
        Ok(distinct)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_grouping(
        ast: &Select,
        context: &mut impl ParseContext,
        scope: &mut ScalarAnalysisContext<'_>,
        results: &mut ResultColumnCollection,
        query_id: usize,
    ) -> Result<Vec<QueryGrouping>, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            scope.set_output_aliases(results);
        }
        let group = QueryGrouping::from_ast_list(&ast.group, context, scope, results)?;
        for grouping in &group {
            grouping.validate_grouping_targets(results, query_id, context)?;
        }
        context
            .analysis_state_mut()
            .grouping_mut()
            .set_keys(query_id, &group, results)?;
        Ok(group)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_having(
        ast: &Select,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        SemScalarExpr::from_ast_optional(&ast.having, context, scope)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_clauses(
        ast: &Select,
        distinct: Option<Vec<SemScalarExpr>>,
        group: Vec<QueryGrouping>,
        having: Option<SemScalarExpr>,
        windows: Vec<QueryWindow>,
    ) -> Result<SelectClauses, AnalysisError> {
        let mut clauses = SelectClauses::new(distinct, group, ast.group_distinct, having, windows);
        let destination = ast
            .into
            .as_ref()
            .map(SelectDestination::from_ast_destination)
            .transpose()?;
        clauses.set_destination(destination);
        Ok(clauses)
    }
}

impl SelectStatement {
    pub(crate) fn ast_build_result(
        results: ResultColumnCollection,
        from: FromClause,
        predicate: Option<SemScalarExpr>,
        clauses: SelectClauses,
    ) -> Self {
        Self::from(SelectStatementNode::from_resolution(
            results,
            &WithClause::default(),
            from,
            predicate,
            clauses,
        ))
    }
}

impl ResultColumnCollection {
    pub(crate) fn ast_append_to_results(
        ast: &Target,
        context: &mut impl ParseContext,
        with: &WithClause,
        from: &FromClause,
        scope: &ScalarAnalysisContext<'_>,
        results: &mut Self,
    ) -> Result<(), AnalysisError> {
        if let Some(name) = ast.source_star_name() {
            return Self::ast_append_source_star(ast, name, context, with, from, scope, results);
        }
        if let Some(base) = SemScalarExpr::ast_projection_base(&ast.expression, context, scope)? {
            return results.append_composite_star(context, base);
        }
        let expression = SemScalarExpr::from_ast_in(&ast.expression, context, scope)?;
        expression.validate_scalar_value(context)?;
        let name = ast
            .alias
            .clone()
            .unwrap_or_else(|| expression.get_column_name());
        results.push_projection(
            &ColumnName::from(name.as_str()),
            expression,
            ast.alias.is_some() && !ast.implicit_alias,
        );
        Ok(())
    }
}

impl ResultColumnCollection {
    pub(crate) fn ast_append_source_star(
        ast: &Target,
        name: &Name,
        context: &mut impl ParseContext,
        with: &WithClause,
        from: &FromClause,
        scope: &ScalarAnalysisContext<'_>,
        results: &mut Self,
    ) -> Result<(), AnalysisError> {
        let relation = FullName::from_ast_star_relation(name)?;
        let qualifier = if relation.is_none() {
            AliasName::from_ast_star_qualifier(name)?
        } else {
            None
        };
        let projection = if let Some(relation) = &relation {
            scope.relation_projection_names(context, relation)?
        } else if let Some(qualifier) = &qualifier {
            scope.projection_names(context, qualifier)?
        } else {
            from.wildcard_projection(context, with, None)?
        };
        let source_found = projection.source_found();
        if !source_found && qualifier.is_some() {
            let base = SemScalarExpr::from_ast_column(name, context, scope)?;
            return results.append_composite_star(context, base);
        }
        if !source_found {
            context.report_analysis_problem(AnalysisProblem::query(
                QueryProblem::reference_not_found("star source"),
            ))?;
        }
        let start = results.len();
        projection.append_results(context, scope, results, ast.location, None)?;
        if relation.is_none() && qualifier.is_none() {
            results.mark_join_sensitive_star_since(start);
        }
        Ok(())
    }
}

#[cfg(test)]
mod wildcard_tests;

impl WithClause {
    pub(crate) fn ast_local_definitions(ast: &With, mut visible: Self) -> Vec<QueryCte> {
        let mut local = Vec::new();
        for entry in &ast.entries {
            if let Some(statement) = visible.take(&entry.name) {
                local.push(QueryCte::new(
                    &entry.name,
                    entry.column_names(),
                    entry.materialized,
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
        for entry in &ast.entries {
            if let Some(scope) = outer
                && let Some(identity) = scope.direct_rule_identity()
                && matches!(entry.name.as_str(), "old" | "new")
            {
                context.report_analysis_problem(AnalysisProblem::rule(
                    &RuleProblem::definition(identity, "CTE conflicts with rule OLD/NEW namespace"),
                ))?;
            }
            CteStatement::ast_require_data_modifying_placement(
                &entry.body,
                context,
                outer.is_none()
                    || outer.is_some_and(|scope| scope.direct_rule_identity().is_some()),
            )?;
            let from = FromClause::default();
            let restricted = outer.map(ScalarAnalysisContext::without_rule_rows);
            let scope = ScalarAnalysisContext::nested(&with, &from, restricted.as_ref());
            let body = CteStatement::from_ast_cte_body(
                &entry.body,
                context,
                &scope,
                &entry.column_names(),
            )?;
            with.insert_resolved(context, &entry.name, body);
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

impl MergeStatement {
    pub(crate) fn from_ast_cte_body(
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
        merge: &Merge,
    ) -> Result<CteStatement, AnalysisError> {
        Self::ast_require_version(context, 17)?;
        let mut merge = Self::from_ast_in(merge, context, Some(scope))?;
        if columns.len() > merge.result_columns().len() {
            return Err(AnalysisError::unexpected_input_error(
                "MERGE CTE declares more aliases than RETURNING columns",
            ));
        }
        merge.rename_columns(columns);
        Ok(CteStatement::from_merge(merge))
    }
}

impl DeleteStatement {
    pub(crate) fn from_ast_cte_body(
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
        delete: &Delete,
    ) -> Result<CteStatement, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            context.report_analysis_problem(AnalysisProblem::delete(
                DeleteProblemKind::sqlite_cte(),
            ))?;
        }
        let mut delete = Self::from_ast_in(delete, context, Some(scope))?;
        if columns.len() > delete.result_columns().len() {
            context.report_analysis_problem(AnalysisProblem::delete(
                DeleteProblemKind::cte_aliases(),
            ))?;
        }
        delete.rename_columns(columns);
        Ok(CteStatement::from_delete(&delete))
    }
}

impl UpdateStatement {
    pub(crate) fn from_ast_cte_body(
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
        update: &Update,
    ) -> Result<CteStatement, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            return AnalysisError::raise_unexpected_input("SQLite UPDATE CTE");
        }
        let Some(mut update) = Self::from_ast_in(update, context, Some(scope))? else {
            return Ok(CteStatement::unresolved());
        };
        if columns.len() > update.result_columns().len() {
            context.report_analysis_problem(AnalysisProblem::update(
                UpdateProblemKind::cte_aliases(),
                "UPDATE CTE has more aliases than outputs",
            ))?;
        }
        update.rename_columns(columns);
        Ok(CteStatement::from_update(update))
    }
}

impl InsertStatement {
    pub(crate) fn from_ast_cte_body(
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
        insert: &Insert,
    ) -> Result<CteStatement, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            return AnalysisError::raise_unexpected_input("SQLite INSERT CTE");
        }
        let Some(mut insert) = Self::from_ast_in(insert, context, Some(scope))? else {
            return Ok(CteStatement::unresolved());
        };
        if columns.len() > insert.result_columns().len() {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::cte_aliases(),
                "INSERT CTE has more column aliases than RETURNING columns",
            ))?;
        }
        insert.rename_columns(columns);
        Ok(CteStatement::from_insert(insert))
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_cte_body(
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
        query: &Query,
    ) -> Result<CteStatement, AnalysisError> {
        let mut query = Self::from_ast_in(query, context, Some(scope))?;
        query.rename_columns(columns);
        Ok(CteStatement::from_query(query))
    }
}

impl AliasSpec {
    pub(crate) fn from_ast(ast: &Alias) -> Self {
        Self::from_names(&ast.name, &ast.columns)
    }
}

impl SemanticJoinKind {
    pub(crate) const fn from_ast(ast: &JoinKind) -> Self {
        match ast {
            JoinKind::Inner | JoinKind::Cross => Self::inner(),
            JoinKind::Left => Self::left(),
            JoinKind::Right => Self::right(),
            JoinKind::Full => Self::full(),
        }
    }
}

impl DataSource {
    pub(crate) fn from_ast(
        ast: &FromItem,
        context: &mut impl ParseContext,
        with: &WithClause,
        preceding: &FromClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        let scope = ScalarAnalysisContext::nested(with, preceding, outer);
        Self::from_ast_in_scope(ast, context, with, &scope)
    }
}

impl DataSource {
    pub(crate) fn from_ast_in_scope(
        ast: &FromItem,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        with_stack(|| Self::from_ast_body(ast, context, with, scope))
    }
}

impl DataSource {
    #[inline(never)]
    pub(crate) fn from_ast_body(
        ast: &FromItem,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        match ast {
            FromItem::Relation(relation) => Self::from_ast_relation(relation, context, with, scope),
            FromItem::Query(derived) => {
                Self::from_ast_derived_table(derived.as_ref(), context, scope)
            }
            FromItem::Join(join) => Self::from_ast_join(join.as_ref(), context, with, scope),
            FromItem::Functions(functions) => {
                Self::from_ast_table_functions(functions, context, scope)
            }
            FromItem::Group(group) => {
                Self::from_ast_from_group(group.as_ref(), context, with, scope)
            }
            FromItem::Xml(table) => Self::from_ast_xml_table(table.as_ref(), context, scope),
            FromItem::Sample(sample) => {
                Self::from_ast_table_sample(sample.as_ref(), context, with, scope)
            }
            FromItem::Json(_) => AnalysisError::raise_unexpected_none("from_clause inner node"),
        }
    }
}

impl JoinUsingClause {
    pub(crate) fn ast_using_clause(ast: &Join) -> Option<Self> {
        if ast.using.is_empty() {
            return None;
        }
        let mut columns = Vec::new();
        for column in &ast.using {
            columns.push(ColumnName::from(column.as_str()));
        }
        Some(Self::new(
            &columns,
            &ast.using_alias.as_ref().map(AliasSpec::from_ast),
        ))
    }
}

impl TableFunctionOutput {
    fn from_ast_typed_columns(
        columns: &[TypedColumn],
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let mut output = Self::default();
        for column in columns {
            let collation = column
                .collation
                .as_ref()
                .map(FullName::from_ast_name)
                .transpose()?;
            output.declare(
                context,
                &column.name,
                &TypeReference::from_ast(&column.type_name)?,
                &WithTypmod::<TypeReference>::ast_modifier_arguments(&column.type_name)?,
                collation.as_ref(),
            )?;
        }
        Ok(output)
    }
}

impl DataSource {
    pub(crate) fn from_ast_table_functions(
        ast: &TableFunctions,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        if ast.rows_from {
            return Self::from_ast_rows_from(ast, context, scope);
        }
        let [function] = ast.calls.as_slice() else {
            return AnalysisError::raise_unexpected_input("table function count");
        };
        let mut output = TableFunctionOutput::from_ast_typed_columns(&ast.columns, context)?;
        if ast.ordinality && !ast.columns.is_empty() {
            output.reject_placement(context)?;
        }
        let mut source = Self::from_ast_table_call(
            &function.expression,
            context,
            scope,
            ast.alias.as_ref().map(AliasSpec::from_ast).as_ref(),
        )?;
        source.apply_function_output(context, &output)?;
        if ast.ordinality {
            source.append_function_ordinality();
        }
        source.validate_function_aliases(context)?;
        Ok(source)
    }
}

impl DataSource {
    pub(crate) fn from_ast_rows_from(
        ast: &TableFunctions,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let mut outer_output = TableFunctionOutput::from_ast_typed_columns(&ast.columns, context)?;
        let outer_allowed =
            ast.calls.len() == 1 && !ast.ordinality && ast.calls[0].columns.is_empty();
        if !ast.columns.is_empty() && !outer_allowed {
            outer_output.reject_placement(context)?;
        }
        let invocation_alias = if ast.calls.len() == 1 {
            ast.alias
                .as_ref()
                .map(|value| AliasSpec::from_names(&value.name, &[]))
        } else {
            None
        };
        let mut functions = Vec::new();
        for function in &ast.calls {
            let local_output =
                TableFunctionOutput::from_ast_typed_columns(&function.columns, context)?;
            let output = if outer_allowed && !ast.columns.is_empty() {
                &outer_output
            } else {
                &local_output
            };
            let mut function = DataSourceRowsFromFunc::from_ast_call(
                &function.expression,
                context,
                scope,
                invocation_alias.as_ref(),
            )?;
            function.apply_output(context, output)?;
            functions.push(function);
        }
        let alias = ast
            .alias
            .as_ref()
            .map(AliasSpec::from_ast)
            .unwrap_or_else(|| {
                AliasSpec::from_local_name(
                    &functions
                        .first()
                        .map(|function| function.default_alias())
                        .unwrap_or_default(),
                )
            });
        let mut source = Self::resolved_rows(&functions, &alias);
        source.retain_rows_definition(&outer_output);
        if ast.ordinality {
            source.append_function_ordinality();
        }
        source.validate_function_aliases(context)?;
        Ok(source)
    }
}

impl DataSourceRowsFromFunc {
    pub(crate) fn from_ast(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_call(ast, context, scope, None).and_then(|mut source| {
            source.apply_output(context, &TableFunctionOutput::default())?;
            Ok(source)
        });
        context.finish_analysis(result)
    }
}

impl DataSourceRowsFromFunc {
    pub(crate) fn from_ast_call(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        alias: Option<&AliasSpec>,
    ) -> Result<Self, AnalysisError> {
        if let Expr::Call(call) = ast
            && SemScalarExpr::ast_is_ordinary(&call.kind)
        {
            return Ok(Self::invocation(&DataSource::from_ast_table(
                call.as_ref(),
                context,
                scope,
                alias,
            )?));
        }
        let value = SemScalarExpr::from_ast_source_expression(ast, context, scope)?;
        let name = if let Some(alias) = alias {
            alias.name().to_string()
        } else {
            TableFunctionOutput::ast_expression_name(ast, &value)?
        };
        Self::expression(&value, &name, context)
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_source_expression(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        match ast {
            Expr::Call(_) | Expr::SqlValue(_) | Expr::Cast(_) => {
                Self::from_ast_in(ast, context, scope)
            }
            Expr::Operator(operator) if ArithExpr::ast_is_null_if(&operator.kind) => {
                Self::from_ast_in(ast, context, scope)
            }
            _ => AnalysisError::raise_unexpected_input("ROWS FROM expression"),
        }
    }
}

impl TableFunctionOutput {
    pub(crate) fn ast_expression_name(
        ast: &Expr,
        value: &SemScalarExpr,
    ) -> Result<String, AnalysisError> {
        match ast {
            Expr::Call(call) => Self::ast_call_name(call.as_ref()),
            Expr::SqlValue(value) => Ok(Self::ast_sql_value_name(value)),
            Expr::Column(name) => Self::ast_relation_name(name),
            Expr::Operator(operator) if ArithExpr::ast_is_null_if(&operator.kind) => {
                Ok(Self::nullif_output_name())
            }
            Expr::Cast(cast) => Self::ast_cast_name(cast.as_ref(), value),
            _ => Ok(value.table_output_name()),
        }
    }
}

impl DataSource {
    pub(crate) fn from_ast_expression_table(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        alias: Option<&AliasSpec>,
    ) -> Result<Self, AnalysisError> {
        let function = DataSourceRowsFromFunc::from_ast_call(ast, context, scope, alias)?;
        let alias = alias
            .cloned()
            .unwrap_or_else(|| AliasSpec::from_local_name(&function.default_alias()));
        Ok(Self::resolved_rows(&[function], &alias))
    }
}

impl DataSource {
    pub(crate) fn from_ast_table_call(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        alias: Option<&AliasSpec>,
    ) -> Result<Self, AnalysisError> {
        match ast {
            Expr::Call(call) if SemScalarExpr::ast_is_ordinary(&call.kind) => {
                Self::from_ast_table(call.as_ref(), context, scope, alias)
            }
            _ => Self::from_ast_expression_table(ast, context, scope, alias),
        }
    }
}

impl DataSource {
    pub(crate) fn from_ast_table(
        ast: &Call,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        alias: Option<&AliasSpec>,
    ) -> Result<Self, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            todo!("SQLite table function affinity and overload semantics");
        }
        let mut arguments = SemScalarExpr::from_ast_list(&ast.arguments, context, scope)?;
        let mut invalid_arguments = false;
        for argument in &mut arguments {
            invalid_arguments |= DataSourceRowsFromFunc::validate_argument(context, argument)?;
        }
        let name = FullName::from_ast_name(&ast.name)?;
        let alias = alias
            .cloned()
            .unwrap_or_else(|| AliasSpec::from_local_name(&name.local_name()));
        if SemScalarExpr::ast_uses_supplied_binding(ast) {
            let mut call = FuncCallExpr::from_ast_supplied_call(ast, context, &arguments)?;
            call.set_modifiers(FunctionModifiers::from_ast_modifiers(ast, context, scope)?);
            let mut source = Self::deferred_table_function(context, &call, &alias);
            source.validate_table_function(context, invalid_arguments)?;
            return Ok(source);
        }
        let mut source = Self::resolve_function(context, &name, &arguments, &alias)?;
        source.validate_table_function(context, invalid_arguments)?;
        Ok(source)
    }
}

impl ArithExpr {
    pub(crate) const fn ast_is_null_if(ast: &OperatorKind) -> bool {
        matches!(ast, OperatorKind::NullIf)
    }
}

impl DataSource {
    pub(crate) fn from_ast_list_in(
        items: &[FromItem],
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<FromClause, AnalysisError> {
        let from = FromClause::default();
        let scope = ScalarAnalysisContext::nested(with, &from, outer);
        Self::from_ast_list_in_scope(items, context, with, &scope)
    }
}

impl DataSource {
    pub(crate) fn from_ast_list_in_scope(
        items: &[FromItem],
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<FromClause, AnalysisError> {
        let mut from = FromClause::default();
        for item in items {
            let source =
                Self::from_ast_in_scope(item, context, with, &scope.with_from_clause(&from))?;
            from.validate_source(context, &source)?;
            from.push(source);
        }

        Ok(from)
    }
}

impl DataSource {
    pub(crate) fn from_ast_list(
        items: &[FromItem],
        context: &mut impl ParseContext,
        with: &WithClause,
    ) -> Result<FromClause, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_list_in(items, context, with, None);
        context.finish_analysis(result)
    }
}

impl DataSource {
    pub(crate) fn from_ast_source(
        ast: &FromItem,
        context: &mut impl ParseContext,
        with: &WithClause,
        preceding: &FromClause,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast(ast, context, with, preceding, None);
        context.finish_analysis(result)
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

impl DataSource {
    pub(crate) fn from_ast_relation(
        ast: &Relation,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let name = FullName::from_ast_name(&ast.name)?;
        let name = if with.resolve_name(context, &name).is_none() {
            scope.trigger_relation_name(context, &name)?
        } else {
            name
        };
        let alias = ast
            .alias
            .as_ref()
            .map(AliasSpec::from_ast)
            .unwrap_or_else(|| AliasSpec::from_local_name(&name.local_name()));
        let source = Self::resolve_relation(context, &name, &alias, with)?;
        Ok(source)
    }
}

impl DataSource {
    pub(crate) fn from_ast_derived_table(
        ast: &DerivedTable,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let non_lateral_scope;
        let scope = if ast.lateral {
            scope
        } else {
            non_lateral_scope = scope.with_from_clause(&FromClause::default());
            &non_lateral_scope
        };
        let query = SelectStatement::from_ast_in(&ast.query, context, Some(scope))?;
        let alias = ast
            .alias
            .as_ref()
            .map(AliasSpec::from_ast)
            .unwrap_or_else(|| AliasSpec::from_local_name(""));
        Ok(Self::resolved_query(&query, &alias))
    }
}

impl DataSource {
    pub(crate) fn from_ast_join(
        ast: &Join,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let left = Self::from_ast_in_scope(&ast.left, context, with, scope)?;
        let visible = scope.with_added_source(&left);
        let right = Self::from_ast_in_scope(&ast.right, context, with, &visible)?;
        right
            .namespace()
            .validate_against(&left.namespace(), context)?;
        let scope = visible.with_added_source(&right);
        let condition = SemScalarExpr::from_ast_optional(
            &ast.condition,
            context,
            &scope.with_grouping_placement("JOIN condition"),
        )?;
        Ok(Self::resolved_join(
            &left,
            &right,
            &SemanticJoinKind::from_ast(&ast.kind),
            &condition,
            &ast.alias.as_ref().map(AliasSpec::from_ast),
            &JoinUsingClause::ast_using_clause(ast),
            ast.natural,
        ))
    }
}

impl DataSource {
    pub(crate) fn from_ast_from_group(
        ast: &FromGroup,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let Some((first, items)) = ast.items.split_first() else {
            return AnalysisError::raise_unexpected_input("empty FROM group");
        };
        let mut source = Self::from_ast_in_scope(first, context, with, scope)?;
        for item in items {
            let right = Self::from_ast_in_scope(item, context, with, scope)?;
            source = Self::resolved_join(
                &source,
                &right,
                &SemanticJoinKind::inner(),
                &None,
                &None,
                &None,
                false,
            );
        }
        if let Some(alias) = &ast.alias
            && Dialect::from(context.metadata()) == Dialect::sqlite()
        {
            source = source.with_group_alias(&AliasSpec::from_ast(alias));
        }
        Ok(source)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_set(
        ast: &SetQuery,
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        let from = FromClause::default();
        let scope = ScalarAnalysisContext::nested(with, &from, outer);
        let left = Self::from_ast_in(&ast.left, context, Some(&scope))?;
        let right = Self::from_ast_in(&ast.right, context, Some(&scope))?;
        Self::from_ast_set_operation(&ast.operation, context, left, right)
    }
}

impl SelectStatement {
    pub(crate) const fn ast_is_compound(ast: &QueryBody) -> bool {
        matches!(ast, QueryBody::Set(_))
    }
}

impl SelectStatement {
    pub(crate) fn ast_window_names(ast: &QueryBody) -> Vec<String> {
        match ast {
            QueryBody::Select(select) => QueryWindow::ast_names(&select.windows),
            _ => Vec::new(),
        }
    }
}

impl QueryOrder {
    pub(crate) const fn ast_order_binding(ast: &QueryBody) -> ClauseBinding {
        match ast {
            QueryBody::Set(_) => ClauseBinding::Compound,
            _ => ClauseBinding::Order,
        }
    }
}

impl SelectStatement {
    pub(crate) fn ast_set_all(ast: &QueryBody) -> bool {
        match ast {
            QueryBody::Set(set) => set.all,
            _ => false,
        }
    }
}

impl FunctionModifiers {
    pub(crate) fn from_ast_modifiers(
        ast: &Call,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let columns = ResultColumnCollection::default();
        let order =
            QueryOrder::from_ast_list(&ast.order, context, scope, &columns, ClauseBinding::Source)?;
        let within_order = QueryOrder::from_ast_list(
            &ast.within_order,
            context,
            scope,
            &columns,
            ClauseBinding::Source,
        )?;
        let filter = SemScalarExpr::from_ast_optional(
            &ast.filter,
            context,
            &scope.with_grouping_placement("aggregate FILTER"),
        )?;
        let over = ast
            .over
            .as_ref()
            .map(|window| QueryWindow::from_ast_over(window, context, scope))
            .transpose()?;
        let (within_order, order) = if ast.within_group && within_order.is_empty() {
            (order, Vec::new())
        } else {
            (within_order, order)
        };
        let mut modifiers = Self::new(within_order, order, filter, over);
        modifiers.set_flags(ast.distinct, ast.star, ast.variadic, ast.within_group);
        Ok(modifiers)
    }
}

impl SelectDestination {
    pub(crate) fn from_ast_destination(ast: &Into) -> Result<Self, AnalysisError> {
        Ok(Self::new(
            &FullName::from_ast_name(&ast.relation.name)?,
            &ast.relation.persistence,
        ))
    }
}

impl DataSource {
    pub(crate) fn from_ast_table_sample(
        ast: &TableSample,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let source = Self::from_ast_in_scope(&ast.relation, context, with, scope)?;
        let expression_scope = scope.with_from_clause(&FromClause::default());
        let arguments = SemScalarExpr::from_ast_list(
            &ast.arguments,
            context,
            &expression_scope.with_grouping_placement("TABLESAMPLE"),
        )?;
        let repeatable = SemScalarExpr::from_ast_optional(
            &ast.repeatable,
            context,
            &expression_scope.with_grouping_placement("REPEATABLE"),
        )?;
        let sample = TableSampleSource::resolve(
            context,
            &source,
            &FullName::from_ast_name(&ast.method)?,
            &arguments,
            &repeatable,
        )?;
        Ok(Self::sampled(&sample))
    }
}

impl TableFunctionOutput {
    pub(crate) fn ast_cast_name(
        ast: &Cast,
        value: &SemScalarExpr,
    ) -> Result<String, AnalysisError> {
        let name = Self::ast_expression_name(
            &ast.argument,
            value.explicit_cast_operand().unwrap_or(value),
        )?;
        if name.is_empty() {
            Ok(TypeReference::from_ast(&ast.target)?
                .full_name()
                .expect("SQL type syntax constructs a concrete reference")
                .local_name())
        } else {
            Ok(name)
        }
    }
}

impl TableFunctionOutput {
    pub(crate) fn ast_call_name(ast: &Call) -> Result<String, AnalysisError> {
        Self::ast_relation_name(&ast.name)
    }
}

impl TableFunctionOutput {
    pub(crate) fn ast_relation_name(ast: &Name) -> Result<String, AnalysisError> {
        Ok(FullName::from_ast_name(ast)?.local_name())
    }
}

impl TableFunctionOutput {
    pub(crate) fn ast_sql_value_name(ast: &SqlValue) -> String {
        ast.name.clone()
    }
}

impl TableFunctionOutput {
    fn nullif_output_name() -> String {
        "nullif".to_owned()
    }
}

impl SemScalarExpr {
    fn table_output_name(&self) -> String {
        self.get_column_name()
    }
}
