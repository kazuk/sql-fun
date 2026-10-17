use crate::ast::{Alias, Assignment, Dialect, Expr, FromItem, Name, Target, Update};
use crate::sem::UpdateDestination;
use crate::sem::problem::UpdateProblemKind;
use crate::sem::{
    AliasSpec, AnalysisError, AnalysisProblem, ColumnName, DataSource, FromClause, ParseContext,
    ResultColumnCollection, ScalarAnalysisContext, TableName, UpdateAssignment, UpdateStatement,
    UpdateValue, WithClause,
};

impl Update {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<UpdateStatement, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context, None).and_then(|mut update| {
            if context.parameter_inference_needed() {
                let constraints =
                    crate::sem::parameter_resolution::ParameterResolution::infer_update(
                        &mut update,
                        context,
                    )?;
                context.set_parameter_constraints(&constraints);
            }
            Ok(update)
        });
        context.finish_analysis(result)
    }
    pub(super) fn resolve_in(
        &self,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<UpdateStatement, AnalysisError> {
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
        let assignments = self.resolve_assignments(context, &table, &alias, &with, &scope)?;
        let predicate = self.resolve_predicate(context, table.name(), &scope)?;
        let returning =
            self.resolve_returning_scope(context, &requested, &table, &alias, &with, &from)?;
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
    ) -> Result<UpdateDestination, AnalysisError> {
        let Some(table) = UpdateDestination::resolve(context, requested)? else {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(requested))?;
            return Err(AnalysisError::unexpected_input_error(
                "UPDATE destination not found",
            ));
        };
        if !table.is_writable() {
            context.report_analysis_problem(AnalysisProblem::update(
                UpdateProblemKind::read_only_target(),
                "destination does not support UPDATE",
            ))?;
            return Err(AnalysisError::unexpected_input_error(
                "read-only UPDATE destination",
            ));
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
    ) -> Result<FromClause, AnalysisError> {
        // FROM subqueries cannot implicitly see the UPDATE target. Correlation
        // within RHS expressions uses the complete scope assembled below.
        let mut from = FromItem::resolve_list_in(&self.from, context, with, None)?;
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
    fn resolve_assignments(
        &self,
        context: &mut impl ParseContext,
        table: &UpdateDestination,
        alias: &AliasSpec,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Vec<UpdateAssignment>, AnalysisError> {
        let mut target_from = FromClause::default();
        target_from.push(DataSource::resolve_relation(
            context,
            table.name().full_name(),
            alias,
            &WithClause::default(),
        )?);
        let target_scope = ScalarAnalysisContext::nested(with, &target_from, None);
        let mut assignments = Vec::new();
        let mut seen: Vec<&Assignment> = Vec::new();
        for assignment in &self.assignments {
            if context.dialect() == Dialect::postgres()
                && seen.iter().any(|previous| previous.overlaps(assignment))
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
                return Err(AnalysisError::unexpected_input_error(
                    "UPDATE column not found",
                ));
            };
            if !table.can_assign(&column_name) {
                context.report_analysis_problem(AnalysisProblem::update(
                    UpdateProblemKind::read_only_target(),
                    "view column does not accept UPDATE assignments",
                ))?;
            }
            let mut destination =
                Name::from_parts(&[alias.name().as_str().to_owned(), assignment.name.clone()])
                    .resolve_column(context, &target_scope)?;
            for access in &assignment.indirection {
                destination = access.resolve(context, scope, destination)?;
            }
            let value = assignment.resolve_update_value(context, scope)?;
            assignment.validate_update_value(context, column, &value)?;
            assignments.push(UpdateAssignment::new(column, &destination, &value));
        }
        Ok(assignments)
    }
    fn resolve_predicate(
        &self,
        context: &mut impl ParseContext,
        target: &TableName,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<crate::sem::UpdatePredicate>, AnalysisError> {
        if let Some(expression) = &self.predicate
            && let Some(cursor) = expression.resolve_current_of(context, target)?
        {
            return Ok(Some(crate::sem::UpdatePredicate::current_of(&cursor)));
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
            context.report_analysis_problem(AnalysisProblem::update(
                UpdateProblemKind::predicate(),
                "UPDATE WHERE requires boolean type",
            ))?;
        }
        Ok(predicate
            .as_ref()
            .map(crate::sem::UpdatePredicate::expression))
    }
    fn resolve_returning_scope(
        &self,
        context: &mut impl ParseContext,
        requested: &TableName,
        table: &UpdateDestination,
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
            self.resolve_returning(context, with, &returning_from, Some(&returning_alias))?
        } else {
            self.resolve_returning(context, with, from, Some(alias))?
        };
        Ok(returning)
    }
    fn resolve_returning(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        from: &FromClause,
        target: Option<&AliasSpec>,
    ) -> Result<ResultColumnCollection, AnalysisError> {
        let scope = ScalarAnalysisContext::nested(with, from, None);
        let mut results = ResultColumnCollection::default();
        for Target { expression, alias } in &self.returning {
            if let Expr::Star(name) = expression {
                if context.dialect() == Dialect::sqlite() && !name.0.is_empty() {
                    context.report_analysis_problem(AnalysisProblem::update(
                        UpdateProblemKind::returning_qualifier(),
                        "SQLite RETURNING does not permit qualified stars",
                    ))?;
                }
                // Unqualified RETURNING * expands only the modified relation.
                let qualifier = if name.0.is_empty() {
                    target.map(|a| a.name().clone())
                } else {
                    name.star_qualifier()?
                };
                let columns = from.projection_names(context, with, qualifier.as_ref())?;
                if columns.is_empty() {
                    context.report_analysis_problem(AnalysisProblem::update(
                        UpdateProblemKind::returning_qualifier(),
                        "RETURNING star source is not visible",
                    ))?;
                }
                for (source, column) in columns {
                    let value = scope.resolve_column(context, Some(&source), &column)?.0;
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
impl Assignment {
    fn validate_update_value(
        &self,
        context: &mut impl ParseContext,
        column: &crate::sem::ColumnDefinition,
        value: &UpdateValue,
    ) -> Result<(), AnalysisError> {
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
    ) -> Result<UpdateValue, AnalysisError> {
        let Some(value) = &self.value else {
            return AnalysisError::raise_unexpected_input("missing UPDATE RHS");
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
    ) -> Result<Option<crate::sem::UpdateCursor>, AnalysisError> {
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
    ) -> Result<UpdateValue, AnalysisError> {
        match self {
            Self::Row(values) | Self::Parenthesized(values) | Self::List(values) => {
                Self::resolve_update_row_values(values, context, scope, index, width)
            }
            Self::Subquery(query) => query.resolve_update_row(context, scope, index, width),
            _ => Err(AnalysisError::unexpected_input_error(
                "row assignment requires row or subquery",
            )),
        }
    }
    fn resolve_update_row_values(
        values: &[Self],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        index: usize,
        width: usize,
    ) -> Result<UpdateValue, AnalysisError> {
        if values.len() != width {
            return Err(AnalysisError::unexpected_input_error(
                "row assignment width mismatch",
            ));
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
    ) -> Result<UpdateValue, AnalysisError> {
        match self {
            Self::Default => Ok(UpdateValue::default_value()),
            _ => Ok(UpdateValue::expression(&self.resolve_in(context, scope)?)),
        }
    }
}

#[cfg(test)]
mod tests;

impl crate::ast::Subquery {
    fn resolve_update_row(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        index: usize,
        width: usize,
    ) -> Result<UpdateValue, AnalysisError> {
        let query = self.query.resolve_in(context, Some(scope))?;
        if !query.has_width(width) {
            return Err(AnalysisError::unexpected_input_error(
                "row subquery width mismatch",
            ));
        }
        Ok(UpdateValue::query(&query, index))
    }
}

impl crate::ast::CurrentOf {
    fn resolve_update(
        &self,
        context: &mut impl ParseContext,
        target: &TableName,
    ) -> Result<crate::sem::UpdateCursor, AnalysisError> {
        if self.parameter != 0 {
            return Err(AnalysisError::unexpected_input_error(
                "CURRENT OF cursor parameters require a procedural variable resolver",
            ));
        }
        let cursor = crate::sem::UpdateCursor::new(&self.name, context.resolve_cursor(&self.name));
        let problem = match cursor.binding() {
            None => Some(format!(
                "cursor {:?} does not exist in the current session scope",
                self.name
            )),
            Some(info) if info.updatable() == Some(false) => Some(format!(
                "cursor {:?} is not statically updatable",
                self.name
            )),
            Some(info) if info.target().is_some_and(|relation| relation != target) => {
                Some(format!("cursor {:?} does not target {}", self.name, target))
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
