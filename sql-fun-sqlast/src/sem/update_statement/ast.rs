use crate::{
    CacheContext,
    ast::{Assignment, Dialect, Expr, Update},
    context::ParseContext,
    names::{ColumnName, FullName, TableName},
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, ColumnDefinition,
        CreateTrigger as SemCreateTrigger, CteStatement, DataSource, FromClause, IndirectionExpr,
        InsertTargetCollection, PgBuiltInType, ResolutionFailure, ResultColumnCollection,
        ScalarAnalysisContext, SemAst, SemScalarExpr, UpdateAssignment, UpdateAssignmentCollection,
        UpdateCursor, UpdateDestination, UpdatePredicate, UpdateStatement, UpdateValue, WithClause,
        parameter_resolution::ParameterResolution, problem::UpdateProblemKind,
    },
};

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

#[cfg(test)]
mod test_nested_update_cte_reports_a_query_problem {
    use crate::{ast::Dialect, test_helpers::update::analyze};

    #[test]
    fn nested_update_cte_reports_a_query_problem() {
        let (_, context) = analyze(
            "SELECT (WITH u AS (UPDATE items SET id=1 RETURNING id) SELECT id FROM u)",
            Dialect::postgres(),
        );
        assert!(context.reported_problem_count() > 0);
    }
}

impl UpdateStatement {
    pub(crate) fn from_ast_trigger_command(
        ast: &Update,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemAst, ResolutionFailure> {
        DataSource::validate_ast_trigger_command(ast.relation())?;
        if !ast.returning().is_empty() {
            return SemCreateTrigger::ast_invalid(
                &FullName::from_ast_name(ast.relation().name())?,
                "SQLite trigger UPDATE does not support RETURNING",
            );
        }
        Self::from_ast_in(ast, context, Some(scope))
            .map(|statement| {
                statement
                    .as_ref()
                    .map_or_else(SemAst::unresolved_statement, SemAst::update)
            })
            .map_err(Into::into)
    }
}

impl UpdateStatement {
    pub(crate) fn from_ast(
        ast: &Update,
        context: &mut impl ParseContext,
        cache: &mut CacheContext,
    ) -> Result<Option<Self>, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_in(ast, context, None).and_then(|value| {
            let Some(mut statement) = value else {
                return Ok(None);
            };
            if context.parameter_inference_needed() {
                let constraints =
                    ParameterResolution::infer_update(&mut statement, context, cache)?;
                context.set_parameter_constraints(&constraints);
            }
            Ok(Some(statement))
        });
        context.finish_analysis(result)
    }
}

#[cfg(test)]
mod test_preserves_default_and_resets_parameters {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        parse_statements,
        sem::SemAst,
        test_helpers::{create_cache_context, update::catalog},
    };

    #[test]
    fn preserves_default_and_resets_parameters() {
        let mut context = catalog(Dialect::postgres());
        for sql in ["UPDATE items SET id=$1", "UPDATE items SET id=DEFAULT"] {
            let statement = parse_statements(sql, Dialect::postgres())
                .next()
                .unwrap()
                .unwrap();
            let result =
                SemAst::from_ast_node(statement.node(), &mut context, &mut create_cache_context())
                    .unwrap();
            if sql.ends_with("DEFAULT") {
                assert!(
                    result.as_update().unwrap().assignments()[0]
                        .value()
                        .is_default()
                );
                assert!(context.parameter_constraints().parameter(1).is_none());
            }
        }
    }
}

impl UpdateStatement {
    pub(crate) fn validate_ast_view_trigger(
        ast: &Update,
        context: &mut impl ParseContext,
        table: &UpdateDestination,
    ) -> Result<(), ResolutionFailure> {
        if let Some(support) = table.view_support() {
            let columns: Vec<_> = ast
                .assignments()
                .iter()
                .map(|assignment| ColumnName::from(assignment.name().as_str()))
                .collect();
            if !support.accepts_assignments(&columns) {
                context.report_analysis_problem(AnalysisProblem::update(
                    UpdateProblemKind::view_operation(),
                    "SET targets do not activate an INSTEAD OF UPDATE trigger",
                ))?;
                return Err(ResolutionFailure::reported());
            }
        }
        Ok(())
    }
}

