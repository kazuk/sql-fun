use crate::{
    ast::{Dialect, Name, Target},
    context::ParseContext,
    names::{AliasName, ColumnName, FullName},
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, FromClause, InsertDestination,
        ResultColumnCollection, ScalarAnalysisContext, SemScalarExpr, WildcardExpansion,
        WildcardProjection, WithClause,
        problem::{DeleteProblemKind, InsertProblemKind, QueryProblem, UpdateProblemKind},
    },
};

impl ResultColumnCollection {
    pub(crate) fn resolve_for_merge(
        targets: &[Target],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let mut results = Self::default();
        for target in targets {
            if let Some(name) = target.source_star_name() {
                let qualifier = AliasName::from_ast_star_qualifier(name)?;
                if scope.check_returning_star(context, qualifier.as_ref())? {
                    continue;
                }
                let projection_qualifier = qualifier.as_ref().map(|alias| {
                    scope
                        .merge_row_target(alias)
                        .map_or(alias, |(target, _, _)| target)
                });
                if !results.expand_projection(
                    context,
                    scope.sources(),
                    scope,
                    target.projection_location(),
                    projection_qualifier,
                    qualifier
                        .as_ref()
                        .filter(|alias| scope.merge_row_target(alias).is_some()),
                )? {
                    return Err(AnalysisError::unexpected_input_error(
                        "MERGE RETURNING star source is not visible",
                    ));
                }
            } else {
                results.append_resolved_target(target, context, scope)?;
            }
        }
        Ok(results)
    }
}

#[cfg(test)]
mod test_postgres_18_merge_row_image_stars_expand_the_target_columns {
    use crate::{
        sem::SemAst,
        test_helpers::{
            create_cache_context, document_merge::catalog, merge_statements::statement,
        },
    };

    #[test]
    fn postgres_18_merge_row_image_stars_expand_the_target_columns() {
        let mut cache_context = create_cache_context();
        let source = statement(
            "MERGE INTO items AS t USING incoming AS s ON true WHEN MATCHED THEN UPDATE SET label = s.slabel WHEN NOT MATCHED BY SOURCE THEN DELETE WHEN NOT MATCHED BY TARGET THEN INSERT (id, label) VALUES (s.sid, s.slabel) RETURNING old.*, new.*",
        );
        let analyzed = SemAst::from_ast(&source, catalog(18), &mut cache_context)
            .expect("MERGE row-image analysis");
        assert_eq!(analyzed.ast().as_merge().unwrap().result_columns().len(), 4);
    }
}

#[cfg(test)]
mod test_merge_wildcard_columns_preserve_record_roles_after_positional_binding {
    use crate::test_helpers::wildcard::RetainedColumnsAssertions as _;

    use crate::{
        context::ParseContext, sem::MergeRecordRole,
        test_helpers::query::source_namespace_statement,
    };

    #[rstest::rstest]
    #[case::duplicate_join(
        "MERGE INTO a.u d USING (a.t CROSS JOIN b.t) j ON true WHEN MATCHED THEN DELETE RETURNING j.*"
    )]
    #[case::rows(
        "MERGE INTO a.u d USING ROWS FROM (mixed_fun(), mixed_fun()) r ON true WHEN MATCHED THEN DELETE RETURNING r.*"
    )]
    fn merge_wildcard_columns_preserve_record_roles_after_positional_binding(#[case] sql: &str) {
        let analyzed = source_namespace_statement(sql);
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let group = &analyzed.context().analysis_state().wildcard_expansions()[0];
        group.columns().assert_merge_role(MergeRecordRole::source());
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
        if let Some(base) = SemScalarExpr::ast_projection_base(ast.expression(), context, scope)? {
            return results.append_composite_star(context, base);
        }
        let expression = SemScalarExpr::from_ast_in(ast.expression(), context, scope)?;
        expression.validate_scalar_value(context)?;
        let name = ast
            .alias()
            .clone()
            .unwrap_or_else(|| expression.get_column_name());
        results.push_projection(
            &ColumnName::from(name.as_str()),
            expression,
            ast.alias().is_some() && !ast.implicit_alias(),
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
        projection.append_results(context, scope, results, ast.location(), None)?;
        if relation.is_none() && qualifier.is_none() {
            results.mark_join_sensitive_star_since(start);
        }
        Ok(())
    }
}

