use crate::ast::{
    Alias, Assignment, Conflict, ConflictAction, Dialect, Expr, Insert, Name, OverridingKind,
    Query, QueryBody, Target,
};
use crate::sem::{
    AliasSpec, AnalysisError, AnalysisProblem, ColumnName, DataSource, FromClause, InsertConflict,
    InsertClauseSemantics, InsertClauseValues, InsertDestination, InsertSource, InsertStatement,
    InsertTarget, InsertTargetCollection, InsertValue, InsertValueCollection, ParseContext,
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
        let Some(table) = InsertDestination::resolve(context, &requested)? else {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(&requested))?;
            return Err(AnalysisError::unexpected_input_error(
                "INSERT destination relation not found",
            ));
        };
        if !table.is_writable() {
            context.report_analysis_problem(AnalysisProblem::insert(
                crate::sem::problem::InsertProblemKind::read_only_target(),
                "destination view does not support INSERT",
            ))?;
            return Err(AnalysisError::unexpected_input_error(
                "read-only INSERT destination",
            ));
        }
        if let Some(support) = table.view_support() {
            if !self.returning.is_empty() && !support.supports_returning() {
                context.report_analysis_problem(AnalysisProblem::insert(
                    crate::sem::problem::InsertProblemKind::view_operation(),
                    "destination view does not support INSERT RETURNING",
                ))?;
                return Err(AnalysisError::unexpected_input_error(
                    "unsupported view RETURNING",
                ));
            }
            if self.conflict.is_some() && !support.supports_on_conflict() {
                context.report_analysis_problem(AnalysisProblem::insert(
                    crate::sem::problem::InsertProblemKind::view_operation(),
                    "destination view does not support ON CONFLICT",
                ))?;
                return Err(AnalysisError::unexpected_input_error(
                    "unsupported view ON CONFLICT",
                ));
            }
        }
        let name = table.name().clone();
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
        let implicit_width = if context.dialect() == Dialect::postgres() {
            if self.source.is_some() {
                Some(source.width())
            } else {
                None
            }
        } else {
            None
        };
        let columns = InsertTargetCollection::resolve(
            context,
            &self.columns,
            &InsertTargetBinding::new(&table, &alias, &scope),
            InsertTargetMode::new(self.source.is_some(), implicit_width),
        )?;
        validate_insert_source(
            context,
            &columns,
            &source,
            self.source.is_some(),
            self.overriding.as_ref(),
        )?;
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
        insert.set_view_support(table.view_support());
        insert.set_modifiers(conflicts, overriding, self.or_conflict.clone());
        insert.validate_assignments(context)?;
        Ok(insert)
    }
}

/// Resolve one INSERT-like clause using independent destination and value scopes.
impl InsertClauseSemantics {
    pub(super) fn resolve(
        context: &mut impl ParseContext,
        assignments: &[Assignment],
        destination: &InsertTargetBinding<'_, '_>,
        values: Option<&[Expr]>,
        value_scope: &ScalarAnalysisContext<'_>,
        overriding: Option<&OverridingKind>,
    ) -> Result<Self, AnalysisError> {
        let implicit_width = values.map(|values| values.len());
        let targets = InsertTargetCollection::resolve(
            context,
            assignments,
            destination,
            InsertTargetMode::new(true, implicit_width),
        )?;
        let values = values
            .map(|values| resolve_insert_values(values, context, value_scope))
            .transpose()?;
        let clause = Self::new(
            targets,
            values.map_or(InsertClauseValues::DefaultValues, InsertClauseValues::Values),
        );
        validate_insert_clause(context, &clause, overriding)?;
        let mut clause = clause;
        clause.validate_assignments(context)?;
        Ok(clause)
    }
}

/// Destination-only bindings for an INSERT target list.
pub(super) struct InsertTargetBinding<'a, 'scope> {
    table: &'a InsertDestination,
    scope: &'a ScalarAnalysisContext<'scope>,
    alias: &'a AliasSpec,
}
impl<'a, 'scope> InsertTargetBinding<'a, 'scope> {
    pub(super) const fn new(
        table: &'a InsertDestination,
        alias: &'a AliasSpec,
        scope: &'a ScalarAnalysisContext<'scope>,
    ) -> Self {
        Self { table, scope, alias }
    }
}

