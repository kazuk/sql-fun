use crate::{
    CacheContext,
    ast::{Delete, Dialect},
    context::ParseContext,
    names::{FullName, TableName},
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, CreateTrigger as SemCreateTrigger, CteStatement,
        DataSource, DeleteCursor, DeleteDestination, DeletePredicate, DeleteStatement, FromClause,
        PgBuiltInType, ResolutionFailure, ResultColumnCollection, ScalarAnalysisContext, SemAst,
        SemScalarExpr, WithClause, parameter_resolution::ParameterResolution,
        problem::DeleteProblemKind,
    },
};

impl DeleteStatement {
    pub(crate) fn from_ast(
        ast: &Delete,
        context: &mut impl ParseContext,
        cache: &mut CacheContext,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_in(ast, context, None).and_then(|mut delete| {
            if context.parameter_inference_needed() {
                let constraints = ParameterResolution::infer_delete(&mut delete, context, cache)?;
                context.set_parameter_constraints(&constraints);
            }
            Ok(delete)
        });
        context.finish_analysis(result)
    }
}

#[cfg(test)]
mod test_analyzes_unused_cte_and_resets_statement_parameter_state {

    use crate::{
        ast::Dialect,
        context::ParseContext,
        sem::{DeleteStatement, PgBuiltInType},
        test_helpers::{delete as test_helpers, delete::catalog},
    };

    #[test]
    fn analyzes_unused_cte_and_resets_statement_parameter_state() {
        let mut context = catalog(Dialect::postgres());
        test_helpers::resolve(
            "WITH d AS (DELETE FROM items WHERE $1) SELECT 1",
            &mut context,
            Dialect::postgres(),
        )
        .unwrap();
        assert!(context.parameter_constraints().parameter(1).is_some());
        let result = test_helpers::resolve(
            "DELETE FROM items RETURNING id",
            &mut context,
            Dialect::postgres(),
        )
        .unwrap();
        assert!(context.parameter_constraints().parameter(1).is_none());
        // Result metadata serialization follows the existing ResultColumn contract:
        // expressions are skipped, while target, scopes and column metadata persist.
        let serialized = ::serde_json::to_value(result.as_delete().unwrap()).unwrap();
        let restored: DeleteStatement = ::serde_json::from_value(serialized).unwrap();
        assert_eq!(restored.target(), result.as_delete().unwrap().target());
        assert_eq!(
            restored
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .col_type(),
            &Some(PgBuiltInType::int4())
        );
    }
}

impl DeleteStatement {
    pub(crate) fn from_ast_in(
        ast: &Delete,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
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
        let predicate = Self::from_ast_predicate(ast, context, table.name(), &scope)?;
        let returning =
            Self::from_ast_returning_scope(ast, context, &requested, &table, &alias, &scope)?;
        let mut delete = Self::new(
            table.name(),
            table.found(),
            &WithClause::ast_local_definitions(ast.with(), with),
            &from,
            &predicate,
            &returning,
        );
        delete.set_modifiers(
            table.view_support(),
            ast.relation().indexed(),
            ast.relation().not_indexed(),
            ast.relation().inherit(),
        );
        Ok(delete)
    }
}

#[cfg(test)]
mod test_retains_delete_bindings {

    use crate::{ast::Dialect, test_helpers::delete::analyze};

    #[rstest::rstest]
    #[case::plain("DELETE FROM items")]
    #[case::star("DELETE FROM items RETURNING *")]
    #[case::only("DELETE FROM ONLY items RETURNING id")]
    #[case::alias("DELETE FROM items AS i WHERE true RETURNING i.id AS removed")]
    #[case::using("DELETE FROM items USING (SELECT 2 n) s RETURNING s.n")]
    #[case::target_star("DELETE FROM items USING (SELECT 2 n) s RETURNING *")]
    #[case::source_star("DELETE FROM items USING (SELECT 2 n) s RETURNING s.*")]
    #[case::with("WITH s AS (SELECT 2 n) DELETE FROM items USING s RETURNING s.n")]
    #[case::correlated(
        "DELETE FROM items WHERE (SELECT items.id IS NULL) RETURNING (SELECT items.label)"
    )]
    #[case::cte_shadow("WITH items AS (SELECT 2 other) DELETE FROM items RETURNING id")]
    fn retains_delete_bindings(#[case] sql: &str) {
        let (statement, context) = analyze(sql, Dialect::postgres());
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let delete = statement.as_delete().unwrap();
        assert_eq!(delete.target().to_string(), "public.items");
        assert_eq!(delete.includes_descendants(), !sql.contains("ONLY"));
        assert!(
            delete
                .from()
                .resolve_alias(&crate::names::AliasName::from(if sql.contains("AS i ") {
                    "i"
                } else {
                    "items"
                }))
                .is_some()
        );
    }
}