impl ResultColumnCollection {
    pub(crate) fn resolve_for_insert(
        targets: &[Target],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        table: &InsertDestination,
        alias: &AliasSpec,
    ) -> Result<Self, AnalysisError> {
        let mut results = Self::default();
        for target in targets {
            if let Some(name) = target.source_star_name() {
                if scope.check_returning_star(
                    context,
                    AliasName::from_ast_star_qualifier(name)?.as_ref(),
                )? {
                    continue;
                }
                Self::require_ast_returning_star(name, context, alias)?;
                let first = results.len();
                let mut index = 0;
                while let Some(column) = table.get_column_at(index) {
                    index += 1;
                    let column_name = column.name().as_ref().expect("catalog column name");
                    results.append_resolved_column(
                        column_name,
                        Some(alias.name()),
                        context,
                        scope,
                    )?;
                }
                context
                    .analysis_state_mut()
                    .record_wildcard(WildcardExpansion::new(
                        target.projection_location(),
                        &results,
                        first,
                        table.is_base_table(),
                        &[],
                    ));
            } else {
                results.append_resolved_target(target, context, scope)?;
            }
        }
        Ok(results)
    }
}

#[cfg(test)]
mod test_returning_retains_expression_alias_and_nullability {
    use crate::{
        ast::Dialect, names::ColumnName, sem::PgBuiltInType, test_helpers::insert::analyze,
    };

    #[test]
    fn returning_retains_expression_alias_and_nullability() {
        let (statement, context) = analyze(
            "INSERT INTO items AS i VALUES(1,'x') RETURNING i.label AS name, i.id",
            Dialect::postgres(),
        );
        assert_eq!(context.reported_problem_count(), 0);
        let columns = statement.as_insert().unwrap().result_columns();
        let label = columns.get_column_at(0).unwrap();
        assert_eq!(label.column_name(), &Some(ColumnName::from("name")));
        assert_eq!(label.col_type(), &Some(PgBuiltInType::text()));
        assert_eq!(label.is_not_null(), &Some(false));
        assert!(label.expression().is_some());
        assert_eq!(columns.get_column_at(1).unwrap().is_not_null(), &Some(true));
    }
}

impl ResultColumnCollection {
    pub(crate) fn resolve_for_update(
        targets: &[Target],
        context: &mut impl ParseContext,
        from: &FromClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let mut results = Self::default();
        for target_expr in targets {
            if let Some(name) = target_expr.source_star_name() {
                if context.dialect() == Dialect::sqlite() && !name.is_empty() {
                    context.report_analysis_problem(AnalysisProblem::update(
                        UpdateProblemKind::returning_qualifier(),
                        "SQLite RETURNING does not permit qualified stars",
                    ))?;
                }
                // Unqualified RETURNING * expands only the modified relation.
                let qualifier = AliasName::from_ast_star_qualifier(name)?;
                if scope.check_returning_star(context, qualifier.as_ref())? {
                    continue;
                }
                if !results.expand_returning_projection(
                    context,
                    from,
                    scope,
                    target_expr.projection_location(),
                    qualifier.as_ref(),
                    None,
                )? {
                    context.report_analysis_problem(AnalysisProblem::update(
                        UpdateProblemKind::returning_qualifier(),
                        "RETURNING star source is not visible",
                    ))?;
                }
            } else {
                results.append_resolved_target(target_expr, context, scope)?;
            }
        }
        Ok(results)
    }

    pub(crate) fn resolve_for_delete(
        targets: &[Target],
        context: &mut impl ParseContext,
        from: &FromClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let mut results = Self::default();
        for target_expr in targets {
            if let Some(name) = target_expr.source_star_name() {
                if context.dialect() == Dialect::sqlite() && !name.is_empty() {
                    context.report_analysis_problem(AnalysisProblem::delete(
                        DeleteProblemKind::returning_qualifier(),
                    ))?;
                }
                // Unqualified RETURNING * expands only the modified relation.
                let qualifier = AliasName::from_ast_star_qualifier(name)?;
                if scope.check_returning_star(context, qualifier.as_ref())? {
                    continue;
                }
                // PostgreSQL 18 can bind OLD and NEW to the modified row while
                // projection_names still needs the underlying target alias.
                let projection_qualifier = qualifier.as_ref().map(|alias| {
                    scope
                        .delete_row_target(alias)
                        .map_or(alias, |(target, _)| target)
                });
                if !results.expand_returning_projection(
                    context,
                    from,
                    scope,
                    target_expr.projection_location(),
                    projection_qualifier,
                    qualifier
                        .as_ref()
                        .filter(|alias| scope.delete_row_target(alias).is_some()),
                )? {
                    context.report_analysis_problem(AnalysisProblem::delete(
                        DeleteProblemKind::returning_qualifier(),
                    ))?;
                }
            } else {
                results.append_resolved_target(target_expr, context, scope)?;
            }
        }
        Ok(results)
    }
}

