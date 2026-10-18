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
        target: &AliasSpec,
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
                let qualifier = if name.0.is_empty() {
                    Some(target.name().clone())
                } else {
                    name.star_qualifier()?
                };
                if results.expand_projection(
                    context,
                    with,
                    from,
                    scope,
                    qualifier.as_ref(),
                    None,
                )? == 0
                {
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
        target: &AliasSpec,
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
                let qualifier = if name.0.is_empty() {
                    Some(target.name().clone())
                } else {
                    name.star_qualifier()?
                };
                // PostgreSQL 18 can bind OLD and NEW to the modified row while
                // projection_names still needs the underlying target alias.
                let projection_qualifier = qualifier.as_ref().map(|alias| {
                    scope
                        .delete_row_target(alias)
                        .map_or(alias, |(target, _)| target)
                });
                if results.expand_projection(
                    context,
                    with,
                    from,
                    scope,
                    projection_qualifier,
                    qualifier.as_ref(),
                )? == 0
                {
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
        projection_qualifier: Option<&AliasName>,
        binding_qualifier: Option<&AliasName>,
    ) -> Result<usize, AnalysisError> {
        let columns = from.projection_names(context, with, projection_qualifier)?;
        let count = columns.len();
        for (source, column) in columns {
            let qualifier = binding_qualifier.unwrap_or(&source);
            self.append_resolved_column(&column, Some(qualifier), context, scope)?;
        }
        Ok(count)
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
