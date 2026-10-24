use crate::ast::{Dialect, Expr, Name, Target};
use crate::sem::problem::{DeleteProblemKind, InsertProblemKind, UpdateProblemKind};
use crate::sem::{
    AliasName, AliasSpec, AnalysisError, AnalysisProblem, ColumnName, FromClause,
    InsertDestination, ParseContext, ResultColumnCollection, ScalarAnalysisContext, WithClause,
};

impl ResultColumnCollection {
    pub(in crate::ast) fn resolve_for_insert(
        targets: &[Target],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        table: &InsertDestination,
        alias: &AliasSpec,
    ) -> Result<Self, AnalysisError> {
        let mut results = Self::default();
        for target in targets {
            if let Expr::Star(name) = &target.expression {
                name.require_returning_star(context, alias)?;
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
                    .record_wildcard(crate::sem::WildcardExpansion::new(
                        target.location,
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

    pub(in crate::ast) fn resolve_for_update(
        targets: &[Target],
        context: &mut impl ParseContext,
        with: &WithClause,
        from: &FromClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let mut results = Self::default();
        for target_expr in targets {
            if let Expr::Star(name) = &target_expr.expression {
                if context.dialect() == Dialect::sqlite() && !name.0.is_empty() {
                    context.report_analysis_problem(AnalysisProblem::update(
                        UpdateProblemKind::returning_qualifier(),
                        "SQLite RETURNING does not permit qualified stars",
                    ))?;
                }
                // Unqualified RETURNING * expands only the modified relation.
                let qualifier = name.star_qualifier()?;
                if !results.expand_returning_projection(
                    context,
                    with,
                    from,
                    scope,
                    target_expr.location,
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

    pub(in crate::ast) fn resolve_for_delete(
        targets: &[Target],
        context: &mut impl ParseContext,
        with: &WithClause,
        from: &FromClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let mut results = Self::default();
        for target_expr in targets {
            if let Expr::Star(name) = &target_expr.expression {
                if context.dialect() == Dialect::sqlite() && !name.0.is_empty() {
                    context.report_analysis_problem(AnalysisProblem::delete(
                        DeleteProblemKind::returning_qualifier(),
                    ))?;
                }
                // Unqualified RETURNING * expands only the modified relation.
                let qualifier = name.star_qualifier()?;
                // PostgreSQL 18 can bind OLD and NEW to the modified row while
                // projection_names still needs the underlying target alias.
                let projection_qualifier = qualifier.as_ref().map(|alias| {
                    scope
                        .delete_row_target(alias)
                        .map_or(alias, |(target, _)| target)
                });
                if !results.expand_returning_projection(
                    context,
                    with,
                    from,
                    scope,
                    target_expr.location,
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

    pub(in crate::ast) fn append_resolved_target(
        &mut self,
        target: &Target,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<(), AnalysisError> {
        let value = target.expression.resolve_in(context, scope)?;
        let name = target
            .alias
            .clone()
            .unwrap_or_else(|| value.get_column_name());
        self.push_expression(&ColumnName::from(name.as_str()), value);
        Ok(())
    }

    pub(in crate::ast) fn append_resolved_column(
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

    pub(in crate::ast) fn expand_projection(
        &mut self,
        context: &mut impl ParseContext,
        with: &WithClause,
        from: &FromClause,
        scope: &ScalarAnalysisContext<'_>,
        location: Option<usize>,
        projection_qualifier: Option<&AliasName>,
        binding_qualifier: Option<&AliasName>,
    ) -> Result<bool, AnalysisError> {
        let columns = from.wildcard_projection(context, with, projection_qualifier)?;
        self.append_projection_columns(columns, context, scope, location, binding_qualifier)
    }

    fn expand_returning_projection(
        &mut self,
        context: &mut impl ParseContext,
        with: &WithClause,
        from: &FromClause,
        scope: &ScalarAnalysisContext<'_>,
        location: Option<usize>,
        projection_qualifier: Option<&AliasName>,
        binding_qualifier: Option<&AliasName>,
    ) -> Result<bool, AnalysisError> {
        let columns = if projection_qualifier.is_none() || binding_qualifier.is_some() {
            from.destination_wildcard_projection(context, with)?
        } else {
            from.wildcard_projection(context, with, projection_qualifier)?
        };
        self.append_projection_columns(columns, context, scope, location, binding_qualifier)
    }

    fn append_projection_columns(
        &mut self,
        projection: crate::sem::WildcardProjection<'_>,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        location: Option<usize>,
        binding_qualifier: Option<&AliasName>,
    ) -> Result<bool, AnalysisError> {
        let (columns, source_found, table_dependent) = projection.into_parts();
        let first = self.len();
        let references: Vec<_> = columns
            .iter()
            .map(|column| {
                binding_qualifier
                    .map(|alias| vec![alias.as_str().to_owned(), column.name().as_str().to_owned()])
                    .or_else(|| column.visible_reference())
            })
            .collect();
        for column in &columns {
            let value = if let Some(qualifier) = binding_qualifier {
                scope
                    .resolve_column(context, Some(qualifier), column.name())?
                    .0
            } else {
                column.bind(context, scope)?
            };
            self.push_expression(column.name(), value);
        }
        let source_projections =
            crate::sem::WildcardSourceProjection::from_columns(&columns, context, scope)?;
        let mut expansion =
            crate::sem::WildcardExpansion::new(location, self, first, table_dependent, &references);
        expansion.retain_source_projections(&source_projections);
        expansion.retain_projection_origins(&columns);
        context.analysis_state_mut().record_wildcard(expansion);
        Ok(source_found)
    }
}

impl Name {
    fn require_returning_star(
        &self,
        context: &mut impl ParseContext,
        alias: &AliasSpec,
    ) -> Result<(), AnalysisError> {
        if !self.0.is_empty() && self.0.as_slice() != [alias.name().as_str()] {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::returning_qualifier(),
                "RETURNING star qualifier does not identify the destination",
            ))?;
        }
        if !self.0.is_empty() && context.dialect() == Dialect::sqlite() {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::returning_qualifier(),
                "SQLite RETURNING does not permit qualified stars",
            ))?;
        }
        Ok(())
    }
}
