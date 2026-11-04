use crate::sem::{AstConversion, convert};
mod clauses;

use crate::{
    CacheContext,
    ast::{Cast, OperatorExpr, OperatorKind, SqlValue, TableSample},
    sem::{SetOperationKind, TableSampleSource, with_stack},
};

use self::clauses::ClauseBinding;

use crate::{
    ast::{
        Alias, Call, CteBody, Delete, DerivedTable, Dialect, Expr, FromGroup, FromItem, Grouping,
        Insert, Into, Join, JoinKind, Lock, Merge, Name, Order, Query, QueryBody, Relation, Select,
        SetOperation, SetQuery, TableFunctions, Target, TypedColumn, Update, Window, With,
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

impl AstConversion<'_, Query> {
    #[cfg(test)]
    pub(crate) fn resolve_test_in(
        &self,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<SelectStatement, AnalysisError> {
        self.resolve_in(context, outer)
    }

    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        cache: &mut CacheContext,
    ) -> Result<SelectStatement, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context, None);
        let result = context.infer_query_parameter_types(result, cache);
        context.finish_analysis(result)
    }

    pub(crate) fn resolve_in(
        &self,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<SelectStatement, AnalysisError> {
        with_stack(|| Self::resolve_in_body(self, context, outer))
    }

    #[inline(never)]
    pub(crate) fn resolve_in_body(
        &self,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<SelectStatement, AnalysisError> {
        self.resolve_query_body(context, outer)
    }
}

#[cfg(test)]
mod grouping_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod test_name_resolution_phase {
    use crate::sem::convert;

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
        let result = convert(&query).resolve_in(&mut catalog, None);
        catalog.finish_analysis(result).unwrap();
        assert_eq!(catalog.reported_problem_count(), 0);
        assert_eq!(crate::sem::parameter_resolution::inference_runs(), before);
    }
}

impl AstConversion<'_, Query> {
    pub(crate) fn resolve_query_body(
        &self,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<SelectStatement, AnalysisError> {
        let with = convert(&self.ast.with).resolve(context, outer)?;
        let restricted = if convert(&self.ast.body).is_compound()
            && outer.is_some_and(|scope| scope.direct_rule_identity().is_some())
        {
            outer.map(ScalarAnalysisContext::without_rule_rows)
        } else {
            None
        };
        let query_id = context.analysis_state_mut().grouping_mut().new_query();
        let body = convert(&self.ast.body).resolve(
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
        let order_outer = if convert(&self.ast.body).is_compound() {
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
        scope.set_windows(convert(&self.ast.body).window_names());
        if context.dialect() == Dialect::sqlite() {
            scope.set_output_aliases(body.result_columns());
        }
        let binding = convert(&self.ast.body).order_binding();
        let order = AstConversion::<Order>::resolve_for_query(
            &self.ast.order,
            &self.ast.body,
            context,
            &scope,
            body.result_columns(),
            binding,
        )?;
        let row_scope = ScalarAnalysisContext::nested(&with, &empty, outer);
        let row_scope = row_scope.with_grouping_placement("LIMIT/OFFSET");
        let limit = AstConversion::<Expr>::resolve_optional(&self.ast.limit, context, &row_scope)?;
        let offset =
            AstConversion::<Expr>::resolve_optional(&self.ast.offset, context, &row_scope)?;
        let locks = AstConversion::<Lock>::resolve_list(&self.ast.locks, context, &scope)?;
        let local = convert(&self.ast.with).local_definitions(with);
        let mut query =
            SemanticQuery::new(local, self.ast.with.recursive, body, order, limit, offset);
        query.set_modifiers(self.ast.with_ties, convert(&self.ast.body).set_all(), locks);
        let mut query = SelectStatement::query(query);
        query.complete_grouping(context)?;
        Ok(query)
    }
}

impl AstConversion<'_, QueryBody> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
        query_id: usize,
    ) -> Result<SelectStatement, AnalysisError> {
        match self.ast {
            QueryBody::Select(select) => {
                convert(select.as_ref()).resolve(context, with, outer, query_id)
            }
            QueryBody::Set(set) => convert(set.as_ref()).resolve(context, with, outer),
            QueryBody::Values(rows) => Self::resolve_values(rows, context, with, outer, query_id),
        }
    }

    pub(crate) fn resolve_values(
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
            resolved.push(AstConversion::<Expr>::resolve_list(row, context, &scope)?);
        }
        Ok(SelectStatement::values(resolved))
    }
}