impl UpdateStatement {
    pub(crate) fn from_ast_in(
        ast: &Update,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Option<Self>, AnalysisError> {
        let result = Self::from_ast_definition(ast, context, outer);
        ResolutionFailure::finish(result, context)
    }
}

impl UpdateStatement {
    pub(crate) fn from_ast_definition(
        ast: &Update,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, ResolutionFailure> {
        let trigger_outer = if let Some(scope) = outer {
            scope.trigger_scope()
        } else {
            None
        };
        let name = FullName::from_ast_name(ast.relation().name())?;
        let name = if let Some(scope) = outer {
            scope.trigger_relation_name(context, &name)?
        } else {
            name
        };
        let requested = TableName::from(name);
        let table = Self::from_ast_destination(
            ast,
            context,
            &requested,
            outer.is_some_and(ScalarAnalysisContext::in_rule_definition),
        )?;
        Self::validate_ast_index_hint(ast, context, &table)?;
        Self::validate_ast_view_trigger(ast, context, &table)?;
        let with = WithClause::from_ast(ast.with(), context, outer)?;
        let alias = ast
            .relation()
            .alias()
            .as_ref()
            .map(AliasSpec::from_ast)
            .unwrap_or_else(|| AliasSpec::from_local_name(&requested.full_name().local_name()));
        let from = Self::from_ast_sources(ast, context, &table, &alias, &with, trigger_outer)?;
        if let Some(outer) = outer {
            outer.validate_rule_sources(context, &from)?;
        }
        let scope = ScalarAnalysisContext::nested(&with, &from, trigger_outer);
        let mut target_from = FromClause::default();
        target_from.push(DataSource::resolve_relation(
            context,
            table.name().full_name(),
            &alias,
            &WithClause::default(),
        )?);
        let target_scope = ScalarAnalysisContext::nested(&with, &target_from, None);
        let assignments = UpdateAssignmentCollection::resolve(
            context,
            ast.assignments(),
            &table,
            &alias,
            &target_scope,
            &scope,
        )?;
        let predicate = Self::from_ast_predicate(ast, context, table.name(), &scope)?;
        let returning = Self::from_ast_returning_scope(ast, context, &requested, &table, &scope)?;
        let mut update = Self::new(
            table.name(),
            &assignments,
            &WithClause::ast_local_definitions(ast.with(), with),
            &from,
            &predicate,
            &returning,
        );
        update.set_modifiers(
            table.view_support(),
            ast.or_conflict(),
            ast.relation().indexed(),
            ast.relation().not_indexed(),
            ast.relation().inherit(),
        );
        update.validate_assignments(context)?;
        Ok(update)
    }
}

#[cfg(test)]
mod test_retains_update_bindings {
    #[cfg(feature = "sqlite")]
    use crate::test_helpers::update as test_helpers;

    use crate::{ast::Dialect, test_helpers::update::analyze};

