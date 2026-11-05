use crate::{
    CacheContext,
    ast::document::{CmdType, Merge, MergeMatchKind, MergeWhen},
    context::ParseContext,
    names::{FullName, TableName},
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, CteStatement, DataSource, FromClause,
        InsertClauseSemantics, MergeAction, MergeDestination, MergeMatchKind as SemMergeMatchKind,
        MergeStatement, MergeWhen as SemMergeWhen, MergeWhenCollection, ResolutionFailure,
        ResultColumnCollection, ScalarAnalysisContext, SemScalarExpr, UpdateAssignmentCollection,
        WithClause, insert_statement::InsertTargetBinding,
        parameter_resolution::ParameterResolution,
    },
};

impl MergeStatement {
    pub(crate) fn from_ast(
        ast: &Merge,
        context: &mut impl ParseContext,
        cache: &mut CacheContext,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_in(ast, context, None).and_then(|mut merge| {
            if context.parameter_inference_needed() {
                let constraints = ParameterResolution::infer_merge(&mut merge, context, cache)?;
                context.set_parameter_constraints(&constraints);
            }
            Ok(merge)
        });
        context.finish_analysis(result)
    }
}

#[cfg(test)]
mod test_cte_source_and_merge_actions_share_parameter_analysis {
    use crate::{
        context::ParseContext,
        sem::{
            PgBuiltInType, SemAst,
            parameter_resolution::inference_runs as parameter_resolution_inference_runs,
        },
        test_helpers::{
            create_cache_context, document_merge::catalog, merge_statements::statement,
        },
    };

    #[test]
    fn cte_source_and_merge_actions_share_parameter_analysis() {
        let mut cache_context = create_cache_context();
        let source = statement(
            "WITH input AS (SELECT $1::integer AS sid) MERGE INTO items t USING input s ON true WHEN MATCHED THEN UPDATE SET id=s.sid WHEN NOT MATCHED THEN INSERT(id) VALUES(s.sid) RETURNING t.id",
        );
        let before = parameter_resolution_inference_runs();
        let analyzed = SemAst::from_ast(&source, catalog(17), &mut cache_context).unwrap();
        assert_eq!(analyzed.context().reported_problem_count(), 0);
        assert_eq!(
            crate::sem::parameter_resolution::inference_runs(),
            before + 1
        );
        assert!(
            analyzed
                .context()
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
                .allows(&PgBuiltInType::int4())
        );
    }
}

impl MergeStatement {
    pub(crate) fn ast_require_version(
        context: &impl ParseContext,
        minimum: u32,
    ) -> Result<(), AnalysisError> {
        Self::require_version(context, minimum)
    }
}

impl MergeStatement {
    pub(crate) fn from_ast_in(
        ast: &Merge,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        Self::ast_require_version(context, 15)?;
        if ast.with_clause().recursive() {
            return Err(AnalysisError::unexpected_input_error(
                "WITH RECURSIVE is not supported for MERGE",
            ));
        }
        if !ast.returning_list().is_empty() {
            Self::ast_require_version(context, 17)?;
        }
        let requested = TableName::from(FullName::from_ast_name(ast.relation().name())?);
        // MERGE eligibility is separate from the ordinary INSERT/UPDATE/DELETE
        // capabilities of a view. Never infer it from those flags.
        let Some(table) = MergeDestination::resolve(context, &requested)? else {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(&requested))?;
            return Err(AnalysisError::unexpected_input_error(
                "MERGE target relation not found",
            ));
        };
        if table.is_materialized_view() {
            return Err(AnalysisError::unexpected_input_error(
                "MERGE cannot target a materialized view",
            ));
        }
        if table.view_support().is_some() {
            Self::ast_require_version(context, 17)?;
        }
        let alias = ast
            .relation()
            .alias()
            .as_ref()
            .map(AliasSpec::from_ast)
            .unwrap_or_else(|| AliasSpec::from_local_name(&requested.full_name().local_name()));
        let with = WithClause::from_ast(ast.with_clause(), context, outer)?;
        let target = DataSource::resolve_relation(
            context,
            table.name().full_name(),
            &alias,
            &WithClause::default(),
        )?;
        let source = DataSource::from_ast(
            ast.source_relation(),
            context,
            &with,
            &FromClause::default(),
            None,
        )?;
        if source.namespace().contains_alias(alias.name()) {
            context.report_analysis_problem(AnalysisProblem::duplicate_alias(alias.name()))?;
        }
        let on_scope = ScalarAnalysisContext::for_merge_on(&with, &target, &source);
        let on = SemScalarExpr::from_ast_merge_predicate(ast.join_condition(), context, &on_scope)?;
        let clauses =
            Self::from_ast_clauses(ast, context, &with, &target, &source, &table, &alias)?;
        if let Some(support) = table.view_support()
            && !support.supports_actions(
                clauses.permits_insert(),
                clauses.permits_update(),
                clauses.permits_delete(),
            )
        {
            return Err(AnalysisError::unexpected_input_error(
                "view does not support all MERGE action types",
            ));
        }
        let returning_scope = ScalarAnalysisContext::for_merge_returning(
            context,
            &with,
            &target,
            &source,
            clauses.source_nullable(),
            clauses.permits_insert(),
            clauses.permits_delete(),
        );
        let returning = ResultColumnCollection::resolve_for_merge(
            ast.returning_list(),
            context,
            &returning_scope,
        )?;
        let mut statement = Self::new(
            table.name(),
            &WithClause::ast_local_definitions(ast.with_clause(), with.clone()),
            on_scope.sources(),
            &on,
            &clauses,
            &returning,
            ast.relation().inherit(),
        );
        statement.set_view_support(table.view_support());
        statement.validate_assignments(context)?;
        Ok(statement)
    }
}

