mod placement;

use crate::ast::{
    Alias, Assignment, Conflict, ConflictAction, Dialect, Expr, Insert, Name, OverridingKind,
    Query, QueryBody, Target,
};
use crate::sem::{
    AliasSpec, AnalysisError, AnalysisProblem, ColumnName, CreateTable, DataSource, FromClause,
    InsertConflict, InsertSource, InsertStatement, InsertTarget, InsertValue, ParseContext,
    ResultColumnCollection, ScalarAnalysisContext, TableName, WithClause,
};

impl Insert {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<InsertStatement, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context, None);
        let result = result.and_then(|mut insert| {
            if context.parameter_inference_needed() {
                let constraints =
                    crate::sem::parameter_resolution::ParameterResolution::infer_insert(
                        &mut insert,
                        context,
                    )?;
                context.set_parameter_constraints(&constraints);
            }
            Ok(insert)
        });
        context.finish_analysis(result)
    }
    pub(super) fn resolve_in(
        &self,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<InsertStatement, AnalysisError> {
        if let Some(conflict) = &self.conflict {
            conflict.require_insert_action();
        }
        let requested = TableName::from(self.relation.name.full_name()?);
        let Some(name) = context.resolve_table_name(&requested) else {
            if context
                .get_view(&crate::sem::ViewName::from_full_name(requested.full_name()))
                .is_some()
            {
                todo!("INSERT into views requires writable-view catalog semantics");
            }
            context.report_analysis_problem(AnalysisProblem::relation_not_found(&requested))?;
            return Err(AnalysisError::unexpected_input_error(
                "INSERT destination relation not found",
            ));
        };
        let table = context
            .get_table(&name)
            .expect("resolved INSERT table")
            .clone();
        let with = self.with.resolve(context, outer)?;
        let empty = FromClause::default();
        let source_scope = ScalarAnalysisContext::nested(&with, &empty, outer);
        let source = self
            .source
            .as_ref()
            .map(|query| query.resolve_insert_source(context, &source_scope))
            .transpose()?
            .unwrap_or_else(InsertSource::default_values);
        let alias = self
            .relation
            .alias
            .as_ref()
            .map(Alias::resolve)
            .unwrap_or_else(|| {
                AliasSpec::from_local_name(
                    &self
                        .relation
                        .name
                        .full_name()
                        .expect("target name")
                        .local_name(),
                )
            });
        let mut from = FromClause::default();
        from.push(DataSource::resolve_relation(
            context,
            name.full_name(),
            &alias,
            &WithClause::default(),
        )?);
        let scope = ScalarAnalysisContext::nested(&with, &from, None);
        let columns = self.resolve_columns(context, &table, &scope, &alias, &source)?;
        self.validate_width(context, &source, columns.len())?;
        self.validate_generated_values(context, &columns, &source)?;
        let returning_alias = if context.dialect() == Dialect::sqlite() {
            AliasSpec::from_local_name(&self.relation.name.full_name()?.local_name())
        } else {
            alias.clone()
        };
        let mut returning_from = FromClause::default();
        returning_from.push(DataSource::resolve_relation(
            context,
            name.full_name(),
            &returning_alias,
            &WithClause::default(),
        )?);
        let returning_scope = ScalarAnalysisContext::nested(&with, &returning_from, None);
        let returning = Target::resolve_insert_returning(
            &self.returning,
            context,
            &returning_scope,
            &table,
            &returning_alias,
        )?;
        let conflicts = self
            .conflict
            .as_ref()
            .map(|conflict| conflict.resolve_insert(context, &scope, &table, &name))
            .transpose()?
            .unwrap_or_default();
        let mut insert = InsertStatement::new(
            &name,
            columns,
            source,
            self.with.local_definitions(with),
            returning,
        );
        let overriding = if let Some(mode) = &self.overriding {
            mode.insert_mode()
        } else {
            None
        };
        insert.set_modifiers(conflicts, overriding, self.or_conflict.clone());
        insert.validate_assignments(context)?;
        Ok(insert)
    }
    fn resolve_columns(
        &self,
        context: &mut impl ParseContext,
        table: &CreateTable,
        scope: &ScalarAnalysisContext<'_>,
        alias: &AliasSpec,
        source: &InsertSource,
    ) -> Result<Vec<InsertTarget>, AnalysisError> {
        let mut columns = Vec::new();
        if self.columns.is_empty() {
            let mut index = 0;
            while let Some(column) = table.get_column_at(index) {
                index += 1;
                if context.dialect() == Dialect::sqlite()
                    && column
                        .constraints()
                        .iter()
                        .any(|c| c.as_generated().is_some())
                {
                    continue;
                }
                let name = column.name().as_ref().expect("catalog column name");
                let destination =
                    Name::from_parts(&[alias.name().as_str().to_owned(), name.as_str().to_owned()])
                        .resolve_column(context, scope)?;
                columns.push(InsertTarget::new(column, destination));
            }
            // PostgreSQL permits the first N implicit columns, unlike SQLite.
            if context.dialect() == Dialect::postgres() && self.source.is_some() {
                columns.truncate(source.width());
            }
        } else {
            let mut seen: Vec<&Assignment> = Vec::new();
            for assignment in &self.columns {
                if context.dialect() == Dialect::postgres()
                    && seen.iter().any(|previous| previous.overlaps(assignment))
                {
                    context.report_analysis_problem(AnalysisProblem::insert(
                        crate::sem::problem::InsertProblemKind::duplicate_target(),
                        &format!("column {} is assigned more than once", assignment.name),
                    ))?;
                }
                seen.push(assignment);
                let Some(column) =
                    table.get_column_def(&ColumnName::from(assignment.name.as_str()))
                else {
                    context.report_analysis_problem(AnalysisProblem::column_not_found_in_table(
                        table.name(),
                        &ColumnName::from(assignment.name.as_str()),
                    ))?;
                    return Err(AnalysisError::unexpected_input_error(
                        "INSERT destination column not found",
                    ));
                };
                let mut destination =
                    Name::from_parts(&[alias.name().as_str().to_owned(), assignment.name.clone()])
                        .resolve_column(context, scope)?;
                for access in &assignment.indirection {
                    destination = access.resolve(context, scope, destination)?;
                }
                columns.push(InsertTarget::new(column, destination));
            }
        }
        Ok(columns)
    }
    fn validate_width(
        &self,
        context: &mut impl ParseContext,
        source: &InsertSource,
        width: usize,
    ) -> Result<(), AnalysisError> {
        if self.source.is_some() && source.width() != width {
            context.report_analysis_problem(AnalysisProblem::insert(
                crate::sem::problem::InsertProblemKind::arity(),
                "INSERT source and destination column counts differ",
            ))?;
        }
        if let Some(query) = source.as_query()
            && !query.has_width(width)
        {
            context.report_analysis_problem(AnalysisProblem::insert(
                crate::sem::problem::InsertProblemKind::arity(),
                "INSERT query branches must match destination column count",
            ))?;
        }
        if let Some(rows) = source.rows() {
            for row in rows {
                if row.len() != width {
                    context.report_analysis_problem(AnalysisProblem::insert(
                        crate::sem::problem::InsertProblemKind::arity(),
                        "INSERT VALUES rows must match the destination column count",
                    ))?;
                }
            }
        }
        Ok(())
    }
    fn validate_generated_values(
        &self,
        context: &mut impl ParseContext,
        columns: &[InsertTarget],
        source: &InsertSource,
    ) -> Result<(), AnalysisError> {
        if self.source.is_none() {
            return Ok(());
        }
        for (index, target) in columns.iter().enumerate() {
            let constraints = target.column().constraints();
            let defaults_only = source.rows().is_some_and(|rows| {
                rows.iter()
                    .all(|row| row.get(index).is_some_and(InsertValue::is_default))
            });
            if constraints.iter().any(|c| c.as_generated().is_some()) && !defaults_only {
                context.report_analysis_problem(AnalysisProblem::insert(
                    crate::sem::problem::InsertProblemKind::generated_target(),
                    "generated column accepts only DEFAULT",
                ))?;
            }
            if constraints
                .iter()
                .filter_map(|c| c.as_identity())
                .any(|c| c.is_always())
                && !defaults_only
                && !self
                    .overriding
                    .as_ref()
                    .is_some_and(OverridingKind::overrides_identity)
            {
                context.report_analysis_problem(AnalysisProblem::insert(
                    crate::sem::problem::InsertProblemKind::identity_target(),
                    "GENERATED ALWAYS identity requires DEFAULT or OVERRIDING",
                ))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

impl Query {
    fn resolve_insert_source(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<InsertSource, AnalysisError> {
        if let QueryBody::Values(rows) = &self.body
            && self.with.entries.is_empty()
            && self.order.is_empty()
            && self.limit.is_none()
            && self.offset.is_none()
            && self.locks.is_empty()
        {
            let mut resolved = Vec::new();
            for row in rows {
                let mut values = Vec::new();
                for value in row {
                    values.push(value.resolve_insert_value(context, scope)?);
                }
                resolved.push(values);
            }
            return Ok(InsertSource::values(resolved));
        }
        Ok(InsertSource::query(self.resolve_in(context, Some(scope))?))
    }
}
impl Expr {
    fn resolve_insert_value(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<InsertValue, AnalysisError> {
        match self {
            Self::Default => Ok(InsertValue::default_value()),
            _ => Ok(InsertValue::expression(self.resolve_in(context, scope)?)),
        }
    }
}
impl Assignment {
    fn overlaps(&self, other: &Self) -> bool {
        if self.name != other.name {
            return false;
        }
        // Multiple subfield/subscript assignments are legal. Whole-column
        // assignments conflict with every assignment to that column.
        self.indirection.is_empty() || other.indirection.is_empty()
    }
}
impl Target {
    fn resolve_insert_returning(
        targets: &[Self],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        table: &CreateTable,
        alias: &AliasSpec,
    ) -> Result<ResultColumnCollection, AnalysisError> {
        let mut results = ResultColumnCollection::default();
        for target in targets {
            if let Expr::Star(name) = &target.expression {
                name.require_returning_star(context, alias)?;
                let mut index = 0;
                while let Some(column) = table.get_column_at(index) {
                    index += 1;
                    let name = column.name().as_ref().expect("catalog column name");
                    let expression = Name::from_parts(&[
                        alias.name().as_str().to_owned(),
                        name.as_str().to_owned(),
                    ])
                    .resolve_column(context, scope)?;
                    results.push_expression(name, expression);
                }
            } else {
                let expression = target.expression.resolve_in(context, scope)?;
                let name = target
                    .alias
                    .clone()
                    .unwrap_or_else(|| expression.get_column_name());
                results.push_expression(&ColumnName::from(name.as_str()), expression);
            }
        }
        Ok(results)
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
                crate::sem::problem::InsertProblemKind::returning_qualifier(),
                "RETURNING star qualifier does not identify the destination",
            ))?;
        }
        if !self.0.is_empty() && context.dialect() == Dialect::sqlite() {
            context.report_analysis_problem(AnalysisProblem::insert(
                crate::sem::problem::InsertProblemKind::returning_qualifier(),
                "SQLite RETURNING does not permit qualified stars",
            ))?;
        }
        Ok(())
    }
}
impl ConflictAction {
    fn require_insert_action(&self) {
        if matches!(self, Self::Update) {
            todo!("ON CONFLICT DO UPDATE semantics are tracked in #474");
        }
    }
}
impl Conflict {
    fn require_insert_action(&self) {
        if let Some(action) = &self.action {
            action.require_insert_action();
        }
        if let Some(next) = &self.next {
            next.require_insert_action();
        }
    }
    fn resolve_insert(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        table: &CreateTable,
        name: &TableName,
    ) -> Result<Vec<InsertConflict>, AnalysisError> {
        if context.dialect() == Dialect::sqlite()
            && self.next.is_some()
            && self.inference.is_empty()
        {
            context.report_analysis_problem(AnalysisProblem::insert(
                crate::sem::problem::InsertProblemKind::conflict_chain(),
                "only the last SQLite UPSERT clause may omit its conflict target",
            ))?;
        }
        for item in &self.inference {
            if !item.collation.0.is_empty() || !item.operator_class.0.is_empty() {
                todo!("conflict collation/operator-class catalog resolution");
            }
        }
        let mut inference = Vec::new();
        for item in &self.inference {
            if let Some(name) = &item.name {
                inference.push(Name::local(name).resolve_column(context, scope)?);
            }
            if let Some(expression) = &item.expression {
                inference.push(expression.resolve_in(context, scope)?);
            }
        }
        let predicate = Expr::resolve_optional(&self.inference_predicate, context, scope)?;
        if context.dialect() == Dialect::postgres()
            && let Some(value) = &predicate
            && let Some(ty) = value.get_type()
            && ty != crate::sem::PgBuiltInType::bool()
            && !value
                .as_const()
                .is_some_and(|value| value.is_null() || value.as_string().is_some())
            && context
                .get_implicit_cast(&ty, &crate::sem::PgBuiltInType::bool())
                .is_none()
        {
            context.report_analysis_problem(AnalysisProblem::insert(
                crate::sem::problem::InsertProblemKind::conflict_predicate(),
                "ON CONFLICT predicate must have boolean type",
            ))?;
        }

        let mut conflict = InsertConflict::new(inference, predicate, self.constraint.as_deref());
        if let Some(constraint) = &self.constraint {
            let lookup = crate::sem::InsertConstraintLookup::new(
                name,
                constraint,
                table.constraint(constraint),
            );
            if !lookup.is_usable() {
                context.report_analysis_problem(AnalysisProblem::insert(
                    crate::sem::problem::InsertProblemKind::conflict_constraint(),
                    "ON CONFLICT constraint is not a unique or primary key constraint of the destination",
                ))?;
            }
            conflict.set_arbiter(crate::sem::InsertArbiterLookup::named(&lookup));
        }
        if !self.inference.is_empty() {
            let mut columns = Vec::new();
            let mut only_columns = true;
            for item in &self.inference {
                if let Some(name) = &item.name {
                    let name = ColumnName::from(name.as_str());
                    if !columns.contains(&name) {
                        columns.push(name);
                    }
                } else {
                    only_columns = false;
                }
            }
            let mut arbiters = if only_columns {
                table.infer_insert_arbiters(name, &columns)
            } else {
                Vec::new()
            };
            let mut incomplete_metadata = false;
            for index in context.indexes_for_table(name) {
                if let Some(eligible) = index.matches_inference(&conflict)? {
                    if eligible {
                        arbiters.push(crate::sem::InsertArbiter::index(index.name()));
                    }
                } else {
                    incomplete_metadata = true;
                }
            }
            if arbiters.is_empty() && incomplete_metadata {
                todo!(
                    "conflict arbiter requires predicate implication or implicit index modifier metadata"
                );
            }
            conflict.set_arbiter(crate::sem::InsertArbiterLookup::inferred(&arbiters));
            if arbiters.is_empty() {
                context.report_analysis_problem(AnalysisProblem::insert(
                    crate::sem::problem::InsertProblemKind::conflict_target(),
                    "ON CONFLICT target has no matching unique key",
                ))?;
            }
        }
        let mut conflicts = vec![conflict];
        if let Some(next) = &self.next {
            conflicts.extend(next.resolve_insert(context, scope, table, name)?);
        }
        Ok(conflicts)
    }
}
impl OverridingKind {
    fn insert_mode(&self) -> Option<String> {
        match self {
            Self::UserValue => Some("user".to_owned()),
            Self::SystemValue => Some("system".to_owned()),
            _ => None,
        }
    }
    const fn overrides_identity(&self) -> bool {
        matches!(self, Self::UserValue | Self::SystemValue)
    }
}