    #[rstest::rstest]
    #[case::literal("UPDATE items SET id=2 RETURNING *")]
    #[case::only("UPDATE ONLY items SET id=2 RETURNING id")]
    #[case::default("UPDATE items SET id=DEFAULT RETURNING id")]
    #[case::alias("UPDATE items AS i SET id=i.id RETURNING i.id AS changed")]
    #[case::simultaneous("UPDATE items SET id=id,label=label")]
    #[case::row("UPDATE items SET (id,label)=(2,'two') RETURNING id")]
    #[case::row_query("UPDATE items SET (id,label)=(SELECT 2,'two') RETURNING id")]
    #[case::from("UPDATE items SET id=s.n FROM (SELECT 2 n) s RETURNING s.n")]
    #[case::target_star("UPDATE items SET id=s.n FROM (SELECT 2 n) s RETURNING *")]
    #[case::source_star("UPDATE items SET id=s.n FROM (SELECT 2 n) s RETURNING s.*")]
    #[case::cte("WITH s AS (SELECT 2 n) UPDATE items SET id=s.n FROM s RETURNING id")]
    #[case::subquery("UPDATE items SET id=(SELECT items.id)")]
    fn retains_update_bindings(#[case] sql: &str) {
        let (statement, context) = analyze(sql, Dialect::postgres());
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let update = statement.as_update().unwrap();
        assert_eq!(update.target().to_string(), "public.items");
        assert!(!update.assignments().is_empty());
        assert!(update.assignments()[0].destination().get_type().is_some());
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::plain("UPDATE items SET id=1 RETURNING *")]
    #[case::duplicate("UPDATE items SET id=1,id=2 RETURNING id")]
    #[case::row("UPDATE items SET (id,label)=(2,'two') RETURNING id")]
    #[case::from("UPDATE items SET id=s.n FROM (SELECT 2 n) s RETURNING id")]
    #[case::original_name("UPDATE items AS i SET id=2 RETURNING items.id")]
    #[case::conflict("UPDATE OR FAIL items SET id=2 RETURNING id")]
    #[case::not_indexed("UPDATE items NOT INDEXED SET id=2 RETURNING id")]
    fn sqlite_update(#[case] sql: &str) {
        test_helpers::sqlite_accepts(sql);
        let (statement, context) = analyze(sql, Dialect::sqlite());
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert!(!statement.as_update().unwrap().result_columns().is_empty());
    }
}

impl UpdateStatement {
    pub(crate) fn from_ast_destination(
        ast: &Update,
        context: &mut impl ParseContext,
        requested: &TableName,
        rule_definition: bool,
    ) -> Result<UpdateDestination, ResolutionFailure> {
        let destination = if rule_definition {
            UpdateDestination::resolve_rule_target(context, requested)?
        } else {
            UpdateDestination::resolve(context, requested)?
        };
        let Some(table) = destination else {
            if context
                .resolve_relation_name(requested.full_name())
                .is_none()
            {
                context.report_analysis_problem(AnalysisProblem::relation_not_found(requested))?;
            }
            return Err(ResolutionFailure::reported());
        };
        if !table.is_writable() {
            context.report_analysis_problem(AnalysisProblem::update(
                UpdateProblemKind::read_only_target(),
                "destination does not support UPDATE",
            ))?;
            return Err(ResolutionFailure::reported());
        }
        if !ast.returning().is_empty()
            && table
                .view_support()
                .is_some_and(|s| !s.supports_returning())
        {
            context.report_analysis_problem(AnalysisProblem::update(
                UpdateProblemKind::view_operation(),
                "view does not support UPDATE RETURNING",
            ))?;
        }
        Ok(table)
    }
}

#[cfg(test)]
mod test_respects_update_capabilities {
    use crate::{
        ast::Dialect,
        names::{ColumnName, FullName},
        parse_statements,
        sem::{CreateView, SemAst, ViewLike, ViewUpdateSupport},
        test_helpers::{create_cache_context, update::catalog},
    };

    #[rstest::rstest]
    #[case::writable("UPDATE writable_items SET id=$1 RETURNING *", true, true, true)]
    #[case::cte_shadow(
        "WITH writable_items AS (SELECT 2 other) UPDATE writable_items SET id=1 RETURNING *",
        true,
        true,
        true
    )]
    #[case::read_only("UPDATE writable_items SET id=1", false, false, false)]
    #[case::column("UPDATE writable_items SET label='one'", true, true, false)]
    #[case::returning("UPDATE writable_items SET id=1 RETURNING id", true, false, false)]
    fn respects_update_capabilities(
        #[case] sql: &str,
        #[case] writable: bool,
        #[case] returning: bool,
        #[case] valid: bool,
    ) {
        let mut context = catalog(Dialect::postgres());
        let view_query = SemAst::from_ast_node(
            parse_statements("SELECT id,label FROM items", Dialect::postgres())
                .next()
                .unwrap()
                .unwrap()
                .node(),
            &mut context,
            &mut create_cache_context(),
        )
        .unwrap()
        .into_select_statemtnt()
        .unwrap();
        let name = FullName::with_schema("public", "writable_items");
        let view = CreateView::new(&name, &view_query, &[], &None, false);
        context.set_get_view_impl_result(&name, &Some(ViewLike::from(&view)));
        let support = if writable {
            ViewUpdateSupport::writable(
                &[view.get_column_def(&ColumnName::from("id")).unwrap()],
                returning,
            )
        } else {
            ViewUpdateSupport::read_only()
        };
        context.set_view_update_support(&name, &support);
        let result = SemAst::from_ast_node(
            parse_statements(sql, Dialect::postgres())
                .next()
                .unwrap()
                .unwrap()
                .node(),
            &mut context,
            &mut create_cache_context(),
        );
        assert_eq!(
            context.reported_problem_count() == 0,
            valid,
            "{:?}",
            context.reported_problems()
        );
        if valid {
            assert_eq!(
                result.unwrap().as_update().unwrap().target().to_string(),
                "public.writable_items"
            );
        }
    }
}