#[cfg(test)]
mod test_retains_merge_bindings {
    use crate::test_helpers::{document_merge::catalog, statements::resolve_postgres as resolve};

    #[rstest::rstest]
    #[case::on_bindings(
        "MERGE INTO items t USING incoming s ON t.id=s.sid WHEN MATCHED THEN DELETE"
    )]
    #[case::update(
        "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=s.sid"
    )]
    #[case::insert(
        "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT (id,label) VALUES(s.sid,s.slabel)"
    )]
    #[case::defaults(
        "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT DEFAULT VALUES"
    )]
    #[case::delete("MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE")]
    #[case::nothing("MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DO NOTHING")]
    #[case::by_source(
        "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED BY SOURCE THEN UPDATE SET id=t.id"
    )]
    #[case::returning(
        "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING t.id,s.sid,merge_action()"
    )]
    #[case::star(
        "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING *"
    )]
    #[case::qualified_star(
        "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING t.*"
    )]
    #[case::correlated_update(
        "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=(SELECT s.sid)"
    )]
    #[case::correlated_insert(
        "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(id) VALUES((SELECT s.sid))"
    )]
    #[case::returning_subquery(
        "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING (SELECT merge_action())"
    )]
    #[case::row_assignment(
        "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET (id,label)=(SELECT s.sid,s.slabel)"
    )]
    #[case::using_cte(
        "WITH incoming AS (SELECT 1 sid) MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=s.sid"
    )]
    #[case::using_query(
        "MERGE INTO items t USING (SELECT 1 sid) s ON true WHEN MATCHED THEN UPDATE SET id=s.sid"
    )]
    fn retains_merge_bindings(#[case] sql: &str) {
        let mut context = catalog(17);
        let statement = resolve(sql, &mut context).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let merge = statement.as_merge().unwrap();
        assert_eq!(merge.target().to_string(), "public.items");
        assert_eq!(merge.clauses().len(), 1);
    }
}

#[cfg(test)]
mod test_trigger_backed_view_requires_a_trigger_for_each_action_type {
    use crate::{
        sem::{ViewMergeSupport, ViewMergeTriggers},
        test_helpers::{
            document_merge::catalog,
            merge_views::{all_columns, install_view, trigger_support, view_catalog},
            statements::resolve_postgres as resolve,
        },
    };

    #[test]
    fn trigger_backed_view_requires_a_trigger_for_each_action_type() {
        let support = trigger_support(true, false, true);
        let mut context = view_catalog(17, &support);
        let accepted = resolve(
            "MERGE INTO writable_items t USING incoming s ON true WHEN MATCHED THEN DELETE WHEN NOT MATCHED THEN INSERT(id,label) VALUES(s.sid,s.slabel)",
            &mut context,
        );
        assert!(
            accepted.is_ok(),
            "{accepted:?} {:?}",
            context.reported_problems()
        );

        let mut context = view_catalog(17, &support);
        let rejected = resolve(
            "MERGE INTO writable_items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET label=s.slabel WHEN NOT MATCHED BY SOURCE THEN DELETE",
            &mut context,
        );
        assert!(rejected.is_err() || context.reported_problem_count() > 0);

        let all_actions = trigger_support(true, true, true);
        let mut context = view_catalog(17, &all_actions);
        let accepted = resolve(
            "MERGE INTO writable_items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET label=s.slabel WHEN NOT MATCHED THEN INSERT(id,label) VALUES(s.sid,s.slabel) WHEN NOT MATCHED BY SOURCE THEN DELETE",
            &mut context,
        );
        assert!(
            accepted.is_ok(),
            "{accepted:?} {:?}",
            context.reported_problems()
        );
    }