impl DeleteStatement {
    pub(crate) fn from_ast_destination(
        ast: &Delete,
        context: &mut impl ParseContext,
        requested: &TableName,
        rule_definition: bool,
    ) -> Result<DeleteDestination, AnalysisError> {
        let destination = if rule_definition {
            DeleteDestination::resolve_rule_target(context, requested)
        } else {
            DeleteDestination::resolve(context, requested)
        };
        let Some(table) = destination else {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(requested))?;
            return Ok(DeleteDestination::missing(requested));
        };
        if !table.is_deletable() {
            context.report_analysis_problem(AnalysisProblem::delete(
                DeleteProblemKind::read_only_target(),
            ))?;
        }
        if !ast.returning().is_empty()
            && table
                .view_support()
                .is_some_and(|s| !s.supports_returning())
        {
            context.report_analysis_problem(AnalysisProblem::delete(
                DeleteProblemKind::view_operation(),
            ))?;
        }
        Ok(table)
    }
}

#[cfg(test)]
mod test_respects_delete_capabilities {
    use crate::{
        ast::Dialect,
        names::FullName,
        parse_statements,
        sem::{CreateView, SemAst, ViewDeleteSupport, ViewLike},
        test_helpers::{create_cache_context, delete::catalog},
    };

    #[rstest::rstest]
    #[case::writable("DELETE FROM writable_items WHERE $1 RETURNING *", true, true, true)]
    #[case::cte_shadow(
        "WITH writable_items AS (SELECT 2 other) DELETE FROM writable_items RETURNING *",
        true,
        true,
        true
    )]
    #[case::read_only("DELETE FROM writable_items", false, false, false)]
    #[case::returning("DELETE FROM writable_items RETURNING id", true, false, false)]
    fn respects_delete_capabilities(
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
            ViewDeleteSupport::deletable(returning)
        } else {
            ViewDeleteSupport::read_only()
        };
        context.set_view_delete_support(&name, &support);
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
        let result = result.unwrap();
        assert!(result.as_delete().unwrap().target_found());
        assert_eq!(result.as_delete().unwrap().view_support(), Some(&support));
        if valid {
            assert_eq!(
                result.as_delete().unwrap().target().to_string(),
                "public.writable_items"
            );
        }
    }
}

#[cfg(test)]
mod test_retains_destination_lookup_outcome {

    use crate::{
        ast::Dialect,
        test_helpers::{delete as test_helpers, delete::catalog},
    };

    #[rstest::rstest]
    #[case::missing("DELETE FROM absent WHERE $1::boolean RETURNING absent.id", false)]
    #[case::missing_star("DELETE FROM absent RETURNING *", false)]
    #[case::found("DELETE FROM items RETURNING id", true)]
    fn retains_destination_lookup_outcome(#[case] sql: &str, #[case] found: bool) {
        let mut context = catalog(Dialect::postgres());
        let statement = test_helpers::resolve(sql, &mut context, Dialect::postgres()).unwrap();
        let delete = statement.as_delete().unwrap();
        assert_eq!(delete.target_found(), found);
        assert_eq!(context.reported_problem_count() == 0, found);
        if !found {
            assert_eq!(delete.target().to_string(), "absent");
            assert!(matches!(
                delete
                    .from()
                    .resolve_alias(&crate::names::AliasName::from("absent"))
                    .unwrap()
                    .0,
                crate::sem::DataSource::Unknown(_, _)
            ));
        }
    }
}

