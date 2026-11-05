use crate::sem::SemScalarExpr;
use crate::{
    ast::{Dialect, Name, Target},
    context::ParseContext,
    names::{AliasName, ColumnName},
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, FromClause, InsertDestination,
        ResultColumnCollection, ScalarAnalysisContext, WildcardExpansion, WildcardProjection,
        problem::{DeleteProblemKind, InsertProblemKind, UpdateProblemKind},
    },
};

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

    pub(crate) fn append_resolved_target(
        &mut self,
        target: &Target,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<(), AnalysisError> {
        let value = SemScalarExpr::from_ast_in(&target.expression, context, scope)?;
        let name = target
            .alias
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
        if !ast.0.is_empty() && ast.0.as_slice() != [alias.name().as_str()] {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::returning_qualifier(),
                "RETURNING star qualifier does not identify the destination",
            ))?;
        }
        if !ast.0.is_empty() && context.dialect() == Dialect::sqlite() {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::returning_qualifier(),
                "SQLite RETURNING does not permit qualified stars",
            ))?;
        }
        Ok(())
    }
}