#[derive(Clone, Copy)]
pub(super) struct InsertTargetMode {
    source_is_present: bool,
    implicit_width: Option<usize>,
}
impl InsertTargetMode {
    pub(super) const fn new(source_is_present: bool, implicit_width: Option<usize>) -> Self {
        Self {
            source_is_present,
            implicit_width,
        }
    }
}

/// Resolve destination bindings for an INSERT clause.
impl InsertTargetCollection {
    pub(super) fn resolve(
        context: &mut impl ParseContext,
        assignments: &[Assignment],
        destination: &InsertTargetBinding<'_, '_>,
        mode: InsertTargetMode,
    ) -> Result<Self, AnalysisError> {
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
                let destination = Name::from_parts(&[
                    alias.name().as_str().to_owned(),
                    name.as_str().to_owned(),
                ])
                .resolve_column(context, scope)?;
                columns.push(InsertTarget::new(column, destination));
            }
            // PostgreSQL permits the first N implicit columns, unlike SQLite.
            if context.dialect() == Dialect::postgres()
                && let Some(width) = mode.implicit_width
            {
                columns.truncate(width);
            }
        } else {
            let mut seen: Vec<&Assignment> = Vec::new();
            for assignment in assignments {
                if context.dialect() == Dialect::postgres()
                    && seen.iter().any(|previous| previous.overlaps(assignment))
                {
                    context.report_analysis_problem(AnalysisProblem::insert(
                        crate::sem::problem::InsertProblemKind::duplicate_target(),
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
                    return Err(AnalysisError::unexpected_input_error(
                        "INSERT destination column not found",
                    ));
                };
                let mut destination = Name::from_parts(&[
                    alias.name().as_str().to_owned(),
                    assignment.name.clone(),
                ])
                .resolve_column(context, scope)?;
                for access in &assignment.indirection {
                    destination = access.resolve(context, scope, destination)?;
                }
                columns.push(InsertTarget::new(column, destination));
            }
        }
        for target in columns.as_slice() {
            if mode.source_is_present
                && let Some(name) = target.column().name()
                && !table.can_assign(name)
            {
                context.report_analysis_problem(AnalysisProblem::insert(
                    crate::sem::problem::InsertProblemKind::read_only_target(),
                    &format!("view column {name} does not accept assignments"),
                ))?;
            }
        }
        Ok(columns)
    }
}

/// Resolve one direct VALUES row against the caller-selected expression scope.
pub(super) fn resolve_insert_values(
    expressions: &[Expr],
    context: &mut impl ParseContext,
    scope: &ScalarAnalysisContext<'_>,
) -> Result<InsertValueCollection, AnalysisError> {
    let mut values = InsertValueCollection::default();
    for expression in expressions {
        values.push(expression.resolve_insert_value(context, scope)?);
    }
    Ok(values)
}

fn validate_insert_clause(
    context: &mut impl ParseContext,
    clause: &InsertClauseSemantics,
    overriding: Option<&OverridingKind>,
) -> Result<(), AnalysisError> {
    let InsertClauseValues::Values(values) = clause.values() else {
        return Ok(());
    };
    validate_insert_width(
        context,
        values.len(),
        clause.targets().as_slice().len(),
        "INSERT source and destination column counts differ",
    )?;
    for (index, target) in clause.targets().as_slice().iter().enumerate() {
        let defaults_only = values
            .as_slice()
            .get(index)
            .is_some_and(InsertValue::is_default);
        validate_generated_target(
            context,
            target,
            defaults_only,
            overriding.is_some_and(OverridingKind::overrides_identity),
        )?;
    }
    Ok(())
}

fn validate_insert_source(
    context: &mut impl ParseContext,
    targets: &InsertTargetCollection,
    source: &InsertSource,
    source_is_present: bool,
    overriding: Option<&OverridingKind>,
) -> Result<(), AnalysisError> {
    if !source_is_present {
        return Ok(());
    }
    let width = targets.as_slice().len();
    validate_insert_width(
        context,
        source.width(),
        width,
        "INSERT source and destination column counts differ",
    )?;
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
            validate_insert_width(
                context,
                row.len(),
                width,
                "INSERT VALUES rows must match the destination column count",
            )?;
        }
    }
    for (index, target) in targets.as_slice().iter().enumerate() {
        let defaults_only = source.rows().is_some_and(|rows| {
            rows.iter()
                .all(|row| row.get(index).is_some_and(InsertValue::is_default))
        });
        validate_generated_target(
            context,
            target,
            defaults_only,
            overriding.is_some_and(OverridingKind::overrides_identity),
        )?;
    }
    Ok(())
}