impl DeleteStatement {
    pub(crate) fn validate_ast_index_hint(
        ast: &Delete,
        context: &mut impl ParseContext,
        table: &DeleteDestination,
    ) -> Result<(), AnalysisError> {
        if let Some(index) = ast.relation().indexed() {
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
                context.report_analysis_problem(AnalysisProblem::delete(
                    DeleteProblemKind::index_hint(),
                ))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_sqlite_index_hint_belongs_to_the_target {
    #[cfg(feature = "sqlite")]
    use crate::{
        ast::Dialect,
        context::ParseContext,
        names::ColumnName,
        test_helpers::{
            delete as test_helpers,
            delete::{analyze, catalog},
        },
    };

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::target("items", false)]
    #[case::other_target("other", true)]
    fn sqlite_index_hint_belongs_to_the_target(#[case] indexed_table: &str, #[case] invalid: bool) {
        use crate::{
            names::{FullName, TableName},
            sem::{CreateIndex, IndexElementCollection},
        };
        let context = catalog(Dialect::sqlite());
        let index = CreateIndex::new(
            &FullName::with_schema("public", "items_idx"),
            &TableName::from(FullName::with_schema("public", indexed_table)),
            &IndexElementCollection::from_column_names(&vec![ColumnName::from("id")].into()),
        );
        let mut context = context.apply_create_index(&index).unwrap();
        let result = test_helpers::resolve(
            "DELETE FROM items INDEXED BY items_idx RETURNING id",
            &mut context,
            Dialect::sqlite(),
        )
        .unwrap();
        assert_eq!(context.reported_problem_count() > 0, invalid);
        assert_eq!(result.as_delete().unwrap().indexed(), Some("items_idx"));
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_missing_index_reports_a_structured_violation() {
        let (_, context) = analyze("DELETE FROM items INDEXED BY absent", Dialect::sqlite());
        assert_eq!(
            context.reported_problems()[0]
                .delete_problem()
                .unwrap()
                .kind(),
            &crate::sem::problem::DeleteProblemKind::index_hint()
        );
    }
}

impl DeleteStatement {
    pub(crate) fn from_ast_sources(
        ast: &Delete,
        context: &mut impl ParseContext,
        table: &DeleteDestination,
        alias: &AliasSpec,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<FromClause, AnalysisError> {
        // USING subqueries cannot implicitly see the DELETE target. WHERE and
        // RETURNING subqueries use the complete scope assembled below.
        let mut from = DataSource::from_ast_list_in(ast.using(), context, with, outer)?;
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

impl DeleteStatement {
    pub(crate) fn from_ast_predicate(
        ast: &Delete,
        context: &mut impl ParseContext,
        target: &TableName,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<DeletePredicate>, AnalysisError> {
        if let Some(expression) = &ast.predicate()
            && let Some(cursor) =
                DeleteCursor::from_ast_delete_current_of(expression, context, target)?
        {
            return Ok(Some(DeletePredicate::current_of(&cursor)));
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
            context
                .report_analysis_problem(AnalysisProblem::delete(DeleteProblemKind::predicate()))?;
        }
        Ok(predicate.as_ref().map(DeletePredicate::expression))
    }
}

impl DeleteStatement {
    pub(crate) fn from_ast_returning_scope(
        ast: &Delete,
        context: &mut impl ParseContext,
        requested: &TableName,
        table: &DeleteDestination,
        alias: &AliasSpec,
        outer: &ScalarAnalysisContext<'_>,
    ) -> Result<ResultColumnCollection, AnalysisError> {
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
            let mut scope = ScalarAnalysisContext::nested(with, &returning_from, None);
            scope.set_delete_returning_target(returning_alias.name(), context);
            ResultColumnCollection::resolve_for_delete(
                ast.returning(),
                context,
                &returning_from,
                &scope,
            )
        } else {
            let mut scope = ScalarAnalysisContext::nested(with, from, Some(outer));
            scope.set_delete_returning_target(alias.name(), context);
            ResultColumnCollection::resolve_for_delete(ast.returning(), context, from, &scope)
        }
    }
}

#[cfg(test)]
mod test_sqlite_delete {
    #[cfg(feature = "sqlite")]
    use crate::{
        ast::Dialect,
        test_helpers::{delete as test_helpers, delete::analyze},
    };

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::plain("DELETE FROM items RETURNING *")]
    #[case::alias("DELETE FROM items AS i WHERE i.id IS NULL RETURNING items.id")]
    #[case::with(
        "WITH c AS (SELECT 1 n) DELETE FROM items WHERE EXISTS (SELECT n FROM c) RETURNING id"
    )]
    #[case::not_indexed("DELETE FROM items NOT INDEXED RETURNING id")]
    #[case::parameter("DELETE FROM items WHERE ?1 RETURNING ?2")]
    fn sqlite_delete(#[case] sql: &str) {
        test_helpers::sqlite_accepts(sql);
        let (statement, context) = analyze(sql, Dialect::sqlite());
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert!(!statement.as_delete().unwrap().result_columns().is_empty());
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::alias("DELETE FROM items AS i RETURNING i.id")]
    #[case::star("DELETE FROM items RETURNING items.*")]
    fn sqlite_rejects_invalid_returning(#[case] sql: &str) {
        let (_, context) = analyze(sql, Dialect::sqlite());
        assert!(context.reported_problem_count() > 0);
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

#[cfg(test)]
mod test_sqlite_delete_cte_reports_an_unsupported_operation {
    #[cfg(feature = "sqlite")]
    use crate::{ast::Dialect, parse_statements, sem::SemAst, test_helpers::delete::catalog};

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_delete_cte_reports_an_unsupported_operation() {
        let mut context = catalog(Dialect::sqlite());
        // The common AST can carry a DELETE CTE even though SQLite's parser rejects it.
        let statement = parse_statements(
            "WITH d AS (DELETE FROM items RETURNING id) SELECT id FROM d",
            Dialect::postgres(),
        )
        .next()
        .unwrap()
        .unwrap();
        assert!(
            SemAst::from_ast_node(
                statement.node(),
                &mut context,
                &mut crate::test_helpers::create_cache_context()
            )
            .is_ok()
        );
        assert!(context.reported_problems().iter().any(|problem| {
            problem.delete_problem().is_some_and(|problem| {
                problem.kind() == &crate::sem::problem::DeleteProblemKind::sqlite_cte()
            })
        }));
    }
}

impl DeleteStatement {
    pub(crate) fn from_ast_trigger_command(
        ast: &Delete,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemAst, ResolutionFailure> {
        DataSource::validate_ast_trigger_command(ast.relation())?;
        if !ast.returning().is_empty() {
            return SemCreateTrigger::ast_invalid(
                &FullName::from_ast_name(ast.relation().name())?,
                "SQLite trigger DELETE does not support RETURNING",
            );
        }
        Self::from_ast_in(ast, context, Some(scope))
            .map(|statement| SemAst::delete(&statement))
            .map_err(Into::into)
    }
}
