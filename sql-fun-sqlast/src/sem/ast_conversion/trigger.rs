//! CREATE TRIGGER resolution consumes only the common syntax tree.

use crate::sem::{AstConversion, convert};

use crate::{
    AstAndContextPair,
    ast::{
        CreateTrigger, Delete, Dialect, Expr, Insert, Literal, Relation, StatementNode,
        TriggerBody, TriggerConstraint, TriggerEvent, TriggerFunction, TriggerGranularity,
        TriggerTiming, Update,
    },
    context::ParseContext,
    names::{ColumnName, FullName, SchemaName},
    sem::{
        AliasSpec, AnalysisError, CreateTrigger as SemCreateTrigger, DataSource, FromClause,
        PgBuiltInType, ResolutionFailure, ScalarAnalysisContext, SemAst, SemScalarExpr,
        TriggerBody as SemTriggerBody, TriggerConstraint as SemTriggerConstraint,
        TriggerEvent as SemTriggerEvent, TriggerEventColumn, TriggerFunction as SemTriggerFunction,
        TriggerGranularity as SemTriggerGranularity, TriggerIdentity,
        TriggerTiming as SemTriggerTiming, TriggerTransition, ViewLike, WithClause,
        problem::TriggerProblem, search_path::expand_search_path_inst,
        traversal::SemanticVisitorMut,
    },
};

