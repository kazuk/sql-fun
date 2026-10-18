//! CREATE TRIGGER resolution consumes only the common syntax tree.
use crate::ast::{
    CreateTrigger, Dialect, StatementNode, TriggerBody, TriggerConstraint, TriggerEvent,
    TriggerFunction, TriggerGranularity, TriggerTiming,
};
use crate::sem::ResolutionFailure;
use crate::sem::problem::TriggerProblem;
use crate::sem::{
    self, AliasSpec, AnalysisError, ColumnName, DataSource, FromClause, FullName, ParseContext,
    ScalarAnalysisContext, SemAst, WithClause,
};

impl CreateTrigger {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<sem::CreateTrigger>, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context).and_then(|definition| {
            if context.parameter_inference_needed() {
                return Self::invalid(
                    &self.relation.name.full_name()?,
                    "trigger definitions cannot contain bind parameters",
                );
            }
            if context.analysis_state().has_problems() {
                return Self::invalid(
                    &self.relation.name.full_name()?,
                    "trigger contains unresolved or invalid expressions",
                );
            }
            Ok(definition)
        });
        let result = ResolutionFailure::finish(result, context);
        context.finish_analysis(result)
    }
    pub(crate) fn resolve_and_apply<C: ParseContext>(
        &self,
        mut context: C,
    ) -> Result<crate::AstAndContextPair<C>, AnalysisError> {
        let Some(definition) = self.resolve(&mut context)? else {
            return Ok(crate::AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        };
        let context = definition.apply(context)?;
        Ok(crate::AstAndContextPair::new(
            SemAst::create_trigger(&definition),
            context,
        ))
    }
    fn resolve_in(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<sem::CreateTrigger, ResolutionFailure> {
        let target = self.resolve_target(context)?;
        let sqlite = context.dialect() == Dialect::sqlite();
        let timing = self.timing.unwrap_or(TriggerTiming::Before);
        let granularity = self.granularity.unwrap_or(if sqlite {
            TriggerGranularity::Row
        } else {
            TriggerGranularity::Statement
        });
        self.validate(context, &target, timing, granularity)?;
        let identity = self.resolve_identity(context, &target)?;
        self.validate_identity(context, &identity)?;
        let events = self.resolve_events(context, &target)?;
        let with = WithClause::default();
        let from = self.resolve_row_bindings(context, &target, granularity, &with)?;
        let scope = self.analysis_scope(context, &target, &with, &from);
        let predicate = self.resolve_predicate(context, &scope, &target)?;
        let constraint = self
            .constraint
            .as_ref()
            .map(|constraint| constraint.resolve(context))
            .transpose()?;
        let transitions = self.resolve_transitions(context, &target);
        let body = self.body.resolve(context, &scope)?;
        let definition = sem::CreateTrigger::new(
            &identity,
            &target,
            timing.semantic(),
            granularity.semantic(),
            &body,
        );
        self.finish_definition(
            context,
            definition,
            &events,
            &predicate,
            &constraint,
            &transitions,
        )
    }
    fn resolve_events(
        &self,
        context: &impl ParseContext,
        target: &FullName,
    ) -> Result<Vec<sem::TriggerEvent>, ResolutionFailure> {
        let mut events = Vec::new();
        for event in &self.events {
            events.push(event.resolve(context, target)?);
        }
        Ok(events)
    }
    fn resolve_row_bindings(
        &self,
        context: &mut impl ParseContext,
        target: &FullName,
        granularity: TriggerGranularity,
        with: &WithClause,
    ) -> Result<FromClause, ResolutionFailure> {
        let mut from = FromClause::default();
        if granularity == TriggerGranularity::Row {
            if self.has_old() {
                from.push(DataSource::resolve_relation(
                    context,
                    target,
                    &AliasSpec::from_local_name("old"),
                    with,
                )?);
            }
            if self.has_new() {
                from.push(DataSource::resolve_relation(
                    context,
                    target,
                    &AliasSpec::from_local_name("new"),
                    with,
                )?);
            }
        }
        Ok(from)
    }
    fn analysis_scope<'a>(
        &self,
        context: &impl ParseContext,
        target: &FullName,
        with: &'a WithClause,
        from: &'a FromClause,
    ) -> ScalarAnalysisContext<'a> {
        let mut scope = ScalarAnalysisContext::trigger(with, from);
        if context.dialect() == Dialect::sqlite() && !self.temporary {
            scope.set_trigger_namespace(target.schema());
        }
        scope
    }
    fn resolve_predicate(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        target: &FullName,
    ) -> Result<Option<sem::SemScalarExpr>, ResolutionFailure> {
        let mut predicate = self
            .predicate
            .as_ref()
            .map(|expression| expression.resolve_in(context, scope))
            .transpose()?;
        if context.dialect() != Dialect::sqlite()
            && let Some(predicate) = &mut predicate
        {
            if self
                .predicate
                .as_ref()
                .is_some_and(crate::ast::Expr::untyped_trigger_predicate)
            {
                *predicate =
                    sem::SemScalarExpr::wrap_implicit_cast(&sem::PgBuiltInType::bool(), predicate);
            } else {
                sem::SemScalarExpr::implicit_cast_resolved(
                    context,
                    &sem::PgBuiltInType::bool(),
                    predicate,
                )?;
            }
            if predicate
                .get_type()
                .is_none_or(|ty| ty != sem::PgBuiltInType::bool())
            {
                return Self::invalid(target, "WHEN must have boolean type");
            }
            let mut inspected = predicate.clone();
            let mut validator = WhenValidator {
                context,
                invalid: false,
            };
            inspected.walk(&mut validator)?;
            if validator.invalid {
                return Self::invalid(
                    target,
                    "PostgreSQL WHEN cannot contain subqueries, aggregates, windows or set-returning functions",
                );
            }
        }
        Ok(predicate)
    }
    fn resolve_transitions(
        &self,
        _context: &impl ParseContext,
        target: &FullName,
    ) -> Vec<sem::TriggerTransition> {
        let mut transitions = Vec::new();
        for transition in &self.transitions {
            transitions.push(sem::TriggerTransition::new(
                &transition.name,
                transition.new,
                target,
            ));
        }
        transitions
    }
    fn finish_definition(
        &self,
        context: &impl ParseContext,
        mut definition: sem::CreateTrigger,
        events: &[sem::TriggerEvent],
        predicate: &Option<sem::SemScalarExpr>,
        constraint: &Option<sem::TriggerConstraint>,
        transitions: &[sem::TriggerTransition],
    ) -> Result<sem::CreateTrigger, ResolutionFailure> {
        definition.set_event_semantics(events, predicate, constraint, transitions);
        definition.set_creation_mode(self.replace, self.if_not_exists, self.temporary);
        definition.retain_dependencies(context)?;
        Ok(definition)
    }
    fn resolve_target(&self, context: &impl ParseContext) -> Result<FullName, ResolutionFailure> {
        let mut name = self.relation.name.full_name()?;
        // Non-TEMP SQLite triggers belong to the selected trigger schema and
        // cannot attach to a relation in another database.
        if context.dialect() == Dialect::sqlite() && !self.temporary {
            let trigger = self.name.full_name()?;
            let schema = trigger
                .schema()
                .cloned()
                .unwrap_or_else(|| sem::SchemaName::from("main"));
            if name.schema().is_none() {
                name = name.set_schema(schema);
            }
        }
        Self::relation(context, &name)
    }
    fn relation(
        context: &impl ParseContext,
        name: &FullName,
    ) -> Result<FullName, ResolutionFailure> {
        sem::search_path::expand_search_path_inst(context, name, |key| {
            if context.dialect() == Dialect::sqlite() {
                return context.sqlite_relation_identity(key);
            }
            if context.get_table_impl(key).is_some() || context.get_view_impl(key).is_some() {
                Some(key.clone())
            } else {
                None
            }
        })
        .ok_or_else(|| ResolutionFailure::trigger(&TriggerProblem::target(name)))
    }
    fn resolve_identity(
        &self,
        context: &impl ParseContext,
        target: &FullName,
    ) -> Result<sem::TriggerIdentity, ResolutionFailure> {
        let supplied = self.name.full_name()?;
        if context.dialect() == Dialect::postgres() {
            if supplied.schema().is_some() || supplied.catalog().is_some() {
                return Self::invalid(
                    target,
                    "PostgreSQL trigger names cannot be schema qualified",
                );
            }
            let name = FullName::new(
                &target.catalog().cloned(),
                &target.schema().cloned(),
                supplied.name(),
            );
            Ok(sem::TriggerIdentity::postgres(target, &name))
        } else {
            let schema = if self.temporary {
                sem::SchemaName::from("temp")
            } else {
                supplied
                    .schema()
                    .cloned()
                    .unwrap_or_else(|| sem::SchemaName::from("main"))
            };
            if self.temporary && supplied.schema().is_some() {
                return Self::invalid(target, "TEMP trigger name cannot be schema qualified");
            }
            if !self.temporary
                && target.schema().is_some_and(|target_schema| {
                    !target_schema.as_str().eq_ignore_ascii_case(schema.as_str())
                })
            {
                return Self::invalid(target, "trigger and target must use the same SQLite schema");
            }
            Ok(sem::TriggerIdentity::sqlite(&supplied.set_schema(schema)))
        }
    }
    fn validate_identity(
        &self,
        context: &impl ParseContext,
        identity: &sem::TriggerIdentity,
    ) -> Result<(), ResolutionFailure> {
        if let Some(existing) = context.trigger(identity)
            && !self.if_not_exists
            && (!self.replace || existing.constraint().is_some() || self.constraint.is_some())
        {
            return Err(ResolutionFailure::trigger(&TriggerProblem::duplicate(
                identity,
            )));
        }
        Ok(())
    }
    fn has_old(&self) -> bool {
        self.events.iter().all(TriggerEvent::has_old)
    }
    fn has_new(&self) -> bool {
        self.events.iter().all(TriggerEvent::has_new)
    }
    fn validate(
        &self,
        context: &impl ParseContext,
        target: &FullName,
        timing: TriggerTiming,
        granularity: TriggerGranularity,
    ) -> Result<(), ResolutionFailure> {
        let view = context.get_view_impl(target);
        if view.is_some_and(sem::ViewLike::is_materialized) {
            return Self::invalid(target, "materialized views cannot own triggers");
        }
        if self.events.is_empty() {
            return Self::invalid(target, "at least one event is required");
        }
        if context.dialect() == Dialect::sqlite() {
            if self.replace
                || self.constraint.is_some()
                || !self.transitions.is_empty()
                || granularity != TriggerGranularity::Row
                || self.events.len() != 1
                || self.events.iter().any(TriggerEvent::is_truncate)
            {
                return Self::invalid(target, "unsupported SQLite trigger option combination");
            }
            if (timing == TriggerTiming::InsteadOf) != view.is_some() {
                return Self::invalid(
                    target,
                    "SQLite views require INSTEAD OF triggers; tables require BEFORE or AFTER",
                );
            }
            return Ok(());
        }
        if self.temporary || self.if_not_exists {
            return Self::invalid(
                target,
                "PostgreSQL has no TEMP or IF NOT EXISTS trigger option",
            );
        }
        if timing == TriggerTiming::InsteadOf
            && (view.is_none()
                || granularity != TriggerGranularity::Row
                || self.predicate.is_some()
                || self.events.iter().any(TriggerEvent::has_columns))
        {
            return Self::invalid(
                target,
                "INSTEAD OF requires a row trigger on a view without WHEN or UPDATE OF",
            );
        }
        if view.is_some()
            && timing != TriggerTiming::InsteadOf
            && granularity != TriggerGranularity::Statement
        {
            return Self::invalid(target, "BEFORE/AFTER view triggers must be statement-level");
        }
        if self.events.iter().any(TriggerEvent::is_truncate)
            && (granularity != TriggerGranularity::Statement || view.is_some())
        {
            return Self::invalid(target, "TRUNCATE requires a statement trigger on a table");
        }
        if let Some(constraint) = &self.constraint
            && (timing != TriggerTiming::After
                || granularity != TriggerGranularity::Row
                || view.is_some()
                || (constraint.initially_deferred && !constraint.deferrable)
                || self.replace)
        {
            return Self::invalid(
                target,
                "invalid constraint trigger timing, target, deferrability or replacement",
            );
        }
        if !self.transitions.is_empty() {
            if timing != TriggerTiming::After
                || view.is_some()
                || self.constraint.is_some()
                || self.events.len() != 1
                || self.events.iter().any(TriggerEvent::has_columns)
                || self.events.iter().any(TriggerEvent::is_truncate)
            {
                return Self::invalid(target, "invalid transition-table combination");
            }
            let mut names = std::collections::HashSet::new();
            let mut kinds = std::collections::HashSet::new();
            for transition in &self.transitions {
                if !transition.table
                    || !names.insert(&transition.name)
                    || !kinds.insert(transition.new)
                    || (transition.new && !self.has_new())
                    || (!transition.new && !self.has_old())
                {
                    return Self::invalid(target, "invalid or duplicate transition-table binding");
                }
            }
        }
        Ok(())
    }
    fn invalid<T>(target: &FullName, rule: &str) -> Result<T, ResolutionFailure> {
        Err(ResolutionFailure::trigger(&TriggerProblem::options(
            target, rule,
        )))
    }
}