impl AstConversion<'_, SetOperation> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        left: SelectStatement,
        right: SelectStatement,
    ) -> Result<SelectStatement, AnalysisError> {
        let kind = match self.ast {
            SetOperation::Union => SetOperationKind::union(),
            SetOperation::Intersect => SetOperationKind::intersect(),
            SetOperation::Except => SetOperationKind::except(),
        };
        SelectStatement::set_operation(context, kind, left, right)
    }
}

impl AstConversion<'_, Select> {
    pub(crate) fn resolve(
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

    pub(crate) fn validate_rule_destination(
        &self,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<(), AnalysisError> {
        if self.ast.into.is_some()
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

    pub(crate) fn resolve_from(
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
        let from = AstConversion::<FromItem>::resolve_list_in_scope(
            &self.ast.from,
            context,
            with,
            &from_scope,
        )?;
        if let Some(scope) = outer {
            scope.validate_rule_sources(context, &from)?;
        }
        Ok(from)
    }

    pub(crate) fn scope<'a>(
        &self,
        with: &'a WithClause,
        from: &'a FromClause,
        outer: Option<&ScalarAnalysisContext<'a>>,
        query_id: usize,
    ) -> ScalarAnalysisContext<'a> {
        let mut scope = ScalarAnalysisContext::nested(with, from, outer);
        scope.set_grouping_query(query_id);
        scope.set_windows(AstConversion::<Window>::names(&self.ast.windows));
        scope
    }

    pub(crate) fn resolve_windows(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Vec<QueryWindow>, AnalysisError> {
        AstConversion::<Window>::resolve_definitions(&self.ast.windows, context, scope)
    }

    pub(crate) fn resolve_where(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        AstConversion::<Expr>::resolve_optional(
            &self.ast.predicate,
            context,
            &scope.with_grouping_placement("WHERE"),
        )
    }

    pub(crate) fn resolve_targets(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        from: &FromClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<ResultColumnCollection, AnalysisError> {
        let mut results = ResultColumnCollection::default();
        for target in &self.ast.targets {
            convert(target).append_to_results(context, with, from, scope, &mut results)?;
        }
        Ok(results)
    }

    pub(crate) fn resolve_distinct(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        results: &ResultColumnCollection,
    ) -> Result<Option<Vec<SemScalarExpr>>, AnalysisError> {
        let distinct = if let Some(distinct) = &self.ast.distinct {
            let mut resolved = Vec::new();
            for expression in distinct {
                resolved.push(convert(expression).resolve_clause(
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

    pub(crate) fn resolve_grouping(
        &self,
        context: &mut impl ParseContext,
        scope: &mut ScalarAnalysisContext<'_>,
        results: &mut ResultColumnCollection,
        query_id: usize,
    ) -> Result<Vec<QueryGrouping>, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            scope.set_output_aliases(results);
        }
        let group =
            AstConversion::<Grouping>::resolve_list(&self.ast.group, context, scope, results)?;
        for grouping in &group {
            grouping.validate_grouping_targets(results, query_id, context)?;
        }
        context
            .analysis_state_mut()
            .grouping_mut()
            .set_keys(query_id, &group, results)?;
        Ok(group)
    }

    pub(crate) fn resolve_having(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        AstConversion::<Expr>::resolve_optional(&self.ast.having, context, scope)
    }

    pub(crate) fn resolve_clauses(
        &self,
        distinct: Option<Vec<SemScalarExpr>>,
        group: Vec<QueryGrouping>,
        having: Option<SemScalarExpr>,
        windows: Vec<QueryWindow>,
    ) -> Result<SelectClauses, AnalysisError> {
        let mut clauses =
            SelectClauses::new(distinct, group, self.ast.group_distinct, having, windows);
        let destination = self
            .ast
            .into
            .as_ref()
            .map(|into| convert(into).resolve_destination())
            .transpose()?;
        clauses.set_destination(destination);
        Ok(clauses)
    }

    pub(crate) fn build_result(
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

impl AstConversion<'_, Target> {
    pub(crate) fn append_to_results(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        from: &FromClause,
        scope: &ScalarAnalysisContext<'_>,
        results: &mut ResultColumnCollection,
    ) -> Result<(), AnalysisError> {
        if let Some(name) = self.ast.source_star_name() {
            return self.append_source_star(name, context, with, from, scope, results);
        }
        if let Some(base) = convert(&self.ast.expression).projection_base(context, scope)? {
            return results.append_composite_star(context, base);
        }
        let expression = convert(&self.ast.expression).resolve_in(context, scope)?;
        expression.validate_scalar_value(context)?;
        let name = self
            .ast
            .alias
            .clone()
            .unwrap_or_else(|| expression.get_column_name());
        results.push_projection(
            &ColumnName::from(name.as_str()),
            expression,
            self.ast.alias.is_some() && !self.ast.implicit_alias,
        );
        Ok(())
    }

    pub(crate) fn append_source_star(
        &self,
        name: &Name,
        context: &mut impl ParseContext,
        with: &WithClause,
        from: &FromClause,
        scope: &ScalarAnalysisContext<'_>,
        results: &mut ResultColumnCollection,
    ) -> Result<(), AnalysisError> {
        let relation = convert(name).star_relation()?;
        let qualifier = if relation.is_none() {
            convert(name).star_qualifier()?
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
            let base = convert(name).resolve_column(context, scope)?;
            return results.append_composite_star(context, base);
        }
        if !source_found {
            context.report_analysis_problem(AnalysisProblem::query(
                QueryProblem::reference_not_found("star source"),
            ))?;
        }
        let start = results.len();
        projection.append_results(context, scope, results, self.ast.location, None)?;
        if relation.is_none() && qualifier.is_none() {
            results.mark_join_sensitive_star_since(start);
        }
        Ok(())
    }
}

#[cfg(test)]
mod wildcard_tests;

impl AstConversion<'_, With> {
    pub(crate) fn local_definitions(&self, mut visible: WithClause) -> Vec<QueryCte> {
        let mut local = Vec::new();
        for entry in &self.ast.entries {
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
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<WithClause, AnalysisError> {
        let mut with = outer
            .map(ScalarAnalysisContext::inherited_ctes)
            .unwrap_or_default();
        for entry in &self.ast.entries {
            if let Some(scope) = outer
                && let Some(identity) = scope.direct_rule_identity()
                && matches!(entry.name.as_str(), "old" | "new")
            {
                context.report_analysis_problem(AnalysisProblem::rule(
                    &RuleProblem::definition(identity, "CTE conflicts with rule OLD/NEW namespace"),
                ))?;
            }
            convert(&entry.body).require_data_modifying_placement(
                context,
                outer.is_none()
                    || outer.is_some_and(|scope| scope.direct_rule_identity().is_some()),
            )?;
            let from = FromClause::default();
            let restricted = outer.map(ScalarAnalysisContext::without_rule_rows);
            let scope = ScalarAnalysisContext::nested(&with, &from, restricted.as_ref());
            let body = convert(&entry.body).resolve(context, &scope, &entry.column_names())?;
            with.insert_resolved(context, &entry.name, body);
        }
        Ok(with)
    }
}

impl AstConversion<'_, CteBody> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
    ) -> Result<CteStatement, AnalysisError> {
        if !self.check_engine_features(context)? {
            return Ok(CteStatement::unresolved());
        }
        match self.ast {
            CteBody::Query(query) => {
                AstConversion::<Query>::resolve_cte_body(context, scope, columns, query)
            }
            CteBody::Insert(insert) => {
                AstConversion::<Insert>::resolve_cte_body(context, scope, columns, insert)
            }
            CteBody::Update(update) => {
                AstConversion::<Update>::resolve_cte_body(context, scope, columns, update)
            }
            CteBody::Delete(delete) => {
                AstConversion::<Delete>::resolve_cte_body(context, scope, columns, delete)
            }
            CteBody::Merge(merge) => {
                AstConversion::<Merge>::resolve_cte_body(context, scope, columns, merge)
            }
        }
    }
}

impl AstConversion<'_, Merge> {
    pub(crate) fn resolve_cte_body(
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
        merge: &Merge,
    ) -> Result<CteStatement, AnalysisError> {
        Self::require_version(context, 17)?;
        let mut merge = convert(merge).resolve_in(context, Some(scope))?;
        if columns.len() > merge.result_columns().len() {
            return Err(AnalysisError::unexpected_input_error(
                "MERGE CTE declares more aliases than RETURNING columns",
            ));
        }
        merge.rename_columns(columns);
        Ok(CteStatement::from_merge(merge))
    }
}

impl AstConversion<'_, Delete> {
    pub(crate) fn resolve_cte_body(
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
        let mut delete = convert(delete).resolve_in(context, Some(scope))?;
        if columns.len() > delete.result_columns().len() {
            context.report_analysis_problem(AnalysisProblem::delete(
                DeleteProblemKind::cte_aliases(),
            ))?;
        }
        delete.rename_columns(columns);
        Ok(CteStatement::from_delete(&delete))
    }
}

impl AstConversion<'_, Update> {
    pub(crate) fn resolve_cte_body(
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
        update: &Update,
    ) -> Result<CteStatement, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            return AnalysisError::raise_unexpected_input("SQLite UPDATE CTE");
        }
        let Some(mut update) = convert(update).resolve_in(context, Some(scope))? else {
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

impl AstConversion<'_, Insert> {
    pub(crate) fn resolve_cte_body(
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
        insert: &Insert,
    ) -> Result<CteStatement, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            return AnalysisError::raise_unexpected_input("SQLite INSERT CTE");
        }
        let Some(mut insert) = convert(insert).resolve_in(context, Some(scope))? else {
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

impl AstConversion<'_, Query> {
    pub(crate) fn resolve_cte_body(
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
        query: &Query,
    ) -> Result<CteStatement, AnalysisError> {
        let mut query = convert(query).resolve_in(context, Some(scope))?;
        query.rename_columns(columns);
        Ok(CteStatement::from_query(query))
    }
}

impl AstConversion<'_, Alias> {
    pub(crate) fn resolve(&self) -> AliasSpec {
        AliasSpec::from_names(&self.ast.name, &self.ast.columns)
    }
}

impl AstConversion<'_, JoinKind> {
    pub(crate) const fn resolve(&self) -> SemanticJoinKind {
        match self.ast {
            JoinKind::Inner | JoinKind::Cross => SemanticJoinKind::inner(),
            JoinKind::Left => SemanticJoinKind::left(),
            JoinKind::Right => SemanticJoinKind::right(),
            JoinKind::Full => SemanticJoinKind::full(),
        }
    }
}