#[cfg(test)]
mod test_preserves_result_bindings_and_metadata {

    use crate::{
        ast::Dialect, context::BaseParseContext, names::ColumnName, sem::PgBuiltInType,
        test_helpers::delete::analyze,
    };

    #[test]
    fn preserves_result_bindings_and_metadata() {
        let (statement, context) = analyze(
            "DELETE FROM items AS i RETURNING i.id AS removed, label",
            Dialect::postgres(),
        );
        let delete = statement.as_delete().unwrap();
        let column = delete.result_columns().get_column_at(0).unwrap();
        assert_eq!(column.column_name(), &Some(ColumnName::from("removed")));
        assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
        assert_eq!(column.is_not_null(), &Some(true));
        assert_eq!(
            column
                .expression()
                .unwrap()
                .get_column_def()
                .unwrap()
                .name(),
            &Some(ColumnName::from("id"))
        );
        assert_eq!(
            delete
                .result_columns()
                .get_column_at(1)
                .unwrap()
                .is_not_null(),
            &Some(false)
        );
        assert!(
            context
                .get_table(&crate::names::TableName::from(
                    crate::names::FullName::from_local_name("items")
                ))
                .is_some()
        );
    }
}

impl ResultColumnCollection {
    pub(crate) fn append_resolved_target(
        &mut self,
        target: &Target,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<(), AnalysisError> {
        let value = SemScalarExpr::from_ast_in(target.expression(), context, scope)?;
        let name = target
            .alias()
            .clone()
            .unwrap_or_else(|| value.get_column_name());
        self.push_expression(&ColumnName::from(name.as_str()), value);
        Ok(())
    }

    pub(crate) fn append_resolved_column(
        &mut self,
        column: &ColumnName,
        qualifier: Option<&AliasName>,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<(), AnalysisError> {
        let value = scope.resolve_column(context, qualifier, column)?.0;
        self.push_expression(column, value);
        Ok(())
    }

    pub(crate) fn expand_projection(
        &mut self,
        context: &mut impl ParseContext,
        from: &FromClause,
        scope: &ScalarAnalysisContext<'_>,
        location: Option<usize>,
        projection_qualifier: Option<&AliasName>,
        binding_qualifier: Option<&AliasName>,
    ) -> Result<bool, AnalysisError> {
        let columns =
            from.wildcard_projection(context, scope.with_clause(), projection_qualifier)?;
        self.append_projection_columns(columns, context, scope, location, binding_qualifier)
    }

    fn expand_returning_projection(
        &mut self,
        context: &mut impl ParseContext,
        from: &FromClause,
        scope: &ScalarAnalysisContext<'_>,
        location: Option<usize>,
        projection_qualifier: Option<&AliasName>,
        binding_qualifier: Option<&AliasName>,
    ) -> Result<bool, AnalysisError> {
        let columns = if projection_qualifier.is_none() || binding_qualifier.is_some() {
            from.destination_wildcard_projection(context, scope.with_clause())?
        } else {
            from.wildcard_projection(context, scope.with_clause(), projection_qualifier)?
        };
        self.append_projection_columns(columns, context, scope, location, binding_qualifier)
    }

    fn append_projection_columns(
        &mut self,
        projection: WildcardProjection<'_>,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        location: Option<usize>,
        binding_qualifier: Option<&AliasName>,
    ) -> Result<bool, AnalysisError> {
        projection.append_results(context, scope, self, location, binding_qualifier)
    }
}

impl ResultColumnCollection {
    pub(crate) fn require_ast_returning_star(
        ast: &Name,
        context: &mut impl ParseContext,
        alias: &AliasSpec,
    ) -> Result<(), AnalysisError> {
        if !ast.parts().is_empty() && ast.parts() != [alias.name().as_str()] {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::returning_qualifier(),
                "RETURNING star qualifier does not identify the destination",
            ))?;
        }
        if !ast.parts().is_empty() && context.dialect() == Dialect::sqlite() {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::returning_qualifier(),
                "SQLite RETURNING does not permit qualified stars",
            ))?;
        }
        Ok(())
    }
}