#[cfg(test)]
mod tests;

impl TriggerTiming {
    const fn semantic(self) -> sem::TriggerTiming {
        match self {
            Self::Before => sem::TriggerTiming::before(),
            Self::After => sem::TriggerTiming::after(),
            Self::InsteadOf => sem::TriggerTiming::instead_of(),
        }
    }
}
impl TriggerGranularity {
    const fn semantic(self) -> sem::TriggerGranularity {
        match self {
            Self::Row => sem::TriggerGranularity::row(),
            Self::Statement => sem::TriggerGranularity::statement(),
        }
    }
}
impl TriggerEvent {
    const fn has_old(&self) -> bool {
        matches!(self, Self::Update(_) | Self::Delete)
    }
    const fn has_new(&self) -> bool {
        matches!(self, Self::Update(_) | Self::Insert)
    }
    const fn is_truncate(&self) -> bool {
        matches!(self, Self::Truncate)
    }
    const fn has_columns(&self) -> bool {
        matches!(self, Self::Update(columns) if !columns.is_empty())
    }
    fn resolve(
        &self,
        context: &impl ParseContext,
        target: &FullName,
    ) -> Result<sem::TriggerEvent, ResolutionFailure> {
        match self {
            Self::Insert => Ok(sem::TriggerEvent::insert()),
            Self::Delete => Ok(sem::TriggerEvent::delete()),
            Self::Truncate => Ok(sem::TriggerEvent::truncate()),
            Self::Update(columns) => {
                let mut resolved = Vec::new();
                for name in columns {
                    let column = ColumnName::from(name.as_str());
                    let definition = sem::CreateTrigger::column(context, target, &column);
                    if definition.is_none() && context.dialect() == Dialect::postgres() {
                        return Err(ResolutionFailure::trigger(&TriggerProblem::column(
                            target, name,
                        )));
                    }
                    resolved.push(sem::TriggerEventColumn::new(&column, definition.as_ref()));
                }
                Ok(sem::TriggerEvent::update(&resolved))
            }
        }
    }
}
impl TriggerConstraint {
    fn resolve(
        &self,
        context: &impl ParseContext,
    ) -> Result<sem::TriggerConstraint, ResolutionFailure> {
        let relation = self
            .relation
            .as_ref()
            .map(|relation| CreateTrigger::relation(context, &relation.name.full_name()?))
            .transpose()?;
        Ok(sem::TriggerConstraint::new(
            &relation,
            self.deferrable,
            self.initially_deferred,
        ))
    }
}
impl TriggerBody {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<sem::TriggerBody, ResolutionFailure> {
        match self {
            Self::Function(function) => Ok(sem::TriggerBody::function(&function.resolve(context)?)),
            Self::Commands(commands) => {
                let mut resolved = Vec::new();
                for command in commands {
                    resolved.push(command.resolve_trigger_command(context, scope)?);
                }
                Ok(sem::TriggerBody::commands(&resolved))
            }
        }
    }
}
impl TriggerFunction {
    fn resolve(
        &self,
        context: &impl ParseContext,
    ) -> Result<sem::TriggerFunction, ResolutionFailure> {
        let requested = self.name.full_name()?;
        let name = context.resolve_function_name_with_search_path(&requested);
        let overloads = context
            .get_function_impl(&name)
            .ok_or_else(|| ResolutionFailure::trigger(&TriggerProblem::function(&requested)))?;
        let mut candidate = None;
        for overload in &overloads {
            if !overload.has_input_parameters() {
                if candidate.is_some() {
                    return Err(ResolutionFailure::trigger(&TriggerProblem::function(&name)));
                }
                candidate = Some(overload);
            }
        }
        let Some(candidate) = candidate else {
            return Err(ResolutionFailure::trigger(&TriggerProblem::function(&name)));
        };
        let Some(return_type) = candidate.ret_type() else {
            return Err(ResolutionFailure::trigger(&TriggerProblem::function(&name)));
        };
        if return_type.full_name() != &FullName::with_schema("pg_catalog", "trigger") {
            return Err(ResolutionFailure::trigger(&TriggerProblem::function(&name)));
        }
        if !context.function_preserves_view_rows(&name, candidate) {
            return Err(ResolutionFailure::trigger(&TriggerProblem::function(&name)));
        }
        Ok(sem::TriggerFunction::new(
            &name,
            &self.arguments,
            return_type,
        ))
    }
}
impl StatementNode {
    fn resolve_trigger_command(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemAst, ResolutionFailure> {
        match self {
            Self::Query(query) => query
                .resolve_in(context, Some(scope))
                .map(|statement| SemAst::select(&statement))
                .map_err(Into::into),
            Self::Insert(insert) => insert.resolve_trigger_command(context, scope),
            Self::Update(update) => update.resolve_trigger_command(context, scope),
            Self::Delete(delete) => delete.resolve_trigger_command(context, scope),
            _ => CreateTrigger::invalid(
                &FullName::from_local_name("trigger"),
                "illegal SQLite trigger body command",
            ),
        }
    }
}
struct WhenValidator<'a, C> {
    context: &'a C,
    invalid: bool,
}
impl<C: ParseContext> sem::traversal::SemanticVisitor for WhenValidator<'_, C> {
    fn enter_query(&mut self) {
        self.invalid = true;
    }
    fn expression(&mut self, expression: &mut sem::SemScalarExpr) -> Result<(), AnalysisError> {
        if !expression.preserves_view_rows(self.context) {
            self.invalid = true;
        }
        Ok(())
    }
}