impl AstConversion<'_, FromItem> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        preceding: &FromClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<DataSource, AnalysisError> {
        let scope = ScalarAnalysisContext::nested(with, preceding, outer);
        self.resolve_in_scope(context, with, &scope)
    }

    pub(crate) fn resolve_in_scope(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSource, AnalysisError> {
        with_stack(|| self.resolve_body(context, with, scope))
    }

    #[inline(never)]
    pub(crate) fn resolve_body(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSource, AnalysisError> {
        match self.ast {
            FromItem::Relation(relation) => convert(relation).resolve(context, with, scope),
            FromItem::Query(derived) => convert(derived.as_ref()).resolve(context, scope),
            FromItem::Join(join) => convert(join.as_ref()).resolve(context, with, scope),
            FromItem::Functions(functions) => convert(functions).resolve(context, scope),
            FromItem::Group(group) => convert(group.as_ref()).resolve(context, with, scope),
            FromItem::Xml(table) => convert(table.as_ref()).resolve(context, scope),
            FromItem::Sample(sample) => convert(sample.as_ref()).resolve(context, with, scope),
            FromItem::Json(_) => AnalysisError::raise_unexpected_none("from_clause inner node"),
        }
    }
}

impl AstConversion<'_, Join> {
    pub(crate) fn using_clause(&self) -> Option<JoinUsingClause> {
        if self.ast.using.is_empty() {
            return None;
        }
        let mut columns = Vec::new();
        for column in &self.ast.using {
            columns.push(ColumnName::from(column.as_str()));
        }
        Some(JoinUsingClause::new(
            &columns,
            &self
                .ast
                .using_alias
                .as_ref()
                .map(|value| convert(value).resolve()),
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
            let collation = column
                .collation
                .as_ref()
                .map(|value| convert(value).full_name())
                .transpose()?;
            output.declare(
                context,
                &column.name,
                &convert(&column.type_name).reference()?,
                &convert(&column.type_name).modifier_arguments()?,
                collation.as_ref(),
            )?;
        }
        Ok(output)
    }
}

impl AstConversion<'_, TableFunctions> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSource, AnalysisError> {
        if self.ast.rows_from {
            return self.resolve_rows_from(context, scope);
        }
        let [function] = self.ast.calls.as_slice() else {
            return AnalysisError::raise_unexpected_input("table function count");
        };
        let mut output =
            TypedColumns::from(self.ast.columns.as_slice()).resolve_function_output(context)?;
        if self.ast.ordinality && !self.ast.columns.is_empty() {
            output.reject_placement(context)?;
        }
        let mut source = convert(&function.expression).resolve_table_call(
            context,
            scope,
            self.ast
                .alias
                .as_ref()
                .map(|value| convert(value).resolve())
                .as_ref(),
        )?;
        source.apply_function_output(context, &output)?;
        if self.ast.ordinality {
            source.append_function_ordinality();
        }
        source.validate_function_aliases(context)?;
        Ok(source)
    }
    pub(crate) fn resolve_rows_from(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSource, AnalysisError> {
        let mut outer_output =
            TypedColumns::from(self.ast.columns.as_slice()).resolve_function_output(context)?;
        let outer_allowed = self.ast.calls.len() == 1
            && !self.ast.ordinality
            && self.ast.calls[0].columns.is_empty();
        if !self.ast.columns.is_empty() && !outer_allowed {
            outer_output.reject_placement(context)?;
        }
        let invocation_alias = if self.ast.calls.len() == 1 {
            self.ast
                .alias
                .as_ref()
                .map(|value| AliasSpec::from_names(&value.name, &[]))
        } else {
            None
        };
        let mut functions = Vec::new();
        for function in &self.ast.calls {
            let local_output =
                TypedColumns::from(function.columns.as_slice()).resolve_function_output(context)?;
            let output = if outer_allowed && !self.ast.columns.is_empty() {
                &outer_output
            } else {
                &local_output
            };
            let mut function = convert(&function.expression).resolve_rows_call(
                context,
                scope,
                invocation_alias.as_ref(),
            )?;
            function.apply_output(context, output)?;
            functions.push(function);
        }
        let alias = self
            .ast
            .alias
            .as_ref()
            .map(|value| convert(value).resolve())
            .unwrap_or_else(|| {
                AliasSpec::from_local_name(
                    &functions
                        .first()
                        .map(|function| function.default_alias())
                        .unwrap_or_default(),
                )
            });
        let mut source = DataSource::resolved_rows(&functions, &alias);
        source.retain_rows_definition(&outer_output);
        if self.ast.ordinality {
            source.append_function_ordinality();
        }
        source.validate_function_aliases(context)?;
        Ok(source)
    }
}