fn validate_insert_width(
    context: &mut impl ParseContext,
    actual: usize,
    expected: usize,
    message: &str,
) -> Result<(), AnalysisError> {
    if actual != expected {
        context.report_analysis_problem(AnalysisProblem::insert(
            crate::sem::problem::InsertProblemKind::arity(),
            message,
        ))?;
    }
    Ok(())
}

fn validate_generated_target(
    context: &mut impl ParseContext,
    target: &InsertTarget,
    defaults_only: bool,
    overrides_identity: bool,
) -> Result<(), AnalysisError> {
    let constraints = target.column().constraints();
    if constraints
        .iter()
        .any(|constraint| constraint.as_generated().is_some())
        && !defaults_only
    {
        context.report_analysis_problem(AnalysisProblem::insert(
            crate::sem::problem::InsertProblemKind::generated_target(),
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
            crate::sem::problem::InsertProblemKind::identity_target(),
            "GENERATED ALWAYS identity requires DEFAULT or OVERRIDING",
        ))?;
    }
    Ok(())
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
            let mut source = InsertSource::empty_values();
            for row in rows {
                let values = resolve_insert_values(row, context, scope)?;
                source.push_values(&values);
            }
            return Ok(source);
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
    pub(super) fn overlaps(&self, other: &Self) -> bool {
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
        table: &InsertDestination,
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
        table: &InsertDestination,
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
        let mut inference = Vec::new();
        for item in &self.inference {
            if let Some(name) = &item.name {
                inference.push(Name::local(name).resolve_column(context, scope)?);
            }
            if let Some(expression) = &item.expression {
                inference.push(expression.resolve_insert_inference(context, scope)?);
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
        let mut modifiers = Vec::new();
        for item in &self.inference {
            modifiers.push(item.resolve_insert_modifiers(context)?);
        }
        conflict.set_modifiers(modifiers);
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
            let arbiters = if conflict.modifiers_are_bound() {
                context.infer_conflict_arbiters(name, &conflict)?
            } else {
                Vec::new()
            };
            conflict.set_arbiter(crate::sem::InsertArbiterLookup::inferred(&arbiters));
            if arbiters.is_empty() && conflict.modifiers_are_bound() {
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

impl crate::ast::IndexExpression {
    fn resolve_insert_modifiers(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<crate::sem::InsertInferenceModifiers, AnalysisError> {
        let mut names = Vec::new();
        if let Some(expression) = &self.expression {
            expression.collect_insert_collations(&mut names);
        }
        if !self.collation.0.is_empty() {
            names.push(&self.collation);
        }
        let mut collations = Vec::new();
        for name in names {
            let lookup = context.lookup_collation(&name.full_name()?);
            lookup.report_insert_problem(context)?;
            collations.push(lookup);
        }
        let operator_class = if self.operator_class.0.is_empty() {
            None
        } else {
            let lookup = context.lookup_operator_class(&self.operator_class.full_name()?, "btree");
            lookup.report_insert_problem(context)?;
            Some(lookup)
        };
        Ok(crate::sem::InsertInferenceModifiers::from_lookups(
            collations,
            operator_class,
        ))
    }
}
impl Expr {
    fn collect_insert_collations<'a>(&'a self, names: &mut Vec<&'a Name>) {
        match self {
            Self::Collate(collate) => {
                collate.argument.collect_insert_collations(names);
                names.push(&collate.name);
            }
            Self::Parenthesized(items) => {
                if let [expression] = items.as_slice() {
                    expression.collect_insert_collations(names);
                }
            }
            _ => {}
        }
    }
    fn resolve_insert_inference(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<crate::sem::SemScalarExpr, AnalysisError> {
        match self {
            Self::Collate(collate) => collate.argument.resolve_insert_inference(context, scope),
            Self::Parenthesized(items) => match items.as_slice() {
                [expression] => expression.resolve_insert_inference(context, scope),
                _ => self.resolve_in(context, scope),
            },
            _ => self.resolve_in(context, scope),
        }
    }
}
impl crate::sem::CatalogNameLookup {
    fn report_insert_problem(&self, context: &mut impl ParseContext) -> Result<(), AnalysisError> {
        let kind = if self.is_missing() {
            Some(crate::sem::problem::InsertProblemKind::conflict_name_missing())
        } else if self.ambiguous_candidates().is_some() {
            Some(crate::sem::problem::InsertProblemKind::conflict_name_ambiguous())
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