impl AstConversion<'_, CreateTrigger> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<SemCreateTrigger>, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context).and_then(|definition| {
            if context.parameter_inference_needed() {
                return Self::invalid(
                    &convert(&self.ast.relation.name).full_name()?,
                    "trigger definitions cannot contain bind parameters",
                );
            }
            if context.analysis_state().has_problems() {
                return Self::invalid(
                    &convert(&self.ast.relation.name).full_name()?,
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
    ) -> Result<AstAndContextPair<C>, AnalysisError> {
        let Some(definition) = self.resolve(&mut context)? else {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        };
        let context = definition.apply(context)?;
        Ok(AstAndContextPair::new(
            SemAst::create_trigger(&definition),
            context,
        ))
    }
    pub(crate) fn resolve_in(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<SemCreateTrigger, ResolutionFailure> {
        let target = self.resolve_target(context)?;
        let sqlite = context.dialect() == Dialect::sqlite();
        let timing = self.ast.timing.unwrap_or(TriggerTiming::Before);
        let granularity = self.ast.granularity.unwrap_or(if sqlite {
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
            .ast
            .constraint
            .as_ref()
            .map(|constraint| convert(constraint).resolve(context))
            .transpose()?;
        let transitions = self.resolve_transitions(context, &target);
        let body = convert(&self.ast.body).resolve(context, &scope)?;
        let definition = SemCreateTrigger::new(
            &identity,
            &target,
            convert(&timing).semantic(),
            convert(&granularity).semantic(),
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
    pub(crate) fn resolve_events(
        &self,
        context: &impl ParseContext,
        target: &FullName,
    ) -> Result<Vec<SemTriggerEvent>, ResolutionFailure> {
        let mut events = Vec::new();
        for event in &self.ast.events {
            events.push(convert(event).resolve(context, target)?);
        }
        Ok(events)
    }
    pub(crate) fn resolve_row_bindings(
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
                    &AliasSpec::from_names("old", &[]),
                    with,
                )?);
            }
            if self.has_new() {
                from.push(DataSource::resolve_relation(
                    context,
                    target,
                    &AliasSpec::from_names("new", &[]),
                    with,
                )?);
            }
        }
        Ok(from)
    }
    pub(crate) fn analysis_scope<'a>(
        &self,
        context: &impl ParseContext,
        target: &FullName,
        with: &'a WithClause,
        from: &'a FromClause,
    ) -> ScalarAnalysisContext<'a> {
        let mut scope = ScalarAnalysisContext::trigger(with, from);
        if context.dialect() == Dialect::sqlite() && !self.ast.temporary {
            scope.set_trigger_namespace(target.schema());
        }
        scope
    }
    pub(crate) fn resolve_predicate(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        target: &FullName,
    ) -> Result<Option<SemScalarExpr>, ResolutionFailure> {
        let mut predicate = self
            .ast
            .predicate
            .as_ref()
            .map(|expression| convert(expression).resolve_in(context, scope))
            .transpose()?;
        if context.dialect() != Dialect::sqlite()
            && let Some(predicate) = &mut predicate
        {
            if self
                .ast
                .predicate
                .as_ref()
                .is_some_and(|value| convert(value).untyped_trigger_predicate())
            {
                *predicate = SemScalarExpr::wrap_implicit_cast(&PgBuiltInType::bool(), predicate);
            } else {
                SemScalarExpr::implicit_cast_resolved(context, &PgBuiltInType::bool(), predicate)?;
            }
            if predicate
                .get_type()
                .is_none_or(|ty| ty != PgBuiltInType::bool())
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
    pub(crate) fn resolve_transitions(
        &self,
        _context: &impl ParseContext,
        target: &FullName,
    ) -> Vec<TriggerTransition> {
        let mut transitions = Vec::new();
        for transition in &self.ast.transitions {
            transitions.push(TriggerTransition::new(
                &transition.name,
                transition.new,
                target,
            ));
        }
        transitions
    }
    pub(crate) fn finish_definition(
        &self,
        context: &impl ParseContext,
        mut definition: SemCreateTrigger,
        events: &[SemTriggerEvent],
        predicate: &Option<SemScalarExpr>,
        constraint: &Option<SemTriggerConstraint>,
        transitions: &[TriggerTransition],
    ) -> Result<SemCreateTrigger, ResolutionFailure> {
        definition.set_event_semantics(events, predicate, constraint, transitions);
        definition.set_creation_mode(self.ast.replace, self.ast.if_not_exists, self.ast.temporary);
        definition.retain_dependencies(context)?;
        Ok(definition)
    }
    pub(crate) fn resolve_target(
        &self,
        context: &impl ParseContext,
    ) -> Result<FullName, ResolutionFailure> {
        let mut name = convert(&self.ast.relation.name).full_name()?;
        // Non-TEMP SQLite triggers belong to the selected trigger schema and
        // cannot attach to a relation in another database.
        if context.dialect() == Dialect::sqlite() && !self.ast.temporary {
            let trigger = convert(&self.ast.name).full_name()?;
            let schema = trigger
                .schema()
                .cloned()
                .unwrap_or_else(|| SchemaName::from("main"));
            if name.schema().is_none() {
                name = name.set_schema(schema);
            }
        }
        Self::relation(context, &name)
    }
    pub(crate) fn relation(
        context: &impl ParseContext,
        name: &FullName,
    ) -> Result<FullName, ResolutionFailure> {
        expand_search_path_inst(context, name, |key| {
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
    pub(crate) fn resolve_identity(
        &self,
        context: &impl ParseContext,
        target: &FullName,
    ) -> Result<TriggerIdentity, ResolutionFailure> {
        let supplied = convert(&self.ast.name).full_name()?;
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
            Ok(TriggerIdentity::postgres(target, &name))
        } else {
            let schema = if self.ast.temporary {
                SchemaName::from("temp")
            } else {
                supplied
                    .schema()
                    .cloned()
                    .unwrap_or_else(|| SchemaName::from("main"))
            };
            if self.ast.temporary && supplied.schema().is_some() {
                return Self::invalid(target, "TEMP trigger name cannot be schema qualified");
            }
            if !self.ast.temporary
                && target.schema().is_some_and(|target_schema| {
                    !target_schema.as_str().eq_ignore_ascii_case(schema.as_str())
                })
            {
                return Self::invalid(target, "trigger and target must use the same SQLite schema");
            }
            Ok(TriggerIdentity::sqlite(&supplied.set_schema(schema)))
        }
    }
    pub(crate) fn validate_identity(
        &self,
        context: &impl ParseContext,
        identity: &TriggerIdentity,
    ) -> Result<(), ResolutionFailure> {
        if let Some(existing) = context.trigger(identity)
            && !self.ast.if_not_exists
            && (!self.ast.replace
                || existing.constraint().is_some()
                || self.ast.constraint.is_some())
        {
            return Err(ResolutionFailure::trigger(&TriggerProblem::duplicate(
                identity,
            )));
        }
        Ok(())
    }
    pub(crate) fn has_old(&self) -> bool {
        self.ast.events.iter().all(|value| convert(value).has_old())
    }
    pub(crate) fn has_new(&self) -> bool {
        self.ast.events.iter().all(|value| convert(value).has_new())
    }
    pub(crate) fn validate(
        &self,
        context: &impl ParseContext,
        target: &FullName,
        timing: TriggerTiming,
        granularity: TriggerGranularity,
    ) -> Result<(), ResolutionFailure> {
        let view = context.get_view_impl(target);
        if view.is_some_and(ViewLike::is_materialized) {
            return Self::invalid(target, "materialized views cannot own triggers");
        }
        if self.ast.events.is_empty() {
            return Self::invalid(target, "at least one event is required");
        }
        if context.dialect() == Dialect::sqlite() {
            if self.ast.replace
                || self.ast.constraint.is_some()
                || !self.ast.transitions.is_empty()
                || granularity != TriggerGranularity::Row
                || self.ast.events.len() != 1
                || self
                    .ast
                    .events
                    .iter()
                    .any(|value| convert(value).is_truncate())
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
        if self.ast.temporary || self.ast.if_not_exists {
            return Self::invalid(
                target,
                "PostgreSQL has no TEMP or IF NOT EXISTS trigger option",
            );
        }
        if timing == TriggerTiming::InsteadOf
            && (view.is_none()
                || granularity != TriggerGranularity::Row
                || self.ast.predicate.is_some()
                || self
                    .ast
                    .events
                    .iter()
                    .any(|value| convert(value).has_columns()))
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
        if self
            .ast
            .events
            .iter()
            .any(|value| convert(value).is_truncate())
            && (granularity != TriggerGranularity::Statement || view.is_some())
        {
            return Self::invalid(target, "TRUNCATE requires a statement trigger on a table");
        }
        if let Some(constraint) = &self.ast.constraint
            && (timing != TriggerTiming::After
                || granularity != TriggerGranularity::Row
                || view.is_some()
                || (constraint.initially_deferred && !constraint.deferrable)
                || self.ast.replace)
        {
            return Self::invalid(
                target,
                "invalid constraint trigger timing, target, deferrability or replacement",
            );
        }
        if !self.ast.transitions.is_empty() {
            if timing != TriggerTiming::After
                || view.is_some()
                || self.ast.constraint.is_some()
                || self.ast.events.len() != 1
                || self
                    .ast
                    .events
                    .iter()
                    .any(|value| convert(value).has_columns())
                || self
                    .ast
                    .events
                    .iter()
                    .any(|value| convert(value).is_truncate())
            {
                return Self::invalid(target, "invalid transition-table combination");
            }
            let mut names = std::collections::HashSet::new();
            let mut kinds = std::collections::HashSet::new();
            for transition in &self.ast.transitions {
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
    pub(crate) fn invalid<T>(target: &FullName, rule: &str) -> Result<T, ResolutionFailure> {
        Err(ResolutionFailure::trigger(&TriggerProblem::options(
            target, rule,
        )))
    }
}

#[cfg(test)]
mod tests;

impl AstConversion<'_, TriggerTiming> {
    pub(crate) const fn semantic(self) -> SemTriggerTiming {
        match self.ast {
            TriggerTiming::Before => SemTriggerTiming::before(),
            TriggerTiming::After => SemTriggerTiming::after(),
            TriggerTiming::InsteadOf => SemTriggerTiming::instead_of(),
        }
    }
}
impl AstConversion<'_, TriggerGranularity> {
    pub(crate) const fn semantic(self) -> SemTriggerGranularity {
        match self.ast {
            TriggerGranularity::Row => SemTriggerGranularity::row(),
            TriggerGranularity::Statement => SemTriggerGranularity::statement(),
        }
    }
}
impl AstConversion<'_, TriggerEvent> {
    pub(crate) const fn has_old(&self) -> bool {
        matches!(self.ast, TriggerEvent::Update(_) | TriggerEvent::Delete)
    }
    pub(crate) const fn has_new(&self) -> bool {
        matches!(self.ast, TriggerEvent::Update(_) | TriggerEvent::Insert)
    }
    pub(crate) const fn is_truncate(&self) -> bool {
        matches!(self.ast, TriggerEvent::Truncate)
    }
    pub(crate) const fn has_columns(&self) -> bool {
        matches!(self.ast, TriggerEvent::Update(columns) if !columns.is_empty())
    }
    pub(crate) fn resolve(
        &self,
        context: &impl ParseContext,
        target: &FullName,
    ) -> Result<SemTriggerEvent, ResolutionFailure> {
        match self.ast {
            TriggerEvent::Insert => Ok(SemTriggerEvent::insert()),
            TriggerEvent::Delete => Ok(SemTriggerEvent::delete()),
            TriggerEvent::Truncate => Ok(SemTriggerEvent::truncate()),
            TriggerEvent::Update(columns) => {
                SemTriggerEvent::from_ast_update(columns, context, target)
            }
        }
    }
}
impl SemTriggerEvent {
    fn from_ast_update(
        columns: &[String],
        context: &impl ParseContext,
        target: &FullName,
    ) -> Result<Self, ResolutionFailure> {
        let mut resolved = Vec::new();
        for name in columns {
            let column = ColumnName::from(name.as_str());
            let definition = SemCreateTrigger::column(context, target, &column);
            if definition.is_none() && context.dialect() == Dialect::postgres() {
                return Err(ResolutionFailure::trigger(&TriggerProblem::column(
                    target, name,
                )));
            }
            resolved.push(TriggerEventColumn::new(&column, definition.as_ref()));
        }
        Ok(Self::update(&resolved))
    }
}

impl AstConversion<'_, TriggerConstraint> {
    pub(crate) fn resolve(
        &self,
        context: &impl ParseContext,
    ) -> Result<SemTriggerConstraint, ResolutionFailure> {
        let relation = self
            .ast
            .relation
            .as_ref()
            .map(|relation| {
                AstConversion::<CreateTrigger>::relation(
                    context,
                    &convert(&relation.name).full_name()?,
                )
            })
            .transpose()?;
        Ok(SemTriggerConstraint::new(
            &relation,
            self.ast.deferrable,
            self.ast.initially_deferred,
        ))
    }
}
impl AstConversion<'_, TriggerBody> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemTriggerBody, ResolutionFailure> {
        match self.ast {
            TriggerBody::Function(function) => Ok(SemTriggerBody::function(
                &convert(function).resolve(context)?,
            )),
            TriggerBody::Commands(commands) => {
                SemTriggerBody::from_ast_commands(commands, context, scope)
            }
        }
    }
}
impl SemTriggerBody {
    fn from_ast_commands(
        commands: &[StatementNode],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, ResolutionFailure> {
        let mut resolved = Vec::new();
        for command in commands {
            resolved.push(convert(command).resolve_trigger_command(context, scope)?);
        }
        Ok(Self::commands(&resolved))
    }
}

impl AstConversion<'_, TriggerFunction> {
    pub(crate) fn resolve(
        &self,
        context: &impl ParseContext,
    ) -> Result<SemTriggerFunction, ResolutionFailure> {
        let requested = convert(&self.ast.name).full_name()?;
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
        if convert(return_type).full_name() != Some(&FullName::with_schema("pg_catalog", "trigger"))
        {
            return Err(ResolutionFailure::trigger(&TriggerProblem::function(&name)));
        }
        if !context.function_preserves_view_rows(&name, candidate) {
            return Err(ResolutionFailure::trigger(&TriggerProblem::function(&name)));
        }
        Ok(SemTriggerFunction::new(
            &name,
            &self.ast.arguments,
            return_type,
        ))
    }
}
impl AstConversion<'_, StatementNode> {
    pub(crate) fn resolve_trigger_command(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemAst, ResolutionFailure> {
        match self.ast {
            StatementNode::Query(query) => convert(query)
                .resolve_in(context, Some(scope))
                .map(|statement| SemAst::select(&statement))
                .map_err(Into::into),
            StatementNode::Insert(insert) => {
                convert(insert).resolve_trigger_command(context, scope)
            }
            StatementNode::Update(update) => {
                convert(update).resolve_trigger_command(context, scope)
            }
            StatementNode::Delete(delete) => {
                convert(delete).resolve_trigger_command(context, scope)
            }
            _ => AstConversion::<CreateTrigger>::invalid(
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
impl<C: ParseContext> SemanticVisitorMut for WhenValidator<'_, C> {
    fn enter_query(&mut self) {
        self.invalid = true;
    }
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        if !expression.preserves_view_rows(self.context) {
            self.invalid = true;
        }
        Ok(())
    }
}

impl AstConversion<'_, Relation> {
    fn validate_trigger_command(&self) -> Result<(), ResolutionFailure> {
        if self.ast.name.0.len() != 1 || self.ast.indexed.is_some() || self.ast.not_indexed {
            return AstConversion::<CreateTrigger>::invalid(
                &convert(&self.ast.name).full_name()?,
                "SQLite trigger DML requires an unqualified target without an index hint",
            );
        }
        Ok(())
    }
}
impl AstConversion<'_, Insert> {
    pub(crate) fn resolve_trigger_command(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemAst, ResolutionFailure> {
        convert(&self.ast.relation).validate_trigger_command()?;
        if !self.ast.returning.is_empty() || self.ast.source.is_none() {
            return AstConversion::<CreateTrigger>::invalid(
                &convert(&self.ast.relation.name).full_name()?,
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
impl AstConversion<'_, Update> {
    pub(crate) fn resolve_trigger_command(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemAst, ResolutionFailure> {
        convert(&self.ast.relation).validate_trigger_command()?;
        if !self.ast.returning.is_empty() {
            return AstConversion::<CreateTrigger>::invalid(
                &convert(&self.ast.relation.name).full_name()?,
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
impl AstConversion<'_, Delete> {
    pub(crate) fn resolve_trigger_command(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemAst, ResolutionFailure> {
        convert(&self.ast.relation).validate_trigger_command()?;
        if !self.ast.returning.is_empty() {
            return AstConversion::<CreateTrigger>::invalid(
                &convert(&self.ast.relation.name).full_name()?,
                "SQLite trigger DELETE does not support RETURNING",
            );
        }
        self.resolve_in(context, Some(scope))
            .map(|statement| SemAst::delete(&statement))
            .map_err(Into::into)
    }
}

impl AstConversion<'_, Expr> {
    pub(crate) const fn untyped_trigger_predicate(&self) -> bool {
        match self.ast {
            Expr::Literal(literal) => convert(literal).untyped_trigger_predicate(),
            _ => false,
        }
    }
}
impl AstConversion<'_, Literal> {
    pub(crate) const fn untyped_trigger_predicate(&self) -> bool {
        matches!(self.ast, Literal::Null | Literal::String(_))
    }
}
