use crate::ast::document::OverridingKind;
use crate::sem::{AstConversion, convert};
use crate::{
    CacheContext,
    ast::{
        Access, Assignment, Collate, Conflict, ConflictAction, Dialect, Expr, IndexExpression,
        Insert, Name, Query, QueryBody,
    },
    context::ParseContext,
    names::{ColumnName, TableName, ViewName},
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, CatalogNameLookup, DataSource, FromClause,
        InsertArbiterLookup, InsertClauseSemantics, InsertClauseValues, InsertConflict,
        InsertConstraintLookup, InsertDestination, InsertInferenceModifiers, InsertSource,
        InsertStatement, InsertTarget, InsertTargetCollection, InsertValue, InsertValueCollection,
        PgBuiltInType, ResolutionFailure, ResultColumnCollection, ScalarAnalysisContext,
        SelectStatement, SemScalarExpr, ViewWriteEvaluation, WithClause,
        parameter_resolution::ParameterResolution, problem::InsertProblemKind,
    },
};

impl AstConversion<'_, Insert> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        cache: &mut CacheContext,
    ) -> Result<Option<InsertStatement>, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context, None).and_then(|value| {
            let Some(mut statement) = value else {
                return Ok(None);
            };
            let implicit_targets =
                self.ast.columns.is_empty() && context.dialect() == Dialect::postgres();
            let expanded = if context.parameter_inference_needed() {
                let (constraints, changed) = ParameterResolution::infer_insert(
                    &mut statement,
                    context,
                    cache,
                    implicit_targets,
                )?;
                context.set_parameter_constraints(&constraints);
                changed
            } else {
                let changed = statement.expand_source_stars(context)?;
                if changed && implicit_targets {
                    statement.truncate_implicit_targets();
                }
                changed
            };
            if expanded {
                if ResolutionFailure::finish(
                    self.validate_expanded_source(&statement, context),
                    context,
                )?
                .is_none()
                {
                    return Ok(None);
                }
                statement.validate_assignments(context)?;
            }
            Ok(Some(statement))
        });
        context.finish_analysis(result)
    }

    pub(crate) fn validate_expanded_source(
        &self,
        statement: &InsertStatement,
        context: &mut impl ParseContext,
    ) -> Result<(), ResolutionFailure> {
        let source = statement.source();
        source.validate_width(context, statement.columns().len())?;
        for (index, target) in statement.columns().iter().enumerate() {
            let defaults_only = source.rows().is_some_and(|rows| {
                rows.iter()
                    .all(|row| row.get(index).is_some_and(InsertValue::is_default))
            });
            target.validate_value(
                context,
                defaults_only,
                self.ast
                    .overriding
                    .as_ref()
                    .is_some_and(|value| convert(value).overrides_identity()),
            )?;
            if let Some(name) = target.column().name()
                && statement
                    .view_support()
                    .is_some_and(|support| !support.can_assign(name))
            {
                context.report_analysis_problem(AnalysisProblem::insert(
                    InsertProblemKind::read_only_target(),
                    &format!("view column {name} does not accept assignments"),
                ))?;
            }
        }
        if self.ast.columns.is_empty()
            && context.dialect() == Dialect::postgres()
            && let Some(support) = statement.view_support()
        {
            let mut seen = Vec::new();
            for target in statement.columns() {
                let name = target.column().name().as_ref().expect("named destination");
                let key = support.assignment_column(name);
                if seen.contains(&key) {
                    context.report_analysis_problem(AnalysisProblem::insert(
                        InsertProblemKind::duplicate_target(),
                        "underlying column is assigned more than once",
                    ))?;
                }
                seen.push(key);
            }
        }
        Ok(())
    }
    pub(crate) fn resolve_in(
        &self,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Option<InsertStatement>, AnalysisError> {
        let result = self.resolve_definition(context, outer);
        ResolutionFailure::finish(result, context)
    }
    pub(crate) fn resolve_definition(
        &self,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<InsertStatement, ResolutionFailure> {
        if let Some(conflict) = &self.ast.conflict {
            convert(conflict).require_insert_action();
        }
        let name = convert(&self.ast.relation.name).full_name()?;
        let name = if let Some(scope) = outer {
            scope.trigger_relation_name(context, &name)?
        } else {
            name
        };
        let requested = TableName::from(name);
        let destination = if outer.is_some_and(ScalarAnalysisContext::in_rule_definition) {
            InsertDestination::resolve_rule_target(context, &requested)?
        } else {
            InsertDestination::resolve(context, &requested)?
        };
        let Some(table) = destination else {
            if context
                .resolve_relation_name(requested.full_name())
                .is_none()
            {
                context.report_analysis_problem(AnalysisProblem::relation_not_found(&requested))?;
            }
            return Err(ResolutionFailure::reported());
        };
        if !table.is_writable() {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::read_only_target(),
                "destination view does not support INSERT",
            ))?;
            return Err(ResolutionFailure::reported());
        }
        if let Some(support) = table.view_support() {
            if !self.ast.returning.is_empty() && !support.supports_returning() {
                context.report_analysis_problem(AnalysisProblem::insert(
                    InsertProblemKind::view_operation(),
                    "destination view does not support INSERT RETURNING",
                ))?;
                return Err(ResolutionFailure::reported());
            }
            if self.ast.conflict.is_some() && !support.supports_on_conflict() {
                context.report_analysis_problem(AnalysisProblem::insert(
                    InsertProblemKind::view_operation(),
                    "destination view does not support ON CONFLICT",
                ))?;
                return Err(ResolutionFailure::reported());
            }
        }
        let name = table.name().clone();
        let with = convert(&self.ast.with).resolve(context, outer)?;
        let empty = FromClause::default();
        let source_scope = ScalarAnalysisContext::rule_action_source(&with, &empty, outer);
        let source = self
            .ast
            .source
            .as_ref()
            .map(|query| convert(query).resolve_insert_source(context, &source_scope))
            .transpose()?
            .unwrap_or_else(InsertSource::default_values);
        let alias = self
            .ast
            .relation
            .alias
            .as_ref()
            .map(|value| convert(value).resolve())
            .unwrap_or_else(|| {
                AliasSpec::from_local_name(
                    &convert(&self.ast.relation.name)
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
        if let Some(outer) = outer {
            outer.validate_rule_sources(context, &from)?;
        }
        let scope = ScalarAnalysisContext::nested(&with, &from, outer);
        let implicit_width = if context.dialect() == Dialect::postgres() {
            if self.ast.source.is_some() {
                if source
                    .as_query()
                    .is_some_and(SelectStatement::has_pending_stars)
                {
                    None
                } else {
                    Some(source.width())
                }
            } else {
                None
            }
        } else {
            None
        };
        let pending_source = source
            .as_query()
            .is_some_and(SelectStatement::has_pending_stars);
        let columns = InsertTargetCollection::resolve(
            context,
            &self.ast.columns,
            &InsertTargetBinding::new(&table, &alias, &scope),
            &scope,
            InsertTargetMode::new(self.ast.source.is_some(), implicit_width, pending_source),
        )?;
        columns.validate_source(
            context,
            &source,
            self.ast.source.is_some(),
            self.ast.overriding.as_ref(),
        )?;
        let returning_alias = if context.dialect() == Dialect::sqlite() {
            AliasSpec::from_local_name(&convert(&self.ast.relation.name).full_name()?.local_name())
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
        let rule_returning = if let Some(scope) = outer {
            scope.insert_returning_outer(context)
        } else {
            None
        };
        let mut returning_scope =
            ScalarAnalysisContext::nested(&with, &returning_from, rule_returning.as_ref());
        returning_scope.set_postgres_returning(context);
        let returning = ResultColumnCollection::resolve_for_insert(
            &self.ast.returning,
            context,
            &returning_scope,
            &table,
            &returning_alias,
        )?;
        let conflicts = self
            .ast
            .conflict
            .as_ref()
            .map(|conflict| convert(conflict).resolve_insert(context, &scope, &table, &name))
            .transpose()?
            .unwrap_or_default();
        let mut insert = InsertStatement::new(
            &name,
            &columns,
            &source,
            &convert(&self.ast.with).local_definitions(with),
            &returning,
        );
        let overriding = if let Some(mode) = &self.ast.overriding {
            convert(mode).insert_mode()
        } else {
            None
        };
        insert.set_view_support(table.view_support());
        insert.set_modifiers(conflicts, overriding, self.ast.or_conflict.clone());
        if !insert.source_has_pending_stars() {
            insert.validate_assignments(context)?;
        }
        Ok(insert)
    }
}

#[cfg(test)]
mod tests;

/// Resolve one INSERT-like clause using independent destination and value scopes.
impl InsertClauseSemantics {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        assignments: &[Assignment],
        destination: &InsertTargetBinding<'_, '_>,
        values: Option<&[Expr]>,
        value_scope: &ScalarAnalysisContext<'_>,
        overriding: Option<&OverridingKind>,
    ) -> Result<Self, ResolutionFailure> {
        let implicit_width = values.map(|values| values.len());
        let targets = InsertTargetCollection::resolve(
            context,
            assignments,
            destination,
            value_scope,
            InsertTargetMode::new(values.is_some(), implicit_width, false),
        )?;
        let values = values
            .map(|values| InsertValueCollection::resolve(values, context, value_scope))
            .transpose()?;
        let clause = Self::new(&targets, &InsertClauseValues::from_values(values));
        clause.validate(context, overriding)?;
        Ok(clause)
    }
}

impl InsertClauseSemantics {
    fn validate(
        &self,
        context: &mut impl ParseContext,
        overriding: Option<&OverridingKind>,
    ) -> Result<(), ResolutionFailure> {
        if let Some(values) = self.values().values() {
            self.targets()
                .validate_values(context, values, overriding)?;
        }
        Ok(())
    }
}

/// Destination-only bindings for an INSERT target list.
pub(crate) struct InsertTargetBinding<'a, 'scope> {
    table: &'a InsertDestination,
    scope: &'a ScalarAnalysisContext<'scope>,
    alias: &'a AliasSpec,
}
impl<'a, 'scope> InsertTargetBinding<'a, 'scope> {
    pub(crate) const fn new(
        table: &'a InsertDestination,
        alias: &'a AliasSpec,
        scope: &'a ScalarAnalysisContext<'scope>,
    ) -> Self {
        Self {
            table,
            scope,
            alias,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct InsertTargetMode {
    source_is_present: bool,
    implicit_width: Option<usize>,
    pending_source: bool,
}
impl InsertTargetMode {
    pub(crate) const fn new(
        source_is_present: bool,
        implicit_width: Option<usize>,
        pending_source: bool,
    ) -> Self {
        Self {
            source_is_present,
            implicit_width,
            pending_source,
        }
    }
}

/// Resolve destination bindings for an INSERT clause.
impl InsertTargetCollection {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        assignments: &[Assignment],
        destination: &InsertTargetBinding<'_, '_>,
        expression_scope: &ScalarAnalysisContext<'_>,
        mode: InsertTargetMode,
    ) -> Result<Self, ResolutionFailure> {
        let table = destination.table;
        let alias = destination.alias;
        let scope = destination.scope;
        let mut columns = Self::default();
        if assignments.is_empty() {
            let mut index = 0;
            while let Some(column) = table.get_column_at(index) {
                index += 1;
                if context.dialect() == Dialect::sqlite()
                    && column
                        .constraints()
                        .iter()
                        .any(|constraint| constraint.as_generated().is_some())
                {
                    continue;
                }
                let name = column.name().as_ref().expect("catalog column name");
                let destination = scope.resolve_column(context, Some(alias.name()), name)?.0;
                columns.push(InsertTarget::new(column, destination));
            }
            // PostgreSQL permits the first N implicit columns, unlike SQLite.
            if context.dialect() == Dialect::postgres()
                && let Some(width) = mode.implicit_width
            {
                columns.truncate(width);
            }
            if context.dialect() == Dialect::postgres()
                && mode.source_is_present
                && !mode.pending_source
                && let Some(support) = table.view_support()
            {
                let mut seen = Vec::new();
                for target in columns.as_slice() {
                    let name = target.column().name().as_ref().expect("named destination");
                    let key = support.assignment_column(name);
                    if seen.contains(&key) {
                        context.report_analysis_problem(AnalysisProblem::insert(
                            InsertProblemKind::duplicate_target(),
                            "underlying column is assigned more than once",
                        ))?;
                    }
                    seen.push(key);
                }
            }
        } else {
            let mut seen: Vec<&Assignment> = Vec::new();
            for assignment in assignments {
                if context.dialect() == Dialect::postgres()
                    && seen.iter().any(|previous| {
                        convert(assignment).overlaps_mapped(previous, |name| {
                            table.view_support().map_or_else(
                                || name.clone(),
                                |support| support.assignment_column(name).clone(),
                            )
                        })
                    })
                {
                    context.report_analysis_problem(AnalysisProblem::insert(
                        InsertProblemKind::duplicate_target(),
                        &format!("column {} is assigned more than once", assignment.name),
                    ))?;
                }
                seen.push(assignment);
                let column_name = ColumnName::from(assignment.name.as_str());
                let Some(column) = table.get_column_def(&column_name) else {
                    context.report_analysis_problem(AnalysisProblem::column_not_found_in_table(
                        table.name(),
                        &column_name,
                    ))?;
                    return Err(ResolutionFailure::reported());
                };
                let destination = scope
                    .resolve_column(context, Some(alias.name()), &column_name)?
                    .0;
                let destination = AstConversion::<Access>::resolve_sequence(
                    &assignment.indirection,
                    context,
                    expression_scope,
                    destination,
                )?;
                columns.push(InsertTarget::new(column, destination));
            }
        }
        for target in columns.as_slice() {
            if mode.source_is_present
                && !mode.pending_source
                && let Some(name) = target.column().name()
                && !table.can_assign(name)
            {
                context.report_analysis_problem(AnalysisProblem::insert(
                    InsertProblemKind::read_only_target(),
                    &format!("view column {name} does not accept assignments"),
                ))?;
            }
        }
        Ok(columns)
    }

    fn validate_values(
        &self,
        context: &mut impl ParseContext,
        values: &InsertValueCollection,
        overriding: Option<&OverridingKind>,
    ) -> Result<(), ResolutionFailure> {
        validate_insert_width(
            context,
            values.len(),
            self.as_slice().len(),
            "INSERT source and destination column counts differ",
        )?;
        for (index, target) in self.as_slice().iter().enumerate() {
            target.validate_value(
                context,
                values
                    .as_slice()
                    .get(index)
                    .is_some_and(InsertValue::is_default),
                overriding.is_some_and(|value| convert(value).overrides_identity()),
            )?;
        }
        Ok(())
    }

    fn validate_source(
        &self,
        context: &mut impl ParseContext,
        source: &InsertSource,
        source_is_present: bool,
        overriding: Option<&OverridingKind>,
    ) -> Result<(), ResolutionFailure> {
        if !source_is_present {
            return Ok(());
        }
        if source
            .as_query()
            .is_some_and(SelectStatement::has_pending_stars)
        {
            return Ok(());
        }
        source.validate_width(context, self.as_slice().len())?;
        for (index, target) in self.as_slice().iter().enumerate() {
            let defaults_only = source.rows().is_some_and(|rows| {
                rows.iter()
                    .all(|row| row.get(index).is_some_and(InsertValue::is_default))
            });
            target.validate_value(
                context,
                defaults_only,
                overriding.is_some_and(|value| convert(value).overrides_identity()),
            )?;
        }
        Ok(())
    }
}

impl InsertValueCollection {
    pub(crate) fn resolve(
        expressions: &[Expr],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, ResolutionFailure> {
        let mut values = Self::default();
        for expression in expressions {
            values.push(convert(expression).resolve_insert_value(context, scope)?);
        }
        Ok(values)
    }
}

impl InsertTarget {
    fn validate_value(
        &self,
        context: &mut impl ParseContext,
        defaults_only: bool,
        overrides_identity: bool,
    ) -> Result<(), ResolutionFailure> {
        let constraints = self.column().constraints();
        if constraints
            .iter()
            .any(|constraint| constraint.as_generated().is_some())
            && !defaults_only
        {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::generated_target(),
                "generated column accepts only DEFAULT",
            ))?;
        }
        if constraints
            .iter()
            .filter_map(|constraint| constraint.as_identity())
            .any(|identity| identity.is_always())
            && !defaults_only
            && !overrides_identity
        {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::identity_target(),
                "GENERATED ALWAYS identity requires DEFAULT or OVERRIDING",
            ))?;
        }
        Ok(())
    }
}

impl InsertSource {
    pub(crate) fn validate_width(
        &self,
        context: &mut impl ParseContext,
        width: usize,
    ) -> Result<(), ResolutionFailure> {
        validate_insert_width(
            context,
            self.width(),
            width,
            "INSERT source and destination column counts differ",
        )?;
        if let Some(query) = self.as_query()
            && !query.has_width(width)
        {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::arity(),
                "INSERT query branches must match destination column count",
            ))?;
        }
        if let Some(rows) = self.rows() {
            for row in rows {
                validate_insert_width(
                    context,
                    row.len(),
                    width,
                    "INSERT VALUES rows must match the destination column count",
                )?;
            }
        }
        Ok(())
    }
}