impl AstConversion<'_, Expr> {
    pub(crate) fn resolve_rows(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSourceRowsFromFunc, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self
            .resolve_rows_call(context, scope, None)
            .and_then(|mut source| {
                source.apply_output(context, &TableFunctionOutput::default())?;
                Ok(source)
            });
        context.finish_analysis(result)
    }
    pub(crate) fn resolve_rows_call(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        alias: Option<&AliasSpec>,
    ) -> Result<DataSourceRowsFromFunc, AnalysisError> {
        if let Expr::Call(call) = self.ast
            && convert(&call.kind).is_ordinary()
        {
            return Ok(DataSourceRowsFromFunc::invocation(
                &convert(call.as_ref()).resolve_table(context, scope, alias)?,
            ));
        }
        let value = self.resolve_from_expression(context, scope)?;
        let name = if let Some(alias) = alias {
            alias.name().to_string()
        } else {
            self.table_output_name(&value)?
        };
        DataSourceRowsFromFunc::expression(&value, &name, context)
    }
    pub(crate) fn resolve_from_expression(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match self.ast {
            Expr::Call(_) | Expr::SqlValue(_) | Expr::Cast(_) => self.resolve_in(context, scope),
            Expr::Operator(operator) if convert(&operator.kind).is_null_if() => {
                self.resolve_in(context, scope)
            }
            _ => AnalysisError::raise_unexpected_input("ROWS FROM expression"),
        }
    }
    pub(crate) fn table_output_name(&self, value: &SemScalarExpr) -> Result<String, AnalysisError> {
        match self.ast {
            Expr::Call(call) => convert(call.as_ref()).table_output_name(),
            Expr::SqlValue(value) => Ok(convert(value).table_output_name()),
            Expr::Column(name) => convert(name).table_output_name(),
            Expr::Operator(operator) if convert(&operator.kind).is_null_if() => {
                Ok(convert(operator.as_ref()).table_output_name())
            }
            Expr::Cast(cast) => convert(cast.as_ref()).table_output_name(value),
            _ => Ok(convert(value).table_output_name()),
        }
    }
    pub(crate) fn resolve_expression_table(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        alias: Option<&AliasSpec>,
    ) -> Result<DataSource, AnalysisError> {
        let function = self.resolve_rows_call(context, scope, alias)?;
        let alias = alias
            .cloned()
            .unwrap_or_else(|| AliasSpec::from_local_name(&function.default_alias()));
        Ok(DataSource::resolved_rows(&[function], &alias))
    }

    pub(crate) fn resolve_table_call(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        alias: Option<&AliasSpec>,
    ) -> Result<DataSource, AnalysisError> {
        match self.ast {
            Expr::Call(call) if convert(&call.kind).is_ordinary() => {
                convert(call.as_ref()).resolve_table(context, scope, alias)
            }
            _ => self.resolve_expression_table(context, scope, alias),
        }
    }
}

