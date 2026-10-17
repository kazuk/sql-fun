use crate::ast::{Alias, Delete, Dialect, Expr, FromItem, Target};
use crate::sem::DeleteDestination;
use crate::sem::problem::DeleteProblemKind;
use crate::sem::{
    AliasSpec, AnalysisError, AnalysisProblem, ColumnName, DataSource, DeleteStatement, FromClause,
    ParseContext, ResultColumnCollection, ScalarAnalysisContext, TableName, WithClause,
};

impl Delete {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<DeleteStatement, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context, None).and_then(|mut delete| {
            if context.parameter_inference_needed() {
                let constraints =
                    crate::sem::parameter_resolution::ParameterResolution::infer_delete(
                        &mut delete,
                        context,
                    )?;
                context.set_parameter_constraints(&constraints);
            }
            Ok(delete)
        });
        context.finish_analysis(result)
    }
    pub(super) fn resolve_in(
        &self,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<DeleteStatement, AnalysisError> {
        let requested = TableName::from(self.relation.name.full_name()?);
        let table = self.resolve_destination(context, &requested)?;
        self.validate_index_hint(context, &table)?;
        let with = self.with.resolve(context, outer)?;
        let alias = self
            .relation
            .alias
            .as_ref()
            .map(Alias::resolve)
            .unwrap_or_else(|| AliasSpec::from_local_name(&requested.full_name().local_name()));
        let from = self.resolve_sources(context, &table, &alias, &with)?;
        let scope = ScalarAnalysisContext::nested(&with, &from, None);
        let predicate = self.resolve_predicate(context, table.name(), &scope)?;
        let returning =
            self.resolve_returning_scope(context, &requested, &table, &alias, &with, &from)?;
        let mut delete = DeleteStatement::new(
            table.name(),
            table.found(),
            &self.with.local_definitions(with),
            &from,
            &predicate,
            &returning,
        );
        delete.set_modifiers(
            table.view_support(),
            &self.relation.indexed,
            self.relation.not_indexed,
            self.relation.inherit,
        );
        Ok(delete)
    }
    fn resolve_destination(
        &self,
        context: &mut impl ParseContext,
        requested: &TableName,
    ) -> Result<DeleteDestination, AnalysisError> {
        let Some(table) = DeleteDestination::resolve(context, requested) else {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(requested))?;
            return Ok(DeleteDestination::missing(requested));
        };
        if !table.is_deletable() {
            context.report_analysis_problem(AnalysisProblem::delete(
                DeleteProblemKind::read_only_target(),
            ))?;
        }
        if !self.returning.is_empty()
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
    fn validate_index_hint(
        &self,
        context: &mut impl ParseContext,
        table: &DeleteDestination,
    ) -> Result<(), AnalysisError> {
        if let Some(index) = &self.relation.indexed {
            let name = crate::sem::FullName::with_schema(
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
    fn resolve_sources(
        &self,
        context: &mut impl ParseContext,
        table: &DeleteDestination,
        alias: &AliasSpec,
        with: &WithClause,
    ) -> Result<FromClause, AnalysisError> {
        // USING subqueries cannot implicitly see the DELETE target. WHERE and
        // RETURNING subqueries use the complete scope assembled below.
        let mut from = FromItem::resolve_list_in(&self.using, context, with, None)?;
        if from.resolve_alias(alias.name()).is_some() {
            context.report_analysis_problem(AnalysisProblem::duplicate_alias(alias.name()))?;
        }
        from.push(DataSource::resolve_relation(
            context,
            table.name().full_name(),
            alias,
            &WithClause::default(),
        )?);
        Ok(from)
    }
    fn resolve_predicate(
        &self,
        context: &mut impl ParseContext,
        target: &TableName,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<crate::sem::DeletePredicate>, AnalysisError> {
        if let Some(expression) = &self.predicate
            && let Some(cursor) = expression.resolve_delete_current_of(context, target)?
        {
            return Ok(Some(crate::sem::DeletePredicate::current_of(&cursor)));
        }
        let predicate = Expr::resolve_optional(&self.predicate, context, scope)?;
        if context.dialect() == Dialect::postgres()
            && let Some(value) = &predicate
            && let Some(ty) = value.get_type()
            && ty != crate::sem::PgBuiltInType::bool()
            && !value
                .as_const()
                .is_some_and(|c| c.is_null() || c.as_string().is_some())
            && context
                .get_implicit_cast(&ty, &crate::sem::PgBuiltInType::bool())
                .is_none()
        {
            context
                .report_analysis_problem(AnalysisProblem::delete(DeleteProblemKind::predicate()))?;
        }
        Ok(predicate
            .as_ref()
            .map(crate::sem::DeletePredicate::expression))
    }
    fn resolve_returning_scope(
        &self,
        context: &mut impl ParseContext,
        requested: &TableName,
        table: &DeleteDestination,
        alias: &AliasSpec,
        with: &WithClause,
        from: &FromClause,
    ) -> Result<ResultColumnCollection, AnalysisError> {
        let mut returning_from = FromClause::default();
        let returning = if context.dialect() == Dialect::sqlite() {
            let returning_alias = AliasSpec::from_local_name(&requested.full_name().local_name());
            returning_from.push(DataSource::resolve_relation(
                context,
                table.name().full_name(),
                &returning_alias,
                &WithClause::default(),
            )?);
            self.resolve_returning(context, with, &returning_from, &returning_alias)?
        } else {
            self.resolve_returning(context, with, from, alias)?
        };
        Ok(returning)
    }
    fn resolve_returning(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        from: &FromClause,
        target: &AliasSpec,
    ) -> Result<ResultColumnCollection, AnalysisError> {
        let mut scope = ScalarAnalysisContext::nested(with, from, None);
        scope.set_delete_returning_target(target.name(), context);
        let mut results = ResultColumnCollection::default();
        for Target { expression, alias } in &self.returning {
            if let Expr::Star(name) = expression {
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
                let projection_qualifier = qualifier.as_ref().map(|alias| {
                    scope
                        .delete_row_target(alias)
                        .map_or(alias, |(target, _)| target)
                });
                let columns = from.projection_names(context, with, projection_qualifier)?;
                if columns.is_empty() {
                    context.report_analysis_problem(AnalysisProblem::delete(
                        DeleteProblemKind::returning_qualifier(),
                    ))?;
                }
                for (source, column) in columns {
                    let binding_alias = qualifier.as_ref().unwrap_or(&source);
                    let value = scope
                        .resolve_column(context, Some(binding_alias), &column)?
                        .0;
                    results.push_expression(&column, value);
                }
            } else {
                let value = expression.resolve_in(context, &scope)?;
                let name = alias.clone().unwrap_or_else(|| value.get_column_name());
                results.push_expression(&ColumnName::from(name.as_str()), value);
            }
        }
        Ok(results)
    }
}
impl Expr {
    fn resolve_delete_current_of(
        &self,
        context: &mut impl ParseContext,
        target: &TableName,
    ) -> Result<Option<crate::sem::DeleteCursor>, AnalysisError> {
        match self {
            Self::CurrentOf(cursor) => cursor.resolve_delete(context, target).map(Some),
            _ => Ok(None),
        }
    }
}
impl crate::ast::CurrentOf {
    fn resolve_delete(
        &self,
        context: &mut impl ParseContext,
        target: &TableName,
    ) -> Result<crate::sem::DeleteCursor, AnalysisError> {
        if self.parameter != 0 {
            context.report_analysis_problem(AnalysisProblem::delete(
                DeleteProblemKind::cursor_parameter(self.parameter),
            ))?;
            return Ok(crate::sem::DeleteCursor::parameter(self.parameter));
        }
        let cursor = crate::sem::DeleteCursor::new(&self.name, context.resolve_cursor(&self.name));
        let problem = match cursor.binding() {
            None => Some(DeleteProblemKind::missing_cursor(&self.name)),
            Some(info) if info.updatable() == Some(false) => {
                Some(DeleteProblemKind::non_updatable_cursor(&self.name))
            }
            Some(info) => info
                .target()
                .filter(|relation| *relation != target)
                .map(|actual| {
                    DeleteProblemKind::cursor_target_mismatch(&self.name, target, actual)
                }),
        };
        if let Some(kind) = problem {
            context.report_analysis_problem(AnalysisProblem::delete(kind))?;
        }
        Ok(cursor)
    }
}

#[cfg(test)]
mod tests;