fn validate_insert_width(
    context: &mut impl ParseContext,
    actual: usize,
    expected: usize,
    message: &str,
) -> Result<(), ResolutionFailure> {
    if actual != expected {
        context.report_analysis_problem(AnalysisProblem::insert(
            InsertProblemKind::arity(),
            message,
        ))?;
    }
    Ok(())
}

impl AstConversion<'_, Query> {
    pub(crate) fn resolve_insert_source(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<InsertSource, ResolutionFailure> {
        if let QueryBody::Values(rows) = &self.ast.body
            && self.ast.with.entries.is_empty()
            && self.ast.order.is_empty()
            && self.ast.limit.is_none()
            && self.ast.offset.is_none()
            && self.ast.locks.is_empty()
        {
            let mut source = InsertSource::empty_values();
            for row in rows {
                let values = InsertValueCollection::resolve(row, context, scope)?;
                source.push_values(&values);
            }
            return Ok(source);
        }
        Ok(InsertSource::query(self.resolve_in(context, Some(scope))?))
    }
}
impl<'ast> AstConversion<'ast, Expr> {
    pub(crate) fn resolve_insert_value(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<InsertValue, ResolutionFailure> {
        match self.ast {
            Expr::Default => Ok(InsertValue::default_value()),
            _ => Ok(InsertValue::expression(self.resolve_in(context, scope)?)),
        }
    }
}
impl AstConversion<'_, Assignment> {
    pub(crate) fn overlaps_mapped(
        &self,
        other: &Assignment,
        map: impl Fn(&ColumnName) -> ColumnName,
    ) -> bool {
        let same = map(&ColumnName::from(self.ast.name.as_str()))
            == map(&ColumnName::from(other.name.as_str()));
        same && (self.ast.indirection.is_empty() || other.indirection.is_empty())
    }
}
impl AstConversion<'_, ConflictAction> {
    pub(crate) fn require_insert_action(&self) {
        if matches!(self.ast, ConflictAction::Update) {
            todo!("ON CONFLICT DO UPDATE semantics are tracked in #474");
        }
    }
}
impl AstConversion<'_, Conflict> {
    pub(crate) fn require_insert_action(&self) {
        if let Some(action) = &self.ast.action {
            convert(action).require_insert_action();
        }
        if let Some(next) = &self.ast.next {
            convert(next.as_ref()).require_insert_action();
        }
    }
    pub(crate) fn resolve_insert(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        table: &InsertDestination,
        name: &TableName,
    ) -> Result<Vec<InsertConflict>, ResolutionFailure> {
        if context.dialect() == Dialect::sqlite()
            && self.ast.next.is_some()
            && self.ast.inference.is_empty()
        {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::conflict_chain(),
                "only the last SQLite UPSERT clause may omit its conflict target",
            ))?;
        }
        let mut inference = Vec::new();
        for item in &self.ast.inference {
            if let Some(name) = &item.name {
                inference.push(
                    scope
                        .resolve_column(context, None, &ColumnName::from(name.as_str()))?
                        .0
                        .mark_unqualified_column(),
                );
            }
            if let Some(expression) = &item.expression {
                inference.push(convert(expression).resolve_insert_inference(context, scope)?);
            }
        }
        let predicate =
            AstConversion::<Expr>::resolve_optional(&self.ast.inference_predicate, context, scope)?;
        if context.dialect() == Dialect::postgres()
            && let Some(value) = &predicate
            && let Some(ty) = value.get_type()
            && ty != PgBuiltInType::bool()
            && !value
                .as_const()
                .is_some_and(|value| value.is_null() || value.as_string().is_some())
            && context
                .get_implicit_cast(&ty, &PgBuiltInType::bool())
                .is_none()
        {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::conflict_predicate(),
                "ON CONFLICT predicate must have boolean type",
            ))?;
        }

        let mut conflict =
            InsertConflict::new(inference, predicate, self.ast.constraint.as_deref());
        let mut modifiers = Vec::new();
        for item in &self.ast.inference {
            modifiers.push(convert(item).resolve_insert_modifiers(context)?);
        }
        conflict.set_modifiers(modifiers);
        if let Some(constraint) = &self.ast.constraint {
            let lookup =
                InsertConstraintLookup::new(name, constraint, table.constraint(constraint));
            if !lookup.is_usable() {
                context.report_analysis_problem(AnalysisProblem::insert(
                    InsertProblemKind::conflict_constraint(),
                    "ON CONFLICT constraint is not a unique or primary key constraint of the destination",
                ))?;
            }
            conflict.set_arbiter(InsertArbiterLookup::named(&lookup));
        }
        if !self.ast.inference.is_empty() {
            let arbiters = if conflict.modifiers_are_bound() {
                match if context
                    .get_view(&ViewName::from_full_name(name.full_name()))
                    .is_some()
                {
                    ViewWriteEvaluation::new(context)
                        .infer_arbiters(name, &conflict)?
                        .ok_or_else(ResolutionFailure::reported)?
                } else {
                    context.infer_conflict_arbiters(name, &conflict)?
                }
                .into_result()
                {
                    Ok(arbiters) => arbiters,
                    Err(problem) => {
                        context.report_analysis_problem(*problem)?;
                        Vec::new()
                    }
                }
            } else {
                Vec::new()
            };
            conflict.set_arbiter(InsertArbiterLookup::inferred(&arbiters));
            if arbiters.is_empty() && conflict.modifiers_are_bound() {
                context.report_analysis_problem(AnalysisProblem::insert(
                    InsertProblemKind::conflict_target(),
                    "ON CONFLICT target has no matching unique key",
                ))?;
            }
        }
        let mut conflicts = vec![conflict];
        if let Some(next) = &self.ast.next {
            conflicts.extend(convert(next.as_ref()).resolve_insert(context, scope, table, name)?);
        }
        Ok(conflicts)
    }
}
impl AstConversion<'_, OverridingKind> {
    pub(crate) fn insert_mode(&self) -> Option<String> {
        match self.ast {
            OverridingKind::UserValue => Some("user".to_owned()),
            OverridingKind::SystemValue => Some("system".to_owned()),
            _ => None,
        }
    }
    pub(crate) const fn overrides_identity(&self) -> bool {
        matches!(
            self.ast,
            OverridingKind::UserValue | OverridingKind::SystemValue
        )
    }
}