impl AstConversion<'_, Call> {
    pub(crate) fn resolve_table(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        alias: Option<&AliasSpec>,
    ) -> Result<DataSource, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            todo!("SQLite table function affinity and overload semantics");
        }
        let mut arguments =
            AstConversion::<Expr>::resolve_list(&self.ast.arguments, context, scope)?;
        let mut invalid_arguments = false;
        for argument in &mut arguments {
            invalid_arguments |= DataSourceRowsFromFunc::validate_argument(context, argument)?;
        }
        let name = convert(&self.ast.name).full_name()?;
        let alias = alias
            .cloned()
            .unwrap_or_else(|| AliasSpec::from_local_name(&name.local_name()));
        if self.uses_supplied_binding() {
            let mut call = self.resolve_supplied_call(context, &arguments)?;
            call.set_modifiers(self.resolve_modifiers(context, scope)?);
            let mut source = DataSource::deferred_table_function(context, &call, &alias);
            source.validate_table_function(context, invalid_arguments)?;
            return Ok(source);
        }
        let mut source = DataSource::resolve_function(context, &name, &arguments, &alias)?;
        source.validate_table_function(context, invalid_arguments)?;
        Ok(source)
    }
}

impl AstConversion<'_, OperatorKind> {
    pub(crate) const fn is_null_if(&self) -> bool {
        matches!(self.ast, OperatorKind::NullIf)
    }
}