impl UpdateStatement {
    pub(crate) fn validate_ast_index_hint(
        ast: &Update,
        context: &mut impl ParseContext,
        table: &UpdateDestination,
    ) -> Result<(), ResolutionFailure> {
        if let Some(index) = &ast.relation().indexed() {
            let name = FullName::with_schema(
                table
                    .name()
                    .full_name()
                    .schema()
                    .map(|schema| schema.as_str())
                    .unwrap_or("main"),
                index,
            );
            if context
                .get_index(&name)
                .is_none_or(|index| !index.belongs_to(table.name()))
            {
                context.report_analysis_problem(AnalysisProblem::update(
                    UpdateProblemKind::index_hint(),
                    "INDEXED BY does not identify an index of the UPDATE destination",
                ))?;
            }
        }
        Ok(())
    }
}

impl UpdateStatement {
    pub(crate) fn from_ast_sources(
        ast: &Update,
        context: &mut impl ParseContext,
        table: &UpdateDestination,
        alias: &AliasSpec,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<FromClause, ResolutionFailure> {
        // FROM subqueries cannot implicitly see the UPDATE target. Correlation
        // within RHS expressions uses the complete scope assembled below.
        let mut from = DataSource::from_ast_list_in(ast.from(), context, with, outer)?;
        let source = DataSource::resolve_relation(
            context,
            table.name().full_name(),
            alias,
            &WithClause::default(),
        )?;
        from.validate_source(context, &source)?;
        from.push(source);
        Ok(from)
    }
}

impl UpdateStatement {
    pub(crate) fn from_ast_predicate(
        ast: &Update,
        context: &mut impl ParseContext,
        target: &TableName,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<UpdatePredicate>, ResolutionFailure> {
        if let Some(expression) = &ast.predicate()
            && let Some(cursor) = UpdateCursor::from_ast_current_of(expression, context, target)?
        {
            return Ok(Some(UpdatePredicate::current_of(&cursor)));
        }
        let predicate = SemScalarExpr::from_ast_optional(ast.predicate(), context, scope)?;
        if context.dialect() == Dialect::postgres()
            && let Some(value) = &predicate
            && let Some(ty) = value.get_type()
            && ty != PgBuiltInType::bool()
            && !value
                .as_const()
                .is_some_and(|c| c.is_null() || c.as_string().is_some())
            && context
                .get_implicit_cast(&ty, &PgBuiltInType::bool())
                .is_none()
        {
            context.report_analysis_problem(AnalysisProblem::update(
                UpdateProblemKind::predicate(),
                "UPDATE WHERE requires boolean type",
            ))?;
        }
        Ok(predicate.as_ref().map(UpdatePredicate::expression))
    }
}

impl UpdateStatement {
    pub(crate) fn from_ast_returning_scope(
        ast: &Update,
        context: &mut impl ParseContext,
        requested: &TableName,
        table: &UpdateDestination,
        outer: &ScalarAnalysisContext<'_>,
    ) -> Result<ResultColumnCollection, ResolutionFailure> {
        let with = outer.with_clause();
        let from = outer.sources();
        if context.dialect() == Dialect::sqlite() {
            let returning_alias = AliasSpec::from_local_name(&requested.full_name().local_name());
            let mut returning_from = FromClause::default();
            returning_from.push(DataSource::resolve_relation(
                context,
                table.name().full_name(),
                &returning_alias,
                &WithClause::default(),
            )?);
            let scope = ScalarAnalysisContext::nested(with, &returning_from, None);
            ResultColumnCollection::resolve_for_update(
                ast.returning(),
                context,
                &returning_from,
                &scope,
            )
            .map_err(Into::into)
        } else {
            let mut scope = ScalarAnalysisContext::nested(with, from, Some(outer));
            scope.set_postgres_returning(context);
            ResultColumnCollection::resolve_for_update(ast.returning(), context, from, &scope)
                .map_err(Into::into)
        }
    }
}

#[cfg(test)]
mod test_sqlite_invalid_returning {
    #[cfg(feature = "sqlite")]
    use crate::{ast::Dialect, test_helpers::update::analyze};

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::alias("UPDATE items AS i SET id=1 RETURNING i.id")]
    #[case::from("UPDATE items SET id=s.n FROM (SELECT 1 n) s RETURNING s.n")]
    #[case::qualified_star("UPDATE items SET id=1 RETURNING items.*")]
    fn sqlite_invalid_returning(#[case] sql: &str) {
        let (_, context) = analyze(sql, Dialect::sqlite());
        assert!(context.reported_problem_count() > 0);
    }
}

impl UpdateAssignmentCollection {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        assignments: &[Assignment],
        table: &UpdateDestination,
        alias: &AliasSpec,
        target_scope: &ScalarAnalysisContext<'_>,
        rhs_scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, ResolutionFailure> {
        let mut resolved = Self::default();
        let mut seen: Vec<&Assignment> = Vec::new();
        for assignment in assignments {
            if context.dialect() == Dialect::postgres()
                && seen.iter().any(|previous| {
                    InsertTargetCollection::ast_overlaps_mapped(assignment, previous, |name| {
                        table.view_support().map_or_else(
                            || name.clone(),
                            |support| support.assignment_column(name).clone(),
                        )
                    })
                })
            {
                context.report_analysis_problem(AnalysisProblem::update(
                    UpdateProblemKind::duplicate_target(),
                    "column assigned more than once",
                ))?;
            }
            seen.push(assignment);
            let column_name = ColumnName::from(assignment.name().as_str());
            let Some(column) = table.column(&column_name) else {
                context.report_analysis_problem(AnalysisProblem::column_not_found_in_table(
                    table.name(),
                    &column_name,
                ))?;
                return Err(ResolutionFailure::reported());
            };
            if !table.can_assign(&column_name) {
                context.report_analysis_problem(AnalysisProblem::update(
                    UpdateProblemKind::read_only_target(),
                    "view column does not accept UPDATE assignments",
                ))?;
            }
            let destination = target_scope
                .resolve_column(context, Some(alias.name()), &column_name)?
                .0;
            let destination = IndirectionExpr::from_ast_sequence(
                assignment.indirection(),
                context,
                rhs_scope,
                destination,
            )?;
            let value = UpdateValue::from_ast_assignment(assignment, context, rhs_scope)?;
            if value.is_default()
                && table
                    .view_support()
                    .is_some_and(|support| !support.can_assign_default(&column_name))
            {
                context.report_analysis_problem(AnalysisProblem::update(
                    UpdateProblemKind::default_target(),
                    "view rewriting expands DEFAULT before the base generated/identity assignment check",
                ))?;
            }
            UpdateValue::validate_ast_update_value(assignment, context, column, &value)?;
            resolved.push(UpdateAssignment::new(column, &destination, &value));
        }
        Ok(resolved)
    }
}

impl UpdateValue {
    pub(crate) fn validate_ast_update_value(
        ast: &Assignment,
        context: &mut impl ParseContext,
        column: &ColumnDefinition,
        value: &Self,
    ) -> Result<(), ResolutionFailure> {
        if value.is_default() && !ast.indirection().is_empty() {
            context.report_analysis_problem(AnalysisProblem::update(
                UpdateProblemKind::default_target(),
                "DEFAULT requires a whole-column destination",
            ))?;
        }
        let mut generated = false;
        let mut identity_always = false;
        for constraint in column.constraints() {
            generated |= constraint.as_generated().is_some();
            identity_always |= constraint
                .as_identity()
                .is_some_and(|identity| identity.is_always());
        }
        if generated && (!value.is_default() || context.dialect() == Dialect::sqlite()) {
            context.report_analysis_problem(AnalysisProblem::update(
                UpdateProblemKind::generated_target(),
                "generated column accepts only PostgreSQL DEFAULT",
            ))?;
        }
        if identity_always && !value.is_default() {
            context.report_analysis_problem(AnalysisProblem::update(
                UpdateProblemKind::identity_target(),
                "GENERATED ALWAYS identity accepts only DEFAULT",
            ))?;
        }
        Ok(())
    }
}

impl UpdateValue {
    pub(crate) fn from_ast_assignment(
        ast: &Assignment,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, ResolutionFailure> {
        let Some(value) = &ast.value() else {
            return AnalysisError::raise_unexpected_input("missing UPDATE RHS").map_err(Into::into);
        };
        if let Some((source, position, width)) = value.multi_assignment_data() {
            let index = usize::try_from(position - 1)
                .map_err(|_| AnalysisError::unexpected_input_error("row assignment position"))?;
            return Self::from_ast_row(source, context, scope, index, width as usize);
        }
        Self::from_ast_expr(value, context, scope)
    }
}

impl UpdateValue {
    pub(crate) fn from_ast_row(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        index: usize,
        width: usize,
    ) -> Result<Self, ResolutionFailure> {
        match ast {
            Expr::Row(values) | Expr::Parenthesized(values) | Expr::List(values) => {
                Self::from_ast_update_row_values(values, context, scope, index, width)
            }
            Expr::Subquery(query) => {
                SemScalarExpr::from_ast_subquery_row(query.as_ref(), context, scope, index, width)
            }
            _ => Err(AnalysisError::unexpected_input_error(
                "row assignment requires row or subquery",
            )
            .into()),
        }
    }
}

#[cfg(test)]
mod test_row_assignment_retains_full_subquery_and_selected_output {
    use crate::{ast::Dialect, sem::PgBuiltInType, test_helpers::update::analyze};