impl AstConversion<'_, IndexExpression> {
    pub(crate) fn resolve_insert_modifiers(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<InsertInferenceModifiers, ResolutionFailure> {
        let mut names = Vec::new();
        if let Some(expression) = &self.ast.expression {
            convert(expression).collect_insert_collations(&mut names);
        }
        if !self.ast.collation.0.is_empty() {
            names.push(&self.ast.collation);
        }
        let mut collations = Vec::new();
        for name in names {
            let lookup = context.lookup_collation(&convert(name).full_name()?);
            lookup.report_insert_problem(context)?;
            collations.push(lookup);
        }
        let operator_class = if self.ast.operator_class.0.is_empty() {
            None
        } else {
            let lookup = context
                .lookup_operator_class(&convert(&self.ast.operator_class).full_name()?, "btree");
            lookup.report_insert_problem(context)?;
            Some(lookup)
        };
        Ok(InsertInferenceModifiers::from_lookups(
            collations,
            operator_class,
        ))
    }
}
impl<'ast> AstConversion<'ast, Expr> {
    pub(crate) fn collect_insert_collations(&self, names: &mut Vec<&'ast Name>) {
        match self.ast {
            Expr::Collate(collate) => convert(collate.as_ref()).collect_insert_collations(names),
            Expr::Parenthesized(items) => {
                Self::collect_parenthesized_insert_collations(items, names)
            }
            _ => {}
        }
    }
    fn collect_parenthesized_insert_collations(items: &'ast [Expr], names: &mut Vec<&'ast Name>) {
        if let [expression] = items {
            convert(expression).collect_insert_collations(names);
        }
    }