impl AstConversion<'_, FromItem> {
    pub(crate) fn resolve_list_in(
        items: &[FromItem],
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<FromClause, AnalysisError> {
        let from = FromClause::default();
        let scope = ScalarAnalysisContext::nested(with, &from, outer);
        Self::resolve_list_in_scope(items, context, with, &scope)
    }

    pub(crate) fn resolve_list_in_scope(
        items: &[FromItem],
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<FromClause, AnalysisError> {
        let mut from = FromClause::default();
        for item in items {
            let source =
                convert(item).resolve_in_scope(context, with, &scope.with_from_clause(&from))?;
            from.validate_source(context, &source)?;
            from.push(source);
        }

        Ok(from)
    }
    pub(crate) fn resolve_list(
        items: &[FromItem],
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
impl AstConversion<'_, With> {
    pub(crate) fn resolve_with(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<WithClause, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve(context, None);
        context.finish_analysis(result)
    }
}

impl AstConversion<'_, Relation> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSource, AnalysisError> {
        let name = convert(&self.ast.name).full_name()?;
        let name = if with.resolve_name(context, &name).is_none() {
            scope.trigger_relation_name(context, &name)?
        } else {
            name
        };
        let alias = self
            .ast
            .alias
            .as_ref()
            .map(|value| convert(value).resolve())
            .unwrap_or_else(|| AliasSpec::from_local_name(&name.local_name()));
        let source = DataSource::resolve_relation(context, &name, &alias, with)?;
        Ok(source)
    }
}

