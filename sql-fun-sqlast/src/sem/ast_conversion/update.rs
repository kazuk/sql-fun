use crate::CacheContext;
use crate::sem::IndirectionExpr;
use crate::sem::InsertTargetCollection;
use crate::sem::SelectStatement;
use crate::sem::SemScalarExpr;

use crate::{
    ast::{Assignment, CurrentOf, Dialect, Expr, Subquery, Update},
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

impl UpdateStatement {
    pub(crate) fn from_ast(
        ast: &Update,
        context: &mut impl ParseContext,
        cache: &mut CacheContext,
    ) -> Result<Option<Self>, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_in(ast, context, None).and_then(|value| {
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
}

impl UpdateStatement {
    pub(crate) fn validate_ast_view_trigger(
        ast: &Update,
        context: &mut impl ParseContext,
        table: &UpdateDestination,
    ) -> Result<(), ResolutionFailure> {
        if let Some(support) = table.view_support() {
            let columns: Vec<_> = ast
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
}

impl UpdateStatement {
    pub(crate) fn from_ast_in(
        ast: &Update,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Option<Self>, AnalysisError> {
        let result = Self::from_ast_definition(ast, context, outer);
        ResolutionFailure::finish(result, context)
    }
}

impl UpdateStatement {
    pub(crate) fn from_ast_definition(
        ast: &Update,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, ResolutionFailure> {
        let trigger_outer = if let Some(scope) = outer {
            scope.trigger_scope()
        } else {
            None
        };
        let name = FullName::from_ast_name(&ast.relation.name)?;
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
        Self::validate_ast_view_trigger(ast, context, &table)?;
        let with = WithClause::from_ast(&ast.with, context, outer)?;
        let alias = ast
            .relation
            .alias
            .as_ref()
            .map(AliasSpec::from_ast)
            .unwrap_or_else(|| AliasSpec::from_local_name(&requested.full_name().local_name()));
        let from = Self::from_ast_sources(ast, context, &table, &alias, &with, trigger_outer)?;
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
            &ast.assignments,
            &table,
            &alias,
            &target_scope,
            &scope,
        )?;
        let predicate = Self::from_ast_predicate(ast, context, table.name(), &scope)?;
        let returning = Self::from_ast_returning_scope(ast, context, &requested, &table, &scope)?;
        let mut update = Self::new(
            table.name(),
            &assignments,
            &WithClause::ast_local_definitions(&ast.with, with),
            &from,
            &predicate,
            &returning,
        );
        update.set_modifiers(
            table.view_support(),
            &ast.or_conflict,
            &ast.relation.indexed,
            ast.relation.not_indexed,
            ast.relation.inherit,
        );
        update.validate_assignments(context)?;
        Ok(update)
    }
}

impl UpdateStatement {
    pub(crate) fn from_ast_destination(
        ast: &Update,
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
        if !ast.returning.is_empty()
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
}

impl UpdateStatement {
    pub(crate) fn validate_ast_index_hint(
        ast: &Update,
        context: &mut impl ParseContext,
        table: &UpdateDestination,
    ) -> Result<(), ResolutionFailure> {
        if let Some(index) = &ast.relation.indexed {
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
}

impl UpdateStatement {
    pub(crate) fn from_ast_sources(
        ast: &Update,
        context: &mut impl ParseContext,
        table: &UpdateDestination,
        alias: &AliasSpec,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<FromClause, ResolutionFailure> {
        // FROM subqueries cannot implicitly see the UPDATE target. Correlation
        // within RHS expressions uses the complete scope assembled below.
        let mut from = DataSource::from_ast_list_in(&ast.from, context, with, outer)?;
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

impl UpdateStatement {
    pub(crate) fn from_ast_predicate(
        ast: &Update,
        context: &mut impl ParseContext,
        target: &TableName,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<UpdatePredicate>, ResolutionFailure> {
        if let Some(expression) = &ast.predicate
            && let Some(cursor) = UpdateCursor::from_ast_current_of(expression, context, target)?
        {
            return Ok(Some(UpdatePredicate::current_of(&cursor)));
        }
        let predicate = SemScalarExpr::from_ast_optional(&ast.predicate, context, scope)?;
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
}

impl UpdateStatement {
    pub(crate) fn from_ast_returning_scope(
        ast: &Update,
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
                &ast.returning,
                context,
                &returning_from,
                &scope,
            )
            .map_err(Into::into)
        } else {
            let mut scope = ScalarAnalysisContext::nested(with, from, Some(outer));
            scope.set_postgres_returning(context);
            ResultColumnCollection::resolve_for_update(&ast.returning, context, from, &scope)
                .map_err(Into::into)
        }
    }
}

#[cfg(test)]
mod tests;

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
                    InsertTargetCollection::ast_overlaps_mapped(assignment, previous, |name| {
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
            let destination = target_scope
                .resolve_column(context, Some(alias.name()), &column_name)?
                .0;
            let destination = IndirectionExpr::from_ast_sequence(
                &assignment.indirection,
                context,
                rhs_scope,
                destination,
            )?;
            let value = UpdateValue::from_ast_assignment(assignment, context, rhs_scope)?;
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
            UpdateValue::validate_ast_update_value(assignment, context, column, &value)?;
            resolved.push(UpdateAssignment::new(column, &destination, &value));
        }
        Ok(resolved)
    }
}
impl UpdateValue {
    pub(crate) fn validate_ast_update_value(
        ast: &Assignment,
        context: &mut impl ParseContext,
        column: &ColumnDefinition,
        value: &Self,
    ) -> Result<(), ResolutionFailure> {
        if value.is_default() && !ast.indirection.is_empty() {
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
}

impl UpdateValue {
    pub(crate) fn from_ast_assignment(
        ast: &Assignment,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, ResolutionFailure> {
        let Some(value) = &ast.value else {
            return AnalysisError::raise_unexpected_input("missing UPDATE RHS").map_err(Into::into);
        };
        if let Some((source, position, width)) = value.multi_assignment_data() {
            let index = usize::try_from(position - 1)
                .map_err(|_| AnalysisError::unexpected_input_error("row assignment position"))?;
            return Self::from_ast_row(source, context, scope, index, width as usize);
        }
        Self::from_ast_expr(value, context, scope)
    }
}

impl UpdateCursor {
    pub(crate) fn from_ast_current_of(
        ast: &Expr,
        context: &mut impl ParseContext,
        target: &TableName,
    ) -> Result<Option<Self>, ResolutionFailure> {
        match ast {
            Expr::CurrentOf(cursor) => Self::from_ast(cursor, context, target).map(Some),
            _ => Ok(None),
        }
    }
}

impl UpdateValue {
    pub(crate) fn from_ast_row(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        index: usize,
        width: usize,
    ) -> Result<Self, ResolutionFailure> {
        match ast {
            Expr::Row(values) | Expr::Parenthesized(values) | Expr::List(values) => {
                Self::from_ast_update_row_values(values, context, scope, index, width)
            }
            Expr::Subquery(query) => {
                SemScalarExpr::from_ast_subquery_row(query.as_ref(), context, scope, index, width)
            }
            _ => Err(AnalysisError::unexpected_input_error(
                "row assignment requires row or subquery",
            )
            .into()),
        }
    }
}

impl UpdateValue {
    pub(crate) fn from_ast_update_row_values(
        values: &[Expr],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        index: usize,
        width: usize,
    ) -> Result<Self, ResolutionFailure> {
        if values.len() != width {
            return Err(
                AnalysisError::unexpected_input_error("row assignment width mismatch").into(),
            );
        }
        Self::from_ast_expr(
            values
                .get(index)
                .ok_or_else(|| AnalysisError::unexpected_input_error("row assignment index"))?,
            context,
            scope,
        )
    }
}

impl UpdateValue {
    pub(crate) fn from_ast_expr(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, ResolutionFailure> {
        match ast {
            Expr::Default => Ok(Self::default_value()),
            _ => Ok(Self::expression(&SemScalarExpr::from_ast_in(
                ast, context, scope,
            )?)),
        }
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_subquery_row(
        ast: &Subquery,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        index: usize,
        width: usize,
    ) -> Result<UpdateValue, ResolutionFailure> {
        let query = SelectStatement::from_ast_in(&ast.query, context, Some(scope))?;
        if !query.has_width(width) {
            return Err(
                AnalysisError::unexpected_input_error("row subquery width mismatch").into(),
            );
        }
        Ok(UpdateValue::query(&query, index))
    }
}

impl UpdateCursor {
    pub(crate) fn from_ast(
        ast: &CurrentOf,
        context: &mut impl ParseContext,
        target: &TableName,
    ) -> Result<Self, ResolutionFailure> {
        if ast.parameter != 0 {
            return Err(AnalysisError::unexpected_input_error(
                "CURRENT OF cursor parameters require a procedural variable resolver",
            )
            .into());
        }
        let cursor = Self::new(&ast.name, context.resolve_cursor(&ast.name));
        let problem = match cursor.binding() {
            None => Some(format!(
                "cursor {} does not exist in the current session scope",
                crate::identifier::QuotedIdentifier::from(ast.name.as_str())
            )),
            Some(info) if info.updatable() == Some(false) => Some(format!(
                "cursor {} is not statically updatable",
                crate::identifier::QuotedIdentifier::from(ast.name.as_str())
            )),
            Some(info) if info.target().is_some_and(|relation| relation != target) => {
                Some(format!(
                    "cursor {} does not target {}",
                    crate::identifier::QuotedIdentifier::from(ast.name.as_str()),
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