    fn resolve_parenthesized_insert_inference(
        &self,
        items: &[Expr],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, ResolutionFailure> {
        match items {
            [expression] => convert(expression).resolve_insert_inference(context, scope),
            _ => self.resolve_in(context, scope).map_err(Into::into),
        }
    }

    pub(crate) fn resolve_insert_inference(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, ResolutionFailure> {
        match self.ast {
            Expr::Collate(collate) => {
                convert(&collate.argument).resolve_insert_inference(context, scope)
            }
            Expr::Parenthesized(items) => {
                self.resolve_parenthesized_insert_inference(items, context, scope)
            }
            _ => self.resolve_in(context, scope).map_err(Into::into),
        }
    }
}
impl<'ast> AstConversion<'ast, Collate> {
    fn collect_insert_collations(&self, names: &mut Vec<&'ast Name>) {
        convert(&self.ast.argument).collect_insert_collations(names);
        names.push(&self.ast.name);
    }
}

impl CatalogNameLookup {
    fn report_insert_problem(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<(), ResolutionFailure> {
        let kind = if self.is_missing() {
            Some(InsertProblemKind::conflict_name_missing())
        } else if self.ambiguous_candidates().is_some() {
            Some(InsertProblemKind::conflict_name_ambiguous())
        } else {
            None
        };
        if let Some(kind) = kind {
            context.report_analysis_problem(AnalysisProblem::insert(
                kind,
                &format!(
                    "conflict target catalog name {} has no unique binding",
                    self.requested()
                ),
            ))?;
        }
        Ok(())
    }
}