    #[test]
    fn rules_and_materialized_views_are_rejected() {
        let with_rules = ViewMergeSupport::new(
            true,
            true,
            ViewMergeTriggers::new(true, true, true),
            Some(&all_columns()),
            Some(&all_columns()),
        );
        let mut context = view_catalog(17, &with_rules);
        let result = resolve(
            "MERGE INTO writable_items t USING incoming s ON true WHEN MATCHED THEN DELETE",
            &mut context,
        );
        assert!(result.is_err() || context.reported_problem_count() > 0);

        let mut context = catalog(17);
        install_view(&mut context, true);
        let result = resolve(
            "MERGE INTO writable_items t USING incoming s ON true WHEN MATCHED THEN DELETE",
            &mut context,
        );
        assert!(result.is_err() || context.reported_problem_count() > 0);
    }
}

impl MergeStatement {
    pub(crate) fn from_ast_clauses(
        ast: &Merge,
        context: &mut impl ParseContext,
        with: &WithClause,
        target: &DataSource,
        source: &DataSource,
        table: &MergeDestination,
        alias: &AliasSpec,
    ) -> Result<MergeWhenCollection, AnalysisError> {
        let target_scope = ScalarAnalysisContext::for_merge_on(with, target, source);
        let mut clauses = MergeWhenCollection::default();
        let mut terminal = [false; 3];
        for clause in ast.merge_when_clauses() {
            let index = SemMergeMatchKind::ast_index(&clause.match_kind());
            if terminal[index] {
                return Err(AnalysisError::unexpected_input_error(
                    "unreachable MERGE WHEN clause after unconditional action",
                ));
            }
            terminal[index] = clause.condition().is_none();
            let kind = SemMergeMatchKind::from_ast_merge_match_kind(&clause.match_kind(), context)?;
            let scope = ScalarAnalysisContext::for_merge_when(with, target, source, kind);
            let condition = clause
                .condition()
                .as_ref()
                .map(|expr| SemScalarExpr::from_ast_merge_predicate(expr, context, &scope))
                .transpose()?;
            let action =
                MergeAction::from_ast_action(clause, context, &scope, &target_scope, table, alias)?;
            clauses.push(&SemMergeWhen::new(kind, &condition, &action));
        }
        Ok(clauses)
    }
}

#[cfg(test)]
mod test_serializes_typed_update_and_insert_actions {
    use crate::{
        sem::{MergeRecordRole, MergeStatement},
        test_helpers::{
            document_merge::catalog, merge_assignments::assignment_catalog,
            statements::resolve_postgres as resolve,
        },
    };

    #[test]
    fn serializes_typed_update_and_insert_actions() {
        let mut context = catalog(17);
        let statement = resolve(
            "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=s.sid WHEN NOT MATCHED THEN INSERT(id,label) VALUES(s.sid,s.slabel)",
            &mut context,
        )
        .unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let merge = statement.as_merge().unwrap();
        let update = merge.clauses()[0].action().as_update().unwrap();
        assert_eq!(
            update.assignments()[0]
                .destination()
                .merge_record_reference()
                .unwrap()
                .role(),
            MergeRecordRole::target()
        );
        assert!(merge.clauses()[1].action().as_insert().is_some());

        let serialized = ::serde_json::to_value(merge).unwrap();
        let clauses = serialized["clauses"].as_array().unwrap();
        assert!(clauses[0]["action"].get("Update").is_some());
        assert!(clauses[1]["action"].get("Insert").is_some());

        let restored: MergeStatement = ::serde_json::from_value(serialized).unwrap();
        assert!(restored.clauses()[0].action().as_update().is_some());
        let insert = restored.clauses()[1].action().as_insert().unwrap();
        assert_eq!(insert.targets().len(), 2);
        assert_eq!(insert.values().unwrap().len(), 2);
    }

