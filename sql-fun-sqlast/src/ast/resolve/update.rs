#[cfg(test)]
mod tests;

use crate::{CacheContext, ast::Access};

use crate::{
    ast::{Alias, Assignment, CurrentOf, Dialect, Expr, FromItem, Name, Subquery, Update},
    context::ParseContext,
    names::{ColumnName, FullName, TableName},
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, ColumnDefinition, DataSource, FromClause,
        PgBuiltInType, ResolutionFailure, ResultColumnCollection, ScalarAnalysisContext,
        UpdateAssignment, UpdateAssignmentCollection, UpdateCursor, UpdateDestination,
        UpdatePredicate, UpdateStatement, UpdateValue, WithClause,
        parameter_resolution::ParameterResolution, problem::UpdateProblemKind,
    },
};

impl Update {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        cache: &mut CacheContext,
    ) -> Result<Option<UpdateStatement>, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context, None).and_then(|value| {
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
    fn validate_view_trigger(
        &self,
        context: &mut impl ParseContext,
        table: &UpdateDestination,
    ) -> Result<(), ResolutionFailure> {
        if let Some(support) = table.view_support() {
            let columns: Vec<_> = self
                .assignments
                .iter()
                .map(|assignment| ColumnName::from(assignment.name.as_str()))
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
    pub(in crate::ast) fn resolve_in(
        &self,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Option<UpdateStatement>, AnalysisError> {
        let result = self.resolve_definition(context, outer);
        ResolutionFailure::finish(result, context)
    }
    fn resolve_definition(
        &self,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<UpdateStatement, ResolutionFailure> {
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
        self.validate_view_trigger(context, &table)?;
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
            &self.assignments,
            &table,
            &alias,
            &target_scope,
            &scope,
        )?;
        let predicate = self.resolve_predicate(context, table.name(), &scope)?;
        let returning = self.resolve_returning_scope(context, &requested, &table, &scope)?;
        let mut update = UpdateStatement::new(
            table.name(),
            &assignments,
            &self.with.local_definitions(with),
            &from,
            &predicate,
            &returning,
        );
        update.set_modifiers(
            table.view_support(),
            &self.or_conflict,
            &self.relation.indexed,
            self.relation.not_indexed,
            self.relation.inherit,
        );
        update.validate_assignments(context)?;
        Ok(update)
    }
    fn resolve_destination(
        &self,
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
        if !self.returning.is_empty()
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
    fn validate_index_hint(
        &self,
        context: &mut impl ParseContext,
        table: &UpdateDestination,
    ) -> Result<(), ResolutionFailure> {
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
                context.report_analysis_problem(AnalysisProblem::update(
                    UpdateProblemKind::index_hint(),
                    "INDEXED BY does not identify an index of the UPDATE destination",
                ))?;
            }
        }
        Ok(())
    }
    fn resolve_sources(
        &self,
        context: &mut impl ParseContext,
        table: &UpdateDestination,
        alias: &AliasSpec,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<FromClause, ResolutionFailure> {
        // FROM subqueries cannot implicitly see the UPDATE target. Correlation
        // within RHS expressions uses the complete scope assembled below.
        let mut from = FromItem::resolve_list_in(&self.from, context, with, outer)?;
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
    ) -> Result<Option<UpdatePredicate>, ResolutionFailure> {
        if let Some(expression) = &self.predicate
            && let Some(cursor) = expression.resolve_current_of(context, target)?
        {
            return Ok(Some(UpdatePredicate::current_of(&cursor)));
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
            context.report_analysis_problem(AnalysisProblem::update(
                UpdateProblemKind::predicate(),
                "UPDATE WHERE requires boolean type",
            ))?;
        }
        Ok(predicate.as_ref().map(UpdatePredicate::expression))
    }
    fn resolve_returning_scope(
        &self,
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
                &self.returning,
                context,
                &returning_from,
                &scope,
            )
            .map_err(Into::into)
        } else {
            let mut scope = ScalarAnalysisContext::nested(with, from, Some(outer));
            scope.set_postgres_returning(context);
            ResultColumnCollection::resolve_for_update(&self.returning, context, from, &scope)
                .map_err(Into::into)
        }
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
                    assignment.overlaps_mapped(previous, |name| {
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
            let column_name = ColumnName::from(assignment.name.as_str());
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
            let destination =
                Name::from_parts(&[alias.name().as_str().to_owned(), assignment.name.clone()])
                    .resolve_column(context, target_scope)?;
            let destination =
                Access::resolve_sequence(&assignment.indirection, context, rhs_scope, destination)?;
            let value = assignment.resolve_update_value(context, rhs_scope)?;
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
            assignment.validate_update_value(context, column, &value)?;
            resolved.push(UpdateAssignment::new(column, &destination, &value));
        }
        Ok(resolved)
    }
}
impl Assignment {
    fn validate_update_value(
        &self,
        context: &mut impl ParseContext,
        column: &ColumnDefinition,
        value: &UpdateValue,
    ) -> Result<(), ResolutionFailure> {
        if value.is_default() && !self.indirection.is_empty() {
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
    fn resolve_update_value(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<UpdateValue, ResolutionFailure> {
        let Some(value) = &self.value else {
            return AnalysisError::raise_unexpected_input("missing UPDATE RHS").map_err(Into::into);
        };
        if let Some((source, position, width)) = value.multi_assignment_data() {
            let index = usize::try_from(position - 1)
                .map_err(|_| AnalysisError::unexpected_input_error("row assignment position"))?;
            return source.resolve_update_row(context, scope, index, width as usize);
        }
        value.resolve_update_value(context, scope)
    }
}
impl Expr {
    fn resolve_current_of(
        &self,
        context: &mut impl ParseContext,
        target: &TableName,
    ) -> Result<Option<UpdateCursor>, ResolutionFailure> {
        match self {
            Self::CurrentOf(cursor) => cursor.resolve_update(context, target).map(Some),
            _ => Ok(None),
        }
    }
    fn resolve_update_row(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        index: usize,
        width: usize,
    ) -> Result<UpdateValue, ResolutionFailure> {
        match self {
            Self::Row(values) | Self::Parenthesized(values) | Self::List(values) => {
                Self::resolve_update_row_values(values, context, scope, index, width)
            }
            Self::Subquery(query) => query.resolve_update_row(context, scope, index, width),
            _ => Err(AnalysisError::unexpected_input_error(
                "row assignment requires row or subquery",
            )
            .into()),
        }
    }
    fn resolve_update_row_values(
        values: &[Self],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        index: usize,
        width: usize,
    ) -> Result<UpdateValue, ResolutionFailure> {
        if values.len() != width {
            return Err(
                AnalysisError::unexpected_input_error("row assignment width mismatch").into(),
            );
        }
        values
            .get(index)
            .ok_or_else(|| AnalysisError::unexpected_input_error("row assignment index"))?
            .resolve_update_value(context, scope)
    }

    fn resolve_update_value(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<UpdateValue, ResolutionFailure> {
        match self {
            Self::Default => Ok(UpdateValue::default_value()),
            _ => Ok(UpdateValue::expression(&self.resolve_in(context, scope)?)),
        }
    }
}

impl Subquery {
    fn resolve_update_row(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        index: usize,
        width: usize,
    ) -> Result<UpdateValue, ResolutionFailure> {
        let query = self.query.resolve_in(context, Some(scope))?;
        if !query.has_width(width) {
            return Err(
                AnalysisError::unexpected_input_error("row subquery width mismatch").into(),
            );
        }
        Ok(UpdateValue::query(&query, index))
    }
}

impl CurrentOf {
    fn resolve_update(
        &self,
        context: &mut impl ParseContext,
        target: &TableName,
    ) -> Result<UpdateCursor, ResolutionFailure> {
        if self.parameter != 0 {
            return Err(AnalysisError::unexpected_input_error(
                "CURRENT OF cursor parameters require a procedural variable resolver",
            )
            .into());
        }
        let cursor = UpdateCursor::new(&self.name, context.resolve_cursor(&self.name));
        let problem = match cursor.binding() {
            None => Some(format!(
                "cursor {} does not exist in the current session scope",
                crate::identifier::QuotedIdentifier::from(self.name.as_str())
            )),
            Some(info) if info.updatable() == Some(false) => Some(format!(
                "cursor {} is not statically updatable",
                crate::identifier::QuotedIdentifier::from(self.name.as_str())
            )),
            Some(info) if info.target().is_some_and(|relation| relation != target) => {
                Some(format!(
                    "cursor {} does not target {}",
                    crate::identifier::QuotedIdentifier::from(self.name.as_str()),
                    target
                ))
            }
            Some(_) => None,
        };
        if let Some(message) = problem {
            context.report_analysis_problem(AnalysisProblem::update(
                UpdateProblemKind::cursor(),
                &message,
            ))?;
        }
        Ok(cursor)
    }
}