impl AstConversion<'_, DerivedTable> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSource, AnalysisError> {
        let non_lateral_scope;
        let scope = if self.ast.lateral {
            scope
        } else {
            non_lateral_scope = scope.with_from_clause(&FromClause::default());
            &non_lateral_scope
        };
        let query = convert(&self.ast.query).resolve_in(context, Some(scope))?;
        let alias = self
            .ast
            .alias
            .as_ref()
            .map(|value| convert(value).resolve())
            .unwrap_or_else(|| AliasSpec::from_local_name(""));
        Ok(DataSource::resolved_query(&query, &alias))
    }
}

impl AstConversion<'_, Join> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSource, AnalysisError> {
        let left = convert(&self.ast.left).resolve_in_scope(context, with, scope)?;
        let visible = scope.with_added_source(&left);
        let right = convert(&self.ast.right).resolve_in_scope(context, with, &visible)?;
        right
            .namespace()
            .validate_against(&left.namespace(), context)?;
        let scope = visible.with_added_source(&right);
        let condition = AstConversion::<Expr>::resolve_optional(
            &self.ast.condition,
            context,
            &scope.with_grouping_placement("JOIN condition"),
        )?;
        Ok(DataSource::resolved_join(
            &left,
            &right,
            &convert(&self.ast.kind).resolve(),
            &condition,
            &self
                .ast
                .alias
                .as_ref()
                .map(|value| convert(value).resolve()),
            &self.using_clause(),
            self.ast.natural,
        ))
    }
}

impl AstConversion<'_, FromGroup> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSource, AnalysisError> {
        let Some((first, items)) = self.ast.items.split_first() else {
            return AnalysisError::raise_unexpected_input("empty FROM group");
        };
        let mut source = convert(first).resolve_in_scope(context, with, scope)?;
        for item in items {
            let right = convert(item).resolve_in_scope(context, with, scope)?;
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
        if let Some(alias) = &self.ast.alias
            && Dialect::from(context.metadata()) == Dialect::sqlite()
        {
            source = source.with_group_alias(&convert(alias).resolve());
        }
        Ok(source)
    }
}

impl AstConversion<'_, SetQuery> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<SelectStatement, AnalysisError> {
        let from = FromClause::default();
        let scope = ScalarAnalysisContext::nested(with, &from, outer);
        let left = convert(&self.ast.left).resolve_in(context, Some(&scope))?;
        let right = convert(&self.ast.right).resolve_in(context, Some(&scope))?;
        convert(&self.ast.operation).resolve(context, left, right)
    }
}