    #[test]
    fn preserves_update_and_insert_default_values() {
        let mut context = assignment_catalog(17);
        let update = resolve(
            "MERGE INTO identity_items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=DEFAULT",
            &mut context,
        )
        .unwrap();
        let assignment = update.as_merge().unwrap().clauses()[0]
            .action()
            .as_update()
            .unwrap()
            .assignments()[0]
            .value();
        assert!(assignment.is_default());

        let insert = resolve(
            "MERGE INTO generated_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(g) VALUES(DEFAULT)",
            &mut context,
        )
        .unwrap();
        let values = insert.as_merge().unwrap().clauses()[0]
            .action()
            .as_insert()
            .unwrap()
            .values()
            .unwrap();
        assert_eq!(values.len(), 1);
        assert!(values[0].is_default());

        let defaults = resolve(
            "MERGE INTO identity_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT DEFAULT VALUES",
            &mut context,
        )
        .unwrap();
        assert!(
            defaults.as_merge().unwrap().clauses()[0]
                .action()
                .as_insert()
                .unwrap()
                .values()
                .is_none()
        );
    }
}

impl SemMergeMatchKind {
    pub(crate) const fn ast_index(ast: &MergeMatchKind) -> usize {
        match ast {
            MergeMatchKind::Matched => 0,
            MergeMatchKind::NotMatchedBySource => 1,
            MergeMatchKind::NotMatchedByTarget => 2,
        }
    }
}

impl SemMergeMatchKind {
    pub(crate) fn from_ast_merge_match_kind(
        ast: &MergeMatchKind,
        context: &impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        match ast {
            MergeMatchKind::Matched => Ok(Self::matched()),
            MergeMatchKind::NotMatchedByTarget => Ok(Self::by_target()),
            MergeMatchKind::NotMatchedBySource => Self::from_ast_by_source(context),
        }
    }
}

impl SemMergeMatchKind {
    pub(crate) const fn ast_permits(ast: &MergeMatchKind, command: CmdType) -> bool {
        match command {
            CmdType::Nothing => true,
            CmdType::Insert => matches!(ast, MergeMatchKind::NotMatchedByTarget),
            CmdType::Update | CmdType::Delete => !matches!(ast, MergeMatchKind::NotMatchedByTarget),
            _ => false,
        }
    }
}

impl SemMergeMatchKind {
    fn from_ast_by_source(context: &impl ParseContext) -> Result<Self, AnalysisError> {
        MergeStatement::ast_require_version(context, 17)?;
        Ok(Self::by_source())
    }
}

impl MergeAction {
    pub(crate) fn from_ast_action(
        ast: &MergeWhen,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        target_scope: &ScalarAnalysisContext<'_>,
        target: &MergeDestination,
        alias: &AliasSpec,
    ) -> Result<Self, AnalysisError> {
        if !SemMergeMatchKind::ast_permits(&ast.match_kind(), ast.command_type()) {
            return Err(AnalysisError::unexpected_input_error(
                "illegal MERGE action for WHEN match kind",
            ));
        }
        match ast.command_type() {
            CmdType::Update => {
                Self::from_ast_update_action(ast, context, scope, target_scope, target, alias)
            }
            CmdType::Insert => {
                Self::from_ast_insert_action(ast, context, scope, target_scope, target, alias)
            }
            CmdType::Delete => Ok(Self::delete()),
            CmdType::Nothing => Ok(Self::nothing()),
            _ => Err(AnalysisError::unexpected_input_error(
                "unsupported MERGE action",
            )),
        }
    }
}

impl MergeAction {
    pub(crate) fn from_ast_update_action(
        ast: &MergeWhen,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        target_scope: &ScalarAnalysisContext<'_>,
        target: &MergeDestination,
        alias: &AliasSpec,
    ) -> Result<Self, AnalysisError> {
        if !target.update().is_writable() {
            return Err(AnalysisError::unexpected_input_error(
                "MERGE target does not accept UPDATE assignments",
            ));
        }
        let result = UpdateAssignmentCollection::resolve(
            context,
            ast.target_list(),
            target.update(),
            alias,
            target_scope,
            scope,
        );
        let Some(assignments) = ResolutionFailure::finish(result, context)? else {
            return Ok(Self::unresolved());
        };
        Ok(Self::update(&assignments))
    }
}

