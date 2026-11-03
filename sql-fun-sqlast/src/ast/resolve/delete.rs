use crate::{
    ast::{Alias, Delete, Dialect, Expr, FromItem},
    context::ParseContext,
    names::{FullName, TableName},
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, DataSource, DeleteCursor, DeleteDestination,
        DeletePredicate, DeleteStatement, FromClause, PgBuiltInType, ResultColumnCollection,
        ScalarAnalysisContext, WithClause, parameter_resolution::ParameterResolution,
        problem::DeleteProblemKind,
    },
};

impl Delete {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        cache: &mut crate::CacheContext,
    ) -> Result<DeleteStatement, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context, None).and_then(|mut delete| {
            if context.parameter_inference_needed() {
                let constraints = ParameterResolution::infer_delete(&mut delete, context, cache)?;
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
        let trigger_outer = if let Some(scope) = outer {
            scope.trigger_scope()
        } else {
            None
        };
        let name = self.relation.name.full_name()?;
        let name = if let Some(scope) = outer {
            scope.trigger_relation_name(context, &name)?
        } else {
            name
        };
        let requested = TableName::from(name);
        let table = self.resolve_destination(
            context,
            &requested,
            outer.is_some_and(ScalarAnalysisContext::in_rule_definition),
        )?;
        self.validate_index_hint(context, &table)?;
        let with = self.with.resolve(context, outer)?;
        let alias = self
            .relation
            .alias
            .as_ref()
            .map(Alias::resolve)
            .unwrap_or_else(|| AliasSpec::from_local_name(&requested.full_name().local_name()));
        let from = self.resolve_sources(context, &table, &alias, &with, trigger_outer)?;
        if let Some(outer) = outer {
            outer.validate_rule_sources(context, &from)?;
        }
        let scope = ScalarAnalysisContext::nested(&with, &from, trigger_outer);
        let predicate = self.resolve_predicate(context, table.name(), &scope)?;
        let returning =
            self.resolve_returning_scope(context, &requested, &table, &alias, &scope)?;
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
    fn resolve_sources(
        &self,
        context: &mut impl ParseContext,
        table: &DeleteDestination,
        alias: &AliasSpec,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<FromClause, AnalysisError> {
        // USING subqueries cannot implicitly see the DELETE target. WHERE and
        // RETURNING subqueries use the complete scope assembled below.
        let mut from = FromItem::resolve_list_in(&self.using, context, with, outer)?;
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
    fn resolve_predicate(
        &self,
        context: &mut impl ParseContext,
        target: &TableName,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<DeletePredicate>, AnalysisError> {
        if let Some(expression) = &self.predicate
            && let Some(cursor) = expression.resolve_delete_current_of(context, target)?
        {
            return Ok(Some(DeletePredicate::current_of(&cursor)));
        }
        let predicate = Expr::resolve_optional(&self.predicate, context, scope)?;
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
    fn resolve_returning_scope(
        &self,
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
                &self.returning,
                context,
                &returning_from,
                &scope,
            )
        } else {
            let mut scope = ScalarAnalysisContext::nested(with, from, Some(outer));
            scope.set_delete_returning_target(alias.name(), context);
            ResultColumnCollection::resolve_for_delete(&self.returning, context, from, &scope)
        }
    }
}
impl Expr {
    fn resolve_delete_current_of(
        &self,
        context: &mut impl ParseContext,
        target: &TableName,
    ) -> Result<Option<DeleteCursor>, AnalysisError> {
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
    ) -> Result<DeleteCursor, AnalysisError> {
        if self.parameter != 0 {
            context.report_analysis_problem(AnalysisProblem::delete(
                DeleteProblemKind::cursor_parameter(self.parameter),
            ))?;
            return Ok(DeleteCursor::parameter(self.parameter));
        }
        let cursor = DeleteCursor::new(&self.name, context.resolve_cursor(&self.name));
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