    #[test]
    fn row_assignment_retains_full_subquery_and_selected_output() {
        let (statement, context) = analyze(
            "UPDATE items SET (id,label)=(SELECT $1::integer,'two')",
            Dialect::postgres(),
        );
        assert_eq!(context.reported_problem_count(), 0);
        let update = statement.as_update().unwrap();
        let value = update.assignments()[0].value().as_query().unwrap();
        assert_eq!(value.index(), 0);
        assert_eq!(value.query().result_columns().len(), 2);
        assert_eq!(
            value
                .query()
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .col_type(),
            &Some(PgBuiltInType::int4())
        );
        assert_eq!(
            update.assignments()[1].value().as_query().unwrap().index(),
            1
        );
    }
}

impl UpdateValue {
    pub(crate) fn from_ast_update_row_values(
        values: &[Expr],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        index: usize,
        width: usize,
    ) -> Result<Self, ResolutionFailure> {
        if values.len() != width {
            return Err(
                AnalysisError::unexpected_input_error("row assignment width mismatch").into(),
            );
        }
        Self::from_ast_expr(
            values
                .get(index)
                .ok_or_else(|| AnalysisError::unexpected_input_error("row assignment index"))?,
            context,
            scope,
        )
    }
}

impl UpdateValue {
    pub(crate) fn from_ast_expr(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, ResolutionFailure> {
        match ast {
            Expr::Default => Ok(Self::default_value()),
            _ => Ok(Self::expression(&SemScalarExpr::from_ast_in(
                ast, context, scope,
            )?)),
        }
    }
}