impl MergeAction {
    pub(crate) fn from_ast_insert_action(
        ast: &MergeWhen,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        target_scope: &ScalarAnalysisContext<'_>,
        target: &MergeDestination,
        alias: &AliasSpec,
    ) -> Result<Self, AnalysisError> {
        if !target.insert().is_writable() {
            return Err(AnalysisError::unexpected_input_error(
                "MERGE target does not accept INSERT assignments",
            ));
        }
        let values = if ast.values().is_empty() {
            None
        } else {
            Some(ast.values().as_slice())
        };
        let result = InsertClauseSemantics::resolve(
            context,
            ast.target_list(),
            &InsertTargetBinding::new(target.insert(), alias, target_scope),
            values,
            scope,
            Some(&ast.overriding()),
        );
        let Some(clause) = ResolutionFailure::finish(result, context)? else {
            return Ok(Self::unresolved());
        };
        let overriding = InsertClauseSemantics::ast_merge_override(&ast.overriding());
        Ok(Self::insert(&clause, overriding.as_deref()))
    }
}

#[cfg(test)]
mod test_retains_insert_overriding_mode {
    use crate::test_helpers::{
        merge_assignments::assignment_catalog, statements::resolve_postgres as resolve,
    };

    #[rstest::rstest]
    #[case::system("SYSTEM", "system")]
    #[case::user("USER", "user")]
    fn retains_insert_overriding_mode(#[case] mode: &str, #[case] semantic_mode: &str) {
        let mut context = assignment_catalog(17);
        let sql = format!(
            "MERGE INTO identity_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(id) OVERRIDING {mode} VALUE VALUES(s.sid)"
        );
        let statement = resolve(&sql, &mut context).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let insert = statement.as_merge().unwrap().clauses()[0]
            .action()
            .as_insert()
            .unwrap();
        assert_eq!(insert.overriding(), Some(semantic_mode));
    }
}

#[cfg(test)]
mod test_default_values_requires_insert_capability_but_no_assignable_columns {
    use crate::{
        sem::{ViewMergeSupport, ViewMergeTriggers},
        test_helpers::{merge_views::view_catalog, statements::resolve_postgres as resolve},
    };

    #[test]
    fn default_values_requires_insert_capability_but_no_assignable_columns() {
        let support =
            ViewMergeSupport::new(true, false, ViewMergeTriggers::default(), Some(&[]), None);
        let mut context = view_catalog(17, &support);
        let sql = "MERGE INTO writable_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT DEFAULT VALUES";
        resolve(sql, &mut context).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let support = ViewMergeSupport::new(true, false, ViewMergeTriggers::default(), None, None);
        let mut context = view_catalog(17, &support);
        assert!(resolve(sql, &mut context).is_err());
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

#[cfg(test)]
mod test_merge_cte_alias_is_visible_to_the_outer_query {
    use crate::{
        names::ColumnName,
        sem::{PgBuiltInType, SemAst},
        test_helpers::{
            create_cache_context, document_merge::catalog, merge_statements::statement,
        },
    };

    #[test]
    fn merge_cte_alias_is_visible_to_the_outer_query() {
        let mut cache_context = create_cache_context();
        let source = statement(
            "WITH changed(renamed_id) AS (MERGE INTO items AS t USING incoming AS s ON true WHEN MATCHED THEN UPDATE SET label = s.slabel RETURNING t.id) SELECT renamed_id FROM changed",
        );
        let analyzed =
            SemAst::from_ast(&source, catalog(17), &mut cache_context).expect("MERGE CTE analysis");
        let query = analyzed
            .ast()
            .clone()
            .into_select_statemtnt()
            .expect("SELECT result");
        let column = query
            .get_result_column_def(&ColumnName::from("renamed_id"))
            .expect("aliased MERGE output");
        assert_eq!(column.get_type(), Some(&PgBuiltInType::int4()));
    }

    #[test]
    fn nested_merge_cte_placement_is_rejected() {
        let mut cache_context = create_cache_context();
        let source = statement(
            "SELECT * FROM (WITH changed AS (MERGE INTO items AS t USING incoming AS s ON true WHEN MATCHED THEN UPDATE SET label = s.slabel RETURNING t.id) SELECT id FROM changed) AS nested",
        );
        let result = SemAst::from_ast(&source, catalog(17), &mut cache_context);
        assert!(match result {
            Err(_) => true,
            Ok(analyzed) => analyzed.context().reported_problem_count() > 0,
        });
    }
}
