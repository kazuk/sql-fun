mod clauses;
#[cfg(test)]
mod grouping_tests;
#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod tests;

use self::clauses::ClauseBinding;

use crate::{
    ast::{
        Alias, Call, CallKind, CteBody, Delete, DerivedTable, Dialect, Expr, FromGroup, FromItem,
        Grouping, Insert, Into, Join, JoinKind, Lock, Merge, Name, Order, Query, QueryBody,
        Relation, Select, SetOperation, SetQuery, TableFunctions, Target, TypedColumn, Update,
        Window, With,
    },
    context::ParseContext,
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, ColumnName, CteStatement, DataSource,
        DataSourceRowsFromFunc, FromClause, FunctionModifiers, JoinKind as SemanticJoinKind,
        JoinUsingClause, QueryCte, QueryGrouping, QueryWindow, ResultColumnCollection,
        ScalarAnalysisContext, SelectClauses, SelectDestination, SelectStatement,
        SelectStatementNode, SemScalarExpr, SemanticQuery, TableFunctionOutput, WithClause,
        problem::{
            DeleteProblemKind, InsertProblemKind, QueryProblem, RuleProblem, UpdateProblemKind,
        },
    },
};

impl Query {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<SelectStatement, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context, None);
        let result = context.infer_query_parameter_types(result);
        context.finish_analysis(result)
    }

    pub(super) fn resolve_in(
        &self,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<SelectStatement, AnalysisError> {
        crate::sem::with_stack(|| Self::resolve_in_body(self, context, outer))
    }

    #[inline(never)]
    fn resolve_in_body(
        &self,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<SelectStatement, AnalysisError> {
        self.resolve_query_body(context, outer)
    }
    fn resolve_query_body(
        &self,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<SelectStatement, AnalysisError> {
        let with = self.with.resolve(context, outer)?;
        let restricted = if self.body.is_compound()
            && outer.is_some_and(|scope| scope.direct_rule_identity().is_some())
        {
            outer.map(ScalarAnalysisContext::without_rule_rows)
        } else {
            None
        };
        let query_id = context.analysis_state_mut().grouping_mut().new_query();
        let body = self
            .body
            .resolve(context, &with, restricted.as_ref().or(outer), query_id)?;
        if body.select_clauses().is_none() {
            context.analysis_state_mut().grouping_mut().set_keys(
                query_id,
                &[],
                body.result_columns(),
            )?;
        }
        let empty = FromClause::default();
        let order_outer = if self.body.is_compound() { None } else { outer };
        let mut scope = ScalarAnalysisContext::nested(
            &with,
            body.source_scope().unwrap_or(&empty),
            order_outer,
        );
        scope.set_grouping_query(query_id);
        scope.set_windows(self.body.window_names());
        if context.dialect() == Dialect::sqlite() {
            scope.set_output_aliases(body.result_columns());
        }
        let binding = self.body.order_binding();
        let order = Order::resolve_for_query(
            &self.order,
            &self.body,
            context,
            &scope,
            body.result_columns(),
            binding,
        )?;
        let row_scope = ScalarAnalysisContext::nested(&with, &empty, outer);
        let row_scope = row_scope.with_grouping_placement("LIMIT/OFFSET");
        let limit = Expr::resolve_optional(&self.limit, context, &row_scope)?;
        let offset = Expr::resolve_optional(&self.offset, context, &row_scope)?;
        let locks = Lock::resolve_list(&self.locks, context, &scope)?;
        let local = self.with.local_definitions(with);
        let mut query = SemanticQuery::new(local, self.with.recursive, body, order, limit, offset);
        query.set_modifiers(self.with_ties, self.body.set_all(), locks);
        let mut query = SelectStatement::query(query);
        query.complete_grouping(context)?;
        Ok(query)
    }
}

#[cfg(test)]
mod test_name_resolution_phase {
    use crate::ast::{Dialect, StatementNode};
    use crate::context::ParseContext;
    use crate::sem::{BinaryOperatorDefinition, OperatorDefinition, OperatorName, PgBuiltInType};
    use crate::test_helpers::TestParseContext;

    #[rstest::rstest]
    #[case("SELECT CASE WHEN true THEN $1 ELSE 1 END")]
    #[case("SELECT COALESCE($1, 1)")]
    #[case("SELECT ($1)[$2]")]
    #[case("SELECT $1 = ANY($2)")]
    #[case("SELECT $1 = ALL($2)")]
    fn untyped_operands_do_not_prevent_name_resolution(#[case] sql: &str) {
        let dialect = Dialect::postgres();
        let statement = crate::parse_statements(sql, dialect)
            .next()
            .unwrap()
            .unwrap();
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
        let before = crate::sem::parameter_resolution::inference_runs();
        catalog.begin_parameter_type_analysis();
        let result = query.resolve_in(&mut catalog, None);
        catalog.finish_analysis(result).unwrap();
        assert_eq!(catalog.reported_problem_count(), 0);
        assert_eq!(crate::sem::parameter_resolution::inference_runs(), before);
    }
}

impl QueryBody {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
        query_id: usize,
    ) -> Result<SelectStatement, AnalysisError> {
        match self {
            Self::Select(select) => select.resolve(context, with, outer, query_id),
            Self::Set(set) => set.resolve(context, with, outer),
            Self::Values(rows) => Self::resolve_values(rows, context, with, outer, query_id),
        }
    }

    fn resolve_values(
        rows: &[Vec<Expr>],
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
        query_id: usize,
    ) -> Result<SelectStatement, AnalysisError> {
        let from = FromClause::default();
        let mut scope = ScalarAnalysisContext::nested(with, &from, outer);
        scope.set_grouping_query(query_id);
        let scope = scope.with_grouping_placement("VALUES");
        let mut resolved = Vec::new();
        for row in rows {
            resolved.push(Expr::resolve_list(row, context, &scope)?);
        }
        Ok(SelectStatement::values(resolved))
    }
}

impl SetOperation {
    fn resolve(&self, left: SelectStatement, right: SelectStatement) -> SelectStatement {
        match self {
            Self::Union => SelectStatement::union(left, right),
            Self::Intersect => SelectStatement::intersect(left, right),
            Self::Except => SelectStatement::except(left, right),
        }
    }
}

impl Select {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
        query_id: usize,
    ) -> Result<SelectStatement, AnalysisError> {
        self.validate_rule_destination(context, outer)?;
        let from = self.resolve_from(context, with, outer, query_id)?;
        let mut scope = self.scope(with, &from, outer, query_id);
        let windows = self.resolve_windows(context, &scope)?;
        let predicate = self.resolve_where(context, &scope)?;
        let mut results = self.resolve_targets(context, with, &from, &scope)?;
        let distinct = self.resolve_distinct(context, &scope, &results)?;
        let group = self.resolve_grouping(context, &mut scope, &mut results, query_id)?;
        let having = self.resolve_having(context, &scope)?;
        let clauses = self.resolve_clauses(distinct, group, having, windows)?;
        Ok(Self::build_result(results, from, predicate, clauses))
    }

    fn validate_rule_destination(
        &self,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<(), AnalysisError> {
        if self.into.is_some()
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

    fn resolve_from(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
        query_id: usize,
    ) -> Result<FromClause, AnalysisError> {
        let empty = FromClause::default();
        let mut from_scope = ScalarAnalysisContext::nested(with, &empty, outer);
        from_scope.set_grouping_query(query_id);
        let from_scope = from_scope.with_grouping_placement("FROM clause of its own query level");
        let from = FromItem::resolve_list_in_scope(&self.from, context, with, &from_scope)?;
        if let Some(scope) = outer {
            scope.validate_rule_sources(context, &from)?;
        }
        Ok(from)
    }

    fn scope<'a>(
        &self,
        with: &'a WithClause,
        from: &'a FromClause,
        outer: Option<&ScalarAnalysisContext<'a>>,
        query_id: usize,
    ) -> ScalarAnalysisContext<'a> {
        let mut scope = ScalarAnalysisContext::nested(with, from, outer);
        scope.set_grouping_query(query_id);
        scope.set_windows(Window::names(&self.windows));
        scope
    }

    fn resolve_windows(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Vec<QueryWindow>, AnalysisError> {
        Window::resolve_definitions(&self.windows, context, scope)
    }

    fn resolve_where(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        Expr::resolve_optional(
            &self.predicate,
            context,
            &scope.with_grouping_placement("WHERE"),
        )
    }

    fn resolve_targets(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        from: &FromClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<ResultColumnCollection, AnalysisError> {
        let mut results = ResultColumnCollection::default();
        for target in &self.targets {
            target.append_to_results(context, with, from, scope, &mut results)?;
        }
        Ok(results)
    }

    fn resolve_distinct(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        results: &ResultColumnCollection,
    ) -> Result<Option<Vec<SemScalarExpr>>, AnalysisError> {
        let distinct = if let Some(distinct) = &self.distinct {
            let mut resolved = Vec::new();
            for expression in distinct {
                resolved.push(expression.resolve_clause(
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

    fn resolve_grouping(
        &self,
        context: &mut impl ParseContext,
        scope: &mut ScalarAnalysisContext<'_>,
        results: &mut ResultColumnCollection,
        query_id: usize,
    ) -> Result<Vec<QueryGrouping>, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            scope.set_output_aliases(results);
        }
        let group = Grouping::resolve_list(&self.group, context, scope, results)?;
        for grouping in &group {
            grouping.validate_grouping_targets(results, query_id, context)?;
        }
        context
            .analysis_state_mut()
            .grouping_mut()
            .set_keys(query_id, &group, results)?;
        Ok(group)
    }

    fn resolve_having(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        Expr::resolve_optional(&self.having, context, scope)
    }

    fn resolve_clauses(
        &self,
        distinct: Option<Vec<SemScalarExpr>>,
        group: Vec<QueryGrouping>,
        having: Option<SemScalarExpr>,
        windows: Vec<QueryWindow>,
    ) -> Result<SelectClauses, AnalysisError> {
        let mut clauses = SelectClauses::new(distinct, group, self.group_distinct, having, windows);
        let destination = self
            .into
            .as_ref()
            .map(|into| into.resolve_destination())
            .transpose()?;
        clauses.set_destination(destination);
        Ok(clauses)
    }

    fn build_result(
        results: ResultColumnCollection,
        from: FromClause,
        predicate: Option<SemScalarExpr>,
        clauses: SelectClauses,
    ) -> SelectStatement {
        SelectStatement::from(SelectStatementNode::from_resolution(
            results,
            &WithClause::default(),
            from,
            predicate,
            clauses,
        ))
    }
}

impl Target {
    fn append_to_results(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        from: &FromClause,
        scope: &ScalarAnalysisContext<'_>,
        results: &mut ResultColumnCollection,
    ) -> Result<(), AnalysisError> {
        if let Some(name) = self.source_star_name() {
            return self.append_source_star(name, context, with, from, scope, results);
        }
        if let Some(base) = self.expression.projection_base(context, scope)? {
            return results.append_composite_star(context, base);
        }
        let expression = self.expression.resolve_in(context, scope)?;
        let name = self
            .alias
            .clone()
            .unwrap_or_else(|| expression.get_column_name());
        results.push_projection(
            &ColumnName::from(name.as_str()),
            expression,
            self.alias.is_some() && !self.implicit_alias,
        );
        Ok(())
    }

    fn append_source_star(
        &self,
        name: &Name,
        context: &mut impl ParseContext,
        with: &WithClause,
        from: &FromClause,
        scope: &ScalarAnalysisContext<'_>,
        results: &mut ResultColumnCollection,
    ) -> Result<(), AnalysisError> {
        let relation = name.star_relation()?;
        let qualifier = if relation.is_none() {
            name.star_qualifier()?
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
            let base = name.resolve_column(context, scope)?;
            return results.append_composite_star(context, base);
        }
        if !source_found {
            context.report_analysis_problem(AnalysisProblem::query(
                QueryProblem::reference_not_found("star source"),
            ))?;
        }
        projection.append_results(context, scope, results, self.location, None)?;
        Ok(())
    }
}

#[cfg(test)]
mod wildcard_tests;

impl With {
    pub(in crate::ast) fn local_definitions(&self, mut visible: WithClause) -> Vec<QueryCte> {
        let mut local = Vec::new();
        for entry in &self.entries {
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
    pub(in crate::ast) fn resolve(
        &self,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<WithClause, AnalysisError> {
        let mut with = outer
            .map(ScalarAnalysisContext::inherited_ctes)
            .unwrap_or_default();
        for entry in &self.entries {
            if let Some(scope) = outer
                && let Some(identity) = scope.direct_rule_identity()
                && matches!(entry.name.as_str(), "old" | "new")
            {
                context.report_analysis_problem(AnalysisProblem::rule(
                    &RuleProblem::definition(identity, "CTE conflicts with rule OLD/NEW namespace"),
                ))?;
            }
            entry.body.require_data_modifying_placement(
                context,
                outer.is_none()
                    || outer.is_some_and(|scope| scope.direct_rule_identity().is_some()),
            )?;
            let from = FromClause::default();
            let restricted = outer.map(ScalarAnalysisContext::without_rule_rows);
            let scope = ScalarAnalysisContext::nested(&with, &from, restricted.as_ref());
            let body = entry.body.resolve(context, &scope, &entry.column_names())?;
            with.insert_resolved(context, &entry.name, body);
        }
        Ok(with)
    }
}

impl CteBody {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
    ) -> Result<CteStatement, AnalysisError> {
        match self {
            Self::Query(query) => Query::resolve_cte_body(context, scope, columns, query),
            Self::Insert(insert) => Insert::resolve_cte_body(context, scope, columns, insert),
            Self::Update(update) => Update::resolve_cte_body(context, scope, columns, update),
            Self::Delete(delete) => Delete::resolve_cte_body(context, scope, columns, delete),
            Self::Merge(merge) => Merge::resolve_cte_body(context, scope, columns, merge),
        }
    }
}

impl Merge {
    fn resolve_cte_body(
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
        merge: &Self,
    ) -> Result<CteStatement, AnalysisError> {
        Self::require_version(context, 17)?;
        let mut merge = merge.resolve_in(context, Some(scope))?;
        if columns.len() > merge.result_columns().len() {
            return Err(AnalysisError::unexpected_input_error(
                "MERGE CTE declares more aliases than RETURNING columns",
            ));
        }
        merge.rename_columns(columns);
        Ok(CteStatement::from_merge(merge))
    }
}

impl Delete {
    fn resolve_cte_body(
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
        delete: &Self,
    ) -> Result<CteStatement, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            context.report_analysis_problem(AnalysisProblem::delete(
                DeleteProblemKind::sqlite_cte(),
            ))?;
        }
        let mut delete = delete.resolve_in(context, Some(scope))?;
        if columns.len() > delete.result_columns().len() {
            context.report_analysis_problem(AnalysisProblem::delete(
                DeleteProblemKind::cte_aliases(),
            ))?;
        }
        delete.rename_columns(columns);
        Ok(CteStatement::from_delete(&delete))
    }
}

impl Update {
    fn resolve_cte_body(
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
        update: &Self,
    ) -> Result<CteStatement, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            return AnalysisError::raise_unexpected_input("SQLite UPDATE CTE");
        }
        let Some(mut update) = update.resolve_in(context, Some(scope))? else {
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

impl Insert {
    fn resolve_cte_body(
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
        insert: &Self,
    ) -> Result<CteStatement, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            return AnalysisError::raise_unexpected_input("SQLite INSERT CTE");
        }
        let Some(mut insert) = insert.resolve_in(context, Some(scope))? else {
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

impl Query {
    fn resolve_cte_body(
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
        query: &Self,
    ) -> Result<CteStatement, AnalysisError> {
        let mut query = query.resolve_in(context, Some(scope))?;
        query.rename_columns(columns);
        Ok(CteStatement::from_query(query))
    }
}

impl Alias {
    pub(in crate::ast) fn resolve(&self) -> AliasSpec {
        AliasSpec::from_names(&self.name, &self.columns)
    }
}

impl JoinKind {
    const fn resolve(&self) -> SemanticJoinKind {
        match self {
            Self::Inner | Self::Cross => SemanticJoinKind::inner(),
            Self::Left => SemanticJoinKind::left(),
            Self::Right => SemanticJoinKind::right(),
            Self::Full => SemanticJoinKind::full(),
        }
    }
}

impl FromItem {
    pub(in crate::ast) fn resolve(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        preceding: &FromClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<DataSource, AnalysisError> {
        let scope = ScalarAnalysisContext::nested(with, preceding, outer);
        self.resolve_in_scope(context, with, &scope)
    }

    fn resolve_in_scope(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSource, AnalysisError> {
        crate::sem::with_stack(|| self.resolve_body(context, with, scope))
    }

    #[inline(never)]
    fn resolve_body(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSource, AnalysisError> {
        match self {
            Self::Relation(relation) => relation.resolve(context, with, scope),
            Self::Query(derived) => derived.resolve(context, scope),
            Self::Join(join) => join.resolve(context, with, scope),
            Self::Functions(functions) => functions.resolve(context, scope),
            Self::Group(group) => group.resolve(context, with, scope),
            Self::Xml(table) => table.resolve(context, scope),
            Self::Sample(_) => todo!("table expression semantic analysis"),
            Self::Json(_) => AnalysisError::raise_unexpected_none("from_clause inner node"),
        }
    }
}

impl Join {
    fn using_clause(&self) -> Option<JoinUsingClause> {
        if self.using.is_empty() {
            return None;
        }
        let mut columns = Vec::new();
        for column in &self.using {
            columns.push(ColumnName::from(column.as_str()));
        }
        Some(JoinUsingClause::new(
            &columns,
            &self.using_alias.as_ref().map(Alias::resolve),
        ))
    }
}

struct TypedColumns<'a>(&'a [TypedColumn]);

impl<'a> From<&'a [TypedColumn]> for TypedColumns<'a> {
    fn from(columns: &'a [TypedColumn]) -> Self {
        Self(columns)
    }
}

impl TypedColumns<'_> {
    fn resolve_function_output(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<TableFunctionOutput, AnalysisError> {
        let mut output = TableFunctionOutput::default();
        for column in self.0 {
            let collation = column.collation.as_ref().map(Name::full_name).transpose()?;
            output.declare(
                context,
                &column.name,
                &column.type_name.reference()?,
                &column.type_name.modifier_arguments()?,
                collation.as_ref(),
            )?;
        }
        Ok(output)
    }
}

impl TableFunctions {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSource, AnalysisError> {
        if self.rows_from {
            let mut outer_output =
                TypedColumns::from(self.columns.as_slice()).resolve_function_output(context)?;
            let outer_allowed =
                self.calls.len() == 1 && !self.ordinality && self.calls[0].columns.is_empty();
            if !self.columns.is_empty() && !outer_allowed {
                outer_output.reject_placement(context)?;
            }
            let mut functions = Vec::new();
            for function in &self.calls {
                let local_output = TypedColumns::from(function.columns.as_slice())
                    .resolve_function_output(context)?;
                let output = if outer_allowed && !self.columns.is_empty() {
                    &outer_output
                } else {
                    &local_output
                };
                let mut source = function
                    .expression
                    .resolve_table_call(context, scope, None)?;
                source.apply_function_output(context, output)?;
                functions.push(DataSourceRowsFromFunc::invocation(&source));
            }
            let alias = self.alias.as_ref().map(Alias::resolve).unwrap_or_else(|| {
                AliasSpec::from_local_name(
                    &functions
                        .first()
                        .map(|function| function.default_alias())
                        .unwrap_or_default(),
                )
            });
            let mut source = DataSource::resolved_rows(&functions, &alias);
            source.retain_rows_definition(&outer_output);
            if self.ordinality {
                source.append_function_ordinality();
            }
            source.validate_function_aliases(context)?;
            return Ok(source);
        }
        let [function] = self.calls.as_slice() else {
            return AnalysisError::raise_unexpected_input("table function count");
        };
        let mut output =
            TypedColumns::from(self.columns.as_slice()).resolve_function_output(context)?;
        if self.ordinality && !self.columns.is_empty() {
            output.reject_placement(context)?;
        }
        let mut source =
            function
                .expression
                .resolve_table_call(context, scope, self.alias.as_ref())?;
        source.apply_function_output(context, &output)?;
        if self.ordinality {
            source.append_function_ordinality();
        }
        source.validate_function_aliases(context)?;
        Ok(source)
    }
}

impl Expr {
    pub(crate) fn resolve_rows(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSourceRowsFromFunc, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_rows_call(context, scope);
        context.finish_analysis(result)
    }
    fn resolve_rows_call(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSourceRowsFromFunc, AnalysisError> {
        match self {
            Self::Call(call) => {
                call.kind.require_table_function()?;
                let arguments = Self::resolve_list(&call.arguments, context, scope)?;
                if call.defers_function_signature() {
                    let mut function = call.resolve_supplied_call(context, &arguments)?;
                    function.set_modifiers(call.resolve_modifiers(context, scope)?);
                    return Ok(DataSourceRowsFromFunc::deferred(&function));
                }
                DataSourceRowsFromFunc::resolve(context, &call.name.full_name()?, &arguments)
            }
            Self::SqlValue(_) => todo!("ROWS FROM expression analysis"),
            _ => AnalysisError::raise_unexpected_input("ROWS FROM expression"),
        }
    }
    pub(in crate::ast) fn resolve_table_call(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        alias: Option<&Alias>,
    ) -> Result<DataSource, AnalysisError> {
        match self {
            Self::Call(call) => call.resolve_table(context, scope, alias),
            _ => AnalysisError::raise_unexpected_none("table expression is not FuncCall"),
        }
    }
}

impl Call {
    fn resolve_table(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        alias: Option<&Alias>,
    ) -> Result<DataSource, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            todo!("SQLite table function affinity and overload semantics");
        }
        self.kind.require_table_function()?;
        let arguments = Expr::resolve_list(&self.arguments, context, scope)?;
        let name = self.name.full_name()?;
        let alias = alias
            .map(Alias::resolve)
            .unwrap_or_else(|| AliasSpec::from_local_name(&name.local_name()));
        if self.defers_function_signature() {
            let mut call = self.resolve_supplied_call(context, &arguments)?;
            call.set_modifiers(self.resolve_modifiers(context, scope)?);
            return Ok(DataSource::deferred_table_function(&call, &alias));
        }
        DataSource::resolve_function(context, &name, &arguments, &alias)
    }
}

impl CallKind {
    fn require_table_function(&self) -> Result<(), AnalysisError> {
        match self {
            Self::Ordinary => Ok(()),
            // The old FROM boundary accepted FuncCall, not special scalar forms.
            Self::Coalesce | Self::MinMax => {
                AnalysisError::raise_unexpected_input("table expression is not FuncCall")
            }
        }
    }
}

impl FromItem {
    pub(in crate::ast) fn resolve_list_in(
        items: &[Self],
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<FromClause, AnalysisError> {
        let from = FromClause::default();
        let scope = ScalarAnalysisContext::nested(with, &from, outer);
        Self::resolve_list_in_scope(items, context, with, &scope)
    }

    fn resolve_list_in_scope(
        items: &[Self],
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<FromClause, AnalysisError> {
        let mut from = FromClause::default();
        for item in items {
            let source = item.resolve_in_scope(context, with, &scope.with_from_clause(&from))?;
            from.validate_source(context, &source)?;
            from.push(source);
        }

        Ok(from)
    }
    pub(crate) fn resolve_list(
        items: &[Self],
        context: &mut impl ParseContext,
        with: &WithClause,
    ) -> Result<FromClause, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::resolve_list_in(items, context, with, None);
        context.finish_analysis(result)
    }
    pub(crate) fn resolve_source(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        preceding: &FromClause,
    ) -> Result<DataSource, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve(context, with, preceding, None);
        context.finish_analysis(result)
    }
}
impl With {
    pub(crate) fn resolve_with(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<WithClause, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve(context, None);
        context.finish_analysis(result)
    }
}

impl Relation {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSource, AnalysisError> {
        let name = self.name.full_name()?;
        let name = if with.resolve_name(context, &name).is_none() {
            scope.trigger_relation_name(context, &name)?
        } else {
            name
        };
        let alias = self
            .alias
            .as_ref()
            .map(Alias::resolve)
            .unwrap_or_else(|| AliasSpec::from_local_name(&name.local_name()));
        let source = DataSource::resolve_relation(context, &name, &alias, with)?;
        Ok(source)
    }
}

impl DerivedTable {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSource, AnalysisError> {
        let non_lateral_scope;
        let scope = if self.lateral {
            scope
        } else {
            non_lateral_scope = scope.with_from_clause(&FromClause::default());
            &non_lateral_scope
        };
        let query = self.query.resolve_in(context, Some(scope))?;
        let alias = self
            .alias
            .as_ref()
            .map(Alias::resolve)
            .unwrap_or_else(|| AliasSpec::from_local_name(""));
        Ok(DataSource::resolved_query(&query, &alias))
    }
}

impl Join {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSource, AnalysisError> {
        let left = self.left.resolve_in_scope(context, with, scope)?;
        let visible = scope.with_added_source(&left);
        let right = self.right.resolve_in_scope(context, with, &visible)?;
        right
            .namespace()
            .validate_against(&left.namespace(), context)?;
        let scope = visible.with_added_source(&right);
        let condition = Expr::resolve_optional(
            &self.condition,
            context,
            &scope.with_grouping_placement("JOIN condition"),
        )?;
        Ok(DataSource::resolved_join(
            &left,
            &right,
            &self.kind.resolve(),
            &condition,
            &self.alias.as_ref().map(Alias::resolve),
            &self.using_clause(),
            self.natural,
        ))
    }
}

impl FromGroup {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSource, AnalysisError> {
        let Some((first, items)) = self.items.split_first() else {
            return AnalysisError::raise_unexpected_input("empty FROM group");
        };
        let mut source = first.resolve_in_scope(context, with, scope)?;
        for item in items {
            let right = item.resolve_in_scope(context, with, scope)?;
            source = DataSource::resolved_join(
                &source,
                &right,
                &SemanticJoinKind::inner(),
                &None,
                &None,
                &None,
                false,
            );
        }
        if let Some(alias) = &self.alias
            && Dialect::from(context.metadata()) == Dialect::sqlite()
        {
            source = source.with_group_alias(&alias.resolve());
        }
        Ok(source)
    }
}

impl SetQuery {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<SelectStatement, AnalysisError> {
        let from = FromClause::default();
        let scope = ScalarAnalysisContext::nested(with, &from, outer);
        let left = self.left.resolve_in(context, Some(&scope))?;
        let right = self.right.resolve_in(context, Some(&scope))?;
        Ok(self.operation.resolve(left, right))
    }
}

impl QueryBody {
    const fn is_compound(&self) -> bool {
        matches!(self, Self::Set(_))
    }
    fn window_names(&self) -> Vec<String> {
        match self {
            Self::Select(select) => Window::names(&select.windows),
            _ => Vec::new(),
        }
    }
    const fn order_binding(&self) -> ClauseBinding {
        match self {
            Self::Set(_) => ClauseBinding::Compound,
            _ => ClauseBinding::Order,
        }
    }
    fn set_all(&self) -> bool {
        match self {
            Self::Set(set) => set.all,
            _ => false,
        }
    }
}

impl Call {
    pub(super) fn resolve_modifiers(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<FunctionModifiers, AnalysisError> {
        let columns = ResultColumnCollection::default();
        let order =
            Order::resolve_list(&self.order, context, scope, &columns, ClauseBinding::Source)?;
        let within_order = Order::resolve_list(
            &self.within_order,
            context,
            scope,
            &columns,
            ClauseBinding::Source,
        )?;
        let filter = Expr::resolve_optional(
            &self.filter,
            context,
            &scope.with_grouping_placement("aggregate FILTER"),
        )?;
        let over = self
            .over
            .as_ref()
            .map(|window| window.resolve_over(context, scope))
            .transpose()?;
        let (within_order, order) = if self.within_group && within_order.is_empty() {
            (order, Vec::new())
        } else {
            (within_order, order)
        };
        let mut modifiers = FunctionModifiers::new(within_order, order, filter, over);
        modifiers.set_flags(self.distinct, self.star, self.variadic, self.within_group);
        Ok(modifiers)
    }
}

impl Into {
    fn resolve_destination(&self) -> Result<SelectDestination, AnalysisError> {
        Ok(SelectDestination::new(
            &self.relation.name.full_name()?,
            &self.relation.persistence,
        ))
    }
}