impl AstConversion<'_, QueryBody> {
    pub(crate) const fn is_compound(&self) -> bool {
        matches!(self.ast, QueryBody::Set(_))
    }
    pub(crate) fn window_names(&self) -> Vec<String> {
        match self.ast {
            QueryBody::Select(select) => AstConversion::<Window>::names(&select.windows),
            _ => Vec::new(),
        }
    }
    pub(crate) const fn order_binding(&self) -> ClauseBinding {
        match self.ast {
            QueryBody::Set(_) => ClauseBinding::Compound,
            _ => ClauseBinding::Order,
        }
    }
    pub(crate) fn set_all(&self) -> bool {
        match self.ast {
            QueryBody::Set(set) => set.all,
            _ => false,
        }
    }
}

impl AstConversion<'_, Call> {
    pub(crate) fn resolve_modifiers(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<FunctionModifiers, AnalysisError> {
        let columns = ResultColumnCollection::default();
        let order = AstConversion::<Order>::resolve_list(
            &self.ast.order,
            context,
            scope,
            &columns,
            ClauseBinding::Source,
        )?;
        let within_order = AstConversion::<Order>::resolve_list(
            &self.ast.within_order,
            context,
            scope,
            &columns,
            ClauseBinding::Source,
        )?;
        let filter = AstConversion::<Expr>::resolve_optional(
            &self.ast.filter,
            context,
            &scope.with_grouping_placement("aggregate FILTER"),
        )?;
        let over = self
            .ast
            .over
            .as_ref()
            .map(|window| convert(window).resolve_over(context, scope))
            .transpose()?;
        let (within_order, order) = if self.ast.within_group && within_order.is_empty() {
            (order, Vec::new())
        } else {
            (within_order, order)
        };
        let mut modifiers = FunctionModifiers::new(within_order, order, filter, over);
        modifiers.set_flags(
            self.ast.distinct,
            self.ast.star,
            self.ast.variadic,
            self.ast.within_group,
        );
        Ok(modifiers)
    }
}

impl AstConversion<'_, Into> {
    pub(crate) fn resolve_destination(&self) -> Result<SelectDestination, AnalysisError> {
        Ok(SelectDestination::new(
            &convert(&self.ast.relation.name).full_name()?,
            &self.ast.relation.persistence,
        ))
    }
}

impl AstConversion<'_, TableSample> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSource, AnalysisError> {
        let source = convert(&self.ast.relation).resolve_in_scope(context, with, scope)?;
        let expression_scope = scope.with_from_clause(&FromClause::default());
        let arguments = AstConversion::<Expr>::resolve_list(
            &self.ast.arguments,
            context,
            &expression_scope.with_grouping_placement("TABLESAMPLE"),
        )?;
        let repeatable = AstConversion::<Expr>::resolve_optional(
            &self.ast.repeatable,
            context,
            &expression_scope.with_grouping_placement("REPEATABLE"),
        )?;
        let sample = TableSampleSource::resolve(
            context,
            &source,
            &convert(&self.ast.method).full_name()?,
            &arguments,
            &repeatable,
        )?;
        Ok(DataSource::sampled(&sample))
    }
}

impl AstConversion<'_, Cast> {
    pub(crate) fn table_output_name(&self, value: &SemScalarExpr) -> Result<String, AnalysisError> {
        let name = convert(&self.ast.argument)
            .table_output_name(value.explicit_cast_operand().unwrap_or(value))?;
        if name.is_empty() {
            Ok(convert(&convert(&self.ast.target).reference()?)
                .full_name()
                .expect("SQL type syntax constructs a concrete reference")
                .local_name())
        } else {
            Ok(name)
        }
    }
}
impl AstConversion<'_, Call> {
    pub(crate) fn table_output_name(&self) -> Result<String, AnalysisError> {
        convert(&self.ast.name).table_output_name()
    }
}
impl AstConversion<'_, Name> {
    pub(crate) fn table_output_name(&self) -> Result<String, AnalysisError> {
        Ok(self.full_name()?.local_name())
    }
}
impl AstConversion<'_, SqlValue> {
    pub(crate) fn table_output_name(&self) -> String {
        self.ast.name.clone()
    }
}
impl AstConversion<'_, OperatorExpr> {
    pub(crate) fn table_output_name(&self) -> String {
        "nullif".to_owned()
    }
}
impl SemScalarExpr {
    fn table_output_name(&self) -> String {
        self.get_column_name()
    }
}