impl crate::ast::Relation {
    fn validate_trigger_command(&self) -> Result<(), ResolutionFailure> {
        if self.name.0.len() != 1 || self.indexed.is_some() || self.not_indexed {
            return CreateTrigger::invalid(
                &self.name.full_name()?,
                "SQLite trigger DML requires an unqualified target without an index hint",
            );
        }
        Ok(())
    }
}
impl crate::ast::Insert {
    fn resolve_trigger_command(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemAst, ResolutionFailure> {
        self.relation.validate_trigger_command()?;
        if !self.returning.is_empty() || self.source.is_none() {
            return CreateTrigger::invalid(
                &self.relation.name.full_name()?,
                "SQLite trigger INSERT does not support RETURNING or DEFAULT VALUES",
            );
        }
        self.resolve_in(context, Some(scope))
            .map(|statement| {
                statement
                    .as_ref()
                    .map_or_else(SemAst::unresolved_statement, SemAst::insert)
            })
            .map_err(Into::into)
    }
}
impl crate::ast::Update {
    fn resolve_trigger_command(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemAst, ResolutionFailure> {
        self.relation.validate_trigger_command()?;
        if !self.returning.is_empty() {
            return CreateTrigger::invalid(
                &self.relation.name.full_name()?,
                "SQLite trigger UPDATE does not support RETURNING",
            );
        }
        self.resolve_in(context, Some(scope))
            .map(|statement| {
                statement
                    .as_ref()
                    .map_or_else(SemAst::unresolved_statement, SemAst::update)
            })
            .map_err(Into::into)
    }
}
impl crate::ast::Delete {
    fn resolve_trigger_command(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemAst, ResolutionFailure> {
        self.relation.validate_trigger_command()?;
        if !self.returning.is_empty() {
            return CreateTrigger::invalid(
                &self.relation.name.full_name()?,
                "SQLite trigger DELETE does not support RETURNING",
            );
        }
        self.resolve_in(context, Some(scope))
            .map(|statement| SemAst::delete(&statement))
            .map_err(Into::into)
    }
}

impl crate::ast::Expr {
    const fn untyped_trigger_predicate(&self) -> bool {
        match self {
            Self::Literal(literal) => literal.untyped_trigger_predicate(),
            _ => false,
        }
    }
}
impl crate::ast::Literal {
    const fn untyped_trigger_predicate(&self) -> bool {
        matches!(self, Self::Null | Self::String(_))
    }
}
