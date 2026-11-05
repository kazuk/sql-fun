//! CREATE TRIGGER resolution consumes only the common syntax tree.

use crate::sem::DeleteStatement;
use crate::sem::InsertStatement;
use crate::sem::SelectStatement;
use crate::sem::UpdateStatement;

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

impl SemCreateTrigger {
    pub(crate) fn from_ast(
        ast: &CreateTrigger,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_in(ast, context).and_then(|definition| {
            if context.parameter_inference_needed() {
                return Self::ast_invalid(
                    &FullName::from_ast_name(&ast.relation.name)?,
                    "trigger definitions cannot contain bind parameters",
                );
            }
            if context.analysis_state().has_problems() {
                return Self::ast_invalid(
                    &FullName::from_ast_name(&ast.relation.name)?,
                    "trigger contains unresolved or invalid expressions",
                );
            }
            Ok(definition)
        });
        let result = ResolutionFailure::finish(result, context);
        context.finish_analysis(result)
    }
}

impl SemCreateTrigger {
    pub(crate) fn apply_ast<C: ParseContext>(
        ast: &CreateTrigger,
        mut context: C,
    ) -> Result<AstAndContextPair<C>, AnalysisError> {
        let Some(definition) = Self::from_ast(ast, &mut context)? else {
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
}

impl SemCreateTrigger {
    pub(crate) fn from_ast_in(
        ast: &CreateTrigger,
        context: &mut impl ParseContext,
    ) -> Result<Self, ResolutionFailure> {
        let target = Self::from_ast_target(ast, context)?;
        let sqlite = context.dialect() == Dialect::sqlite();
        let timing = ast.timing.unwrap_or(TriggerTiming::Before);
        let granularity = ast.granularity.unwrap_or(if sqlite {
            TriggerGranularity::Row
        } else {
            TriggerGranularity::Statement
        });
        Self::ast_validate(ast, context, &target, timing, granularity)?;
        let identity = Self::from_ast_identity(ast, context, &target)?;
        Self::validate_ast_identity(ast, context, &identity)?;
        let events = Self::from_ast_events(ast, context, &target)?;
        let with = WithClause::default();
        let from = Self::from_ast_row_bindings(ast, context, &target, granularity, &with)?;
        let scope = Self::ast_analysis_scope(ast, context, &target, &with, &from);
        let predicate = Self::from_ast_predicate(ast, context, &scope, &target)?;
        let constraint = ast
            .constraint
            .as_ref()
            .map(|constraint| SemTriggerConstraint::from_ast(constraint, context))
            .transpose()?;
        let transitions = Self::from_ast_transitions(ast, context, &target);
        let body = SemTriggerBody::from_ast(&ast.body, context, &scope)?;
        let definition = Self::new(
            &identity,
            &target,
            SemTriggerTiming::ast_semantic(&timing),
            SemTriggerGranularity::ast_semantic(&granularity),
            &body,
        );
        Self::ast_finish_definition(
            ast,
            context,
            definition,
            &events,
            &predicate,
            &constraint,
            &transitions,
        )
    }
}

impl SemCreateTrigger {
    pub(crate) fn from_ast_events(
        ast: &CreateTrigger,
        context: &impl ParseContext,
        target: &FullName,
    ) -> Result<Vec<SemTriggerEvent>, ResolutionFailure> {
        let mut events = Vec::new();
        for event in &ast.events {
            events.push(SemTriggerEvent::from_ast(event, context, target)?);
        }
        Ok(events)
    }
}

impl SemCreateTrigger {
    pub(crate) fn from_ast_row_bindings(
        ast: &CreateTrigger,
        context: &mut impl ParseContext,
        target: &FullName,
        granularity: TriggerGranularity,
        with: &WithClause,
    ) -> Result<FromClause, ResolutionFailure> {
        let mut from = FromClause::default();
        if granularity == TriggerGranularity::Row {
            if Self::ast_has_old(ast) {
                from.push(DataSource::resolve_relation(
                    context,
                    target,
                    &AliasSpec::from_names("old", &[]),
                    with,
                )?);
            }
            if Self::ast_has_new(ast) {
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
}

impl SemCreateTrigger {
    pub(crate) fn ast_analysis_scope<'a>(
        ast: &CreateTrigger,
        context: &impl ParseContext,
        target: &FullName,
        with: &'a WithClause,
        from: &'a FromClause,
    ) -> ScalarAnalysisContext<'a> {
        let mut scope = ScalarAnalysisContext::trigger(with, from);
        if context.dialect() == Dialect::sqlite() && !ast.temporary {
            scope.set_trigger_namespace(target.schema());
        }
        scope
    }
}

impl SemCreateTrigger {
    pub(crate) fn from_ast_predicate(
        ast: &CreateTrigger,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        target: &FullName,
    ) -> Result<Option<SemScalarExpr>, ResolutionFailure> {
        let mut predicate = ast
            .predicate
            .as_ref()
            .map(|expression| SemScalarExpr::from_ast_in(expression, context, scope))
            .transpose()?;
        if context.dialect() != Dialect::sqlite()
            && let Some(predicate) = &mut predicate
        {
            if ast
                .predicate
                .as_ref()
                .is_some_and(SemScalarExpr::ast_is_untyped_trigger_predicate)
            {
                *predicate = SemScalarExpr::wrap_implicit_cast(&PgBuiltInType::bool(), predicate);
            } else {
                SemScalarExpr::implicit_cast_resolved(context, &PgBuiltInType::bool(), predicate)?;
            }
            if predicate
                .get_type()
                .is_none_or(|ty| ty != PgBuiltInType::bool())
            {
                return Self::ast_invalid(target, "WHEN must have boolean type");
            }
            let mut inspected = predicate.clone();
            let mut validator = WhenValidator {
                context,
                invalid: false,
            };
            inspected.walk(&mut validator)?;
            if validator.invalid {
                return Self::ast_invalid(
                    target,
                    "PostgreSQL WHEN cannot contain subqueries, aggregates, windows or set-returning functions",
                );
            }
        }
        Ok(predicate)
    }
}

impl SemCreateTrigger {
    pub(crate) fn from_ast_transitions(
        ast: &CreateTrigger,
        _context: &impl ParseContext,
        target: &FullName,
    ) -> Vec<TriggerTransition> {
        let mut transitions = Vec::new();
        for transition in &ast.transitions {
            transitions.push(TriggerTransition::new(
                &transition.name,
                transition.new,
                target,
            ));
        }
        transitions
    }
}

impl SemCreateTrigger {
    pub(crate) fn ast_finish_definition(
        ast: &CreateTrigger,
        context: &impl ParseContext,
        mut definition: Self,
        events: &[SemTriggerEvent],
        predicate: &Option<SemScalarExpr>,
        constraint: &Option<SemTriggerConstraint>,
        transitions: &[TriggerTransition],
    ) -> Result<Self, ResolutionFailure> {
        definition.set_event_semantics(events, predicate, constraint, transitions);
        definition.set_creation_mode(ast.replace, ast.if_not_exists, ast.temporary);
        definition.retain_dependencies(context)?;
        Ok(definition)
    }
}

impl SemCreateTrigger {
    pub(crate) fn from_ast_target(
        ast: &CreateTrigger,
        context: &impl ParseContext,
    ) -> Result<FullName, ResolutionFailure> {
        let mut name = FullName::from_ast_name(&ast.relation.name)?;
        // Non-TEMP SQLite triggers belong to the selected trigger schema and
        // cannot attach to a relation in another database.
        if context.dialect() == Dialect::sqlite() && !ast.temporary {
            let trigger = FullName::from_ast_name(&ast.name)?;
            let schema = trigger
                .schema()
                .cloned()
                .unwrap_or_else(|| SchemaName::from("main"));
            if name.schema().is_none() {
                name = name.set_schema(schema);
            }
        }
        Self::ast_relation(context, &name)
    }
}

impl SemCreateTrigger {
    pub(crate) fn ast_relation(
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
}

impl SemCreateTrigger {
    pub(crate) fn from_ast_identity(
        ast: &CreateTrigger,
        context: &impl ParseContext,
        target: &FullName,
    ) -> Result<TriggerIdentity, ResolutionFailure> {
        let supplied = FullName::from_ast_name(&ast.name)?;
        if context.dialect() == Dialect::postgres() {
            if supplied.schema().is_some() || supplied.catalog().is_some() {
                return Self::ast_invalid(
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
            let schema = if ast.temporary {
                SchemaName::from("temp")
            } else {
                supplied
                    .schema()
                    .cloned()
                    .unwrap_or_else(|| SchemaName::from("main"))
            };
            if ast.temporary && supplied.schema().is_some() {
                return Self::ast_invalid(target, "TEMP trigger name cannot be schema qualified");
            }
            if !ast.temporary
                && target.schema().is_some_and(|target_schema| {
                    !target_schema.as_str().eq_ignore_ascii_case(schema.as_str())
                })
            {
                return Self::ast_invalid(
                    target,
                    "trigger and target must use the same SQLite schema",
                );
            }
            Ok(TriggerIdentity::sqlite(&supplied.set_schema(schema)))
        }
    }
}

impl SemCreateTrigger {
    pub(crate) fn validate_ast_identity(
        ast: &CreateTrigger,
        context: &impl ParseContext,
        identity: &TriggerIdentity,
    ) -> Result<(), ResolutionFailure> {
        if let Some(existing) = context.trigger(identity)
            && !ast.if_not_exists
            && (!ast.replace || existing.constraint().is_some() || ast.constraint.is_some())
        {
            return Err(ResolutionFailure::trigger(&TriggerProblem::duplicate(
                identity,
            )));
        }
        Ok(())
    }
}

impl SemCreateTrigger {
    pub(crate) fn ast_has_old(ast: &CreateTrigger) -> bool {
        ast.events.iter().all(SemTriggerEvent::ast_has_old)
    }
}

impl SemCreateTrigger {
    pub(crate) fn ast_has_new(ast: &CreateTrigger) -> bool {
        ast.events.iter().all(SemTriggerEvent::ast_has_new)
    }
}

impl SemCreateTrigger {
    pub(crate) fn ast_validate(
        ast: &CreateTrigger,
        context: &impl ParseContext,
        target: &FullName,
        timing: TriggerTiming,
        granularity: TriggerGranularity,
    ) -> Result<(), ResolutionFailure> {
        let view = context.get_view_impl(target);
        if view.is_some_and(ViewLike::is_materialized) {
            return Self::ast_invalid(target, "materialized views cannot own triggers");
        }
        if ast.events.is_empty() {
            return Self::ast_invalid(target, "at least one event is required");
        }
        if context.dialect() == Dialect::sqlite() {
            if ast.replace
                || ast.constraint.is_some()
                || !ast.transitions.is_empty()
                || granularity != TriggerGranularity::Row
                || ast.events.len() != 1
                || ast.events.iter().any(SemTriggerEvent::ast_is_truncate)
            {
                return Self::ast_invalid(target, "unsupported SQLite trigger option combination");
            }
            if (timing == TriggerTiming::InsteadOf) != view.is_some() {
                return Self::ast_invalid(
                    target,
                    "SQLite views require INSTEAD OF triggers; tables require BEFORE or AFTER",
                );
            }
            return Ok(());
        }
        if ast.temporary || ast.if_not_exists {
            return Self::ast_invalid(
                target,
                "PostgreSQL has no TEMP or IF NOT EXISTS trigger option",
            );
        }
        if timing == TriggerTiming::InsteadOf
            && (view.is_none()
                || granularity != TriggerGranularity::Row
                || ast.predicate.is_some()
                || ast.events.iter().any(SemTriggerEvent::ast_has_columns))
        {
            return Self::ast_invalid(
                target,
                "INSTEAD OF requires a row trigger on a view without WHEN or UPDATE OF",
            );
        }
        if view.is_some()
            && timing != TriggerTiming::InsteadOf
            && granularity != TriggerGranularity::Statement
        {
            return Self::ast_invalid(target, "BEFORE/AFTER view triggers must be statement-level");
        }
        if ast.events.iter().any(SemTriggerEvent::ast_is_truncate)
            && (granularity != TriggerGranularity::Statement || view.is_some())
        {
            return Self::ast_invalid(target, "TRUNCATE requires a statement trigger on a table");
        }
        if let Some(constraint) = &ast.constraint
            && (timing != TriggerTiming::After
                || granularity != TriggerGranularity::Row
                || view.is_some()
                || (constraint.initially_deferred && !constraint.deferrable)
                || ast.replace)
        {
            return Self::ast_invalid(
                target,
                "invalid constraint trigger timing, target, deferrability or replacement",
            );
        }
        if !ast.transitions.is_empty() {
            if timing != TriggerTiming::After
                || view.is_some()
                || ast.constraint.is_some()
                || ast.events.len() != 1
                || ast.events.iter().any(SemTriggerEvent::ast_has_columns)
                || ast.events.iter().any(SemTriggerEvent::ast_is_truncate)
            {
                return Self::ast_invalid(target, "invalid transition-table combination");
            }
            let mut names = std::collections::HashSet::new();
            let mut kinds = std::collections::HashSet::new();
            for transition in &ast.transitions {
                if !transition.table
                    || !names.insert(&transition.name)
                    || !kinds.insert(transition.new)
                    || (transition.new && !Self::ast_has_new(ast))
                    || (!transition.new && !Self::ast_has_old(ast))
                {
                    return Self::ast_invalid(
                        target,
                        "invalid or duplicate transition-table binding",
                    );
                }
            }
        }
        Ok(())
    }
}

impl SemCreateTrigger {
    pub(crate) fn ast_invalid<T>(target: &FullName, rule: &str) -> Result<T, ResolutionFailure> {
        Err(ResolutionFailure::trigger(&TriggerProblem::options(
            target, rule,
        )))
    }
}

#[cfg(test)]
mod tests;

impl SemTriggerTiming {
    pub(crate) const fn ast_semantic(ast: &TriggerTiming) -> Self {
        match ast {
            TriggerTiming::Before => Self::before(),
            TriggerTiming::After => Self::after(),
            TriggerTiming::InsteadOf => Self::instead_of(),
        }
    }
}

impl SemTriggerGranularity {
    pub(crate) const fn ast_semantic(ast: &TriggerGranularity) -> Self {
        match ast {
            TriggerGranularity::Row => Self::row(),
            TriggerGranularity::Statement => Self::statement(),
        }
    }
}

impl SemTriggerEvent {
    pub(crate) const fn ast_has_old(ast: &TriggerEvent) -> bool {
        matches!(ast, TriggerEvent::Update(_) | TriggerEvent::Delete)
    }
}

impl SemTriggerEvent {
    pub(crate) const fn ast_has_new(ast: &TriggerEvent) -> bool {
        matches!(ast, TriggerEvent::Update(_) | TriggerEvent::Insert)
    }
}

impl SemTriggerEvent {
    pub(crate) const fn ast_is_truncate(ast: &TriggerEvent) -> bool {
        matches!(ast, TriggerEvent::Truncate)
    }
}

impl SemTriggerEvent {
    pub(crate) const fn ast_has_columns(ast: &TriggerEvent) -> bool {
        matches!(ast, TriggerEvent::Update(columns) if !columns.is_empty())
    }
}

impl SemTriggerEvent {
    pub(crate) fn from_ast(
        ast: &TriggerEvent,
        context: &impl ParseContext,
        target: &FullName,
    ) -> Result<Self, ResolutionFailure> {
        match ast {
            TriggerEvent::Insert => Ok(Self::insert()),
            TriggerEvent::Delete => Ok(Self::delete()),
            TriggerEvent::Truncate => Ok(Self::truncate()),
            TriggerEvent::Update(columns) => Self::from_ast_update(columns, context, target),
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

impl SemTriggerConstraint {
    pub(crate) fn from_ast(
        ast: &TriggerConstraint,
        context: &impl ParseContext,
    ) -> Result<Self, ResolutionFailure> {
        let relation = ast
            .relation
            .as_ref()
            .map(|relation| {
                SemCreateTrigger::ast_relation(context, &FullName::from_ast_name(&relation.name)?)
            })
            .transpose()?;
        Ok(Self::new(&relation, ast.deferrable, ast.initially_deferred))
    }
}

impl SemTriggerBody {
    pub(crate) fn from_ast(
        ast: &TriggerBody,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, ResolutionFailure> {
        match ast {
            TriggerBody::Function(function) => Ok(Self::function(&SemTriggerFunction::from_ast(
                function, context,
            )?)),
            TriggerBody::Commands(commands) => Self::from_ast_commands(commands, context, scope),
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
            resolved.push(SemAst::from_ast_trigger_command(command, context, scope)?);
        }
        Ok(Self::commands(&resolved))
    }
}

impl SemTriggerFunction {
    pub(crate) fn from_ast(
        ast: &TriggerFunction,
        context: &impl ParseContext,
    ) -> Result<Self, ResolutionFailure> {
        let requested = FullName::from_ast_name(&ast.name)?;
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
        if return_type.full_name() != Some(&FullName::with_schema("pg_catalog", "trigger")) {
            return Err(ResolutionFailure::trigger(&TriggerProblem::function(&name)));
        }
        if !context.function_preserves_view_rows(&name, candidate) {
            return Err(ResolutionFailure::trigger(&TriggerProblem::function(&name)));
        }
        Ok(Self::new(&name, &ast.arguments, return_type))
    }
}

impl SemAst {
    pub(crate) fn from_ast_trigger_command(
        ast: &StatementNode,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, ResolutionFailure> {
        match ast {
            StatementNode::Query(query) => {
                SelectStatement::from_ast_in(query, context, Some(scope))
                    .map(|statement| Self::select(&statement))
                    .map_err(Into::into)
            }
            StatementNode::Insert(insert) => {
                InsertStatement::from_ast_trigger_command(insert, context, scope)
            }
            StatementNode::Update(update) => {
                UpdateStatement::from_ast_trigger_command(update, context, scope)
            }
            StatementNode::Delete(delete) => {
                DeleteStatement::from_ast_trigger_command(delete, context, scope)
            }
            _ => SemCreateTrigger::ast_invalid(
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

impl DataSource {
    fn validate_ast_trigger_command(ast: &Relation) -> Result<(), ResolutionFailure> {
        if ast.name.0.len() != 1 || ast.indexed.is_some() || ast.not_indexed {
            return SemCreateTrigger::ast_invalid(
                &FullName::from_ast_name(&ast.name)?,
                "SQLite trigger DML requires an unqualified target without an index hint",
            );
        }
        Ok(())
    }
}

impl InsertStatement {
    pub(crate) fn from_ast_trigger_command(
        ast: &Insert,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemAst, ResolutionFailure> {
        DataSource::validate_ast_trigger_command(&ast.relation)?;
        if !ast.returning.is_empty() || ast.source.is_none() {
            return SemCreateTrigger::ast_invalid(
                &FullName::from_ast_name(&ast.relation.name)?,
                "SQLite trigger INSERT does not support RETURNING or DEFAULT VALUES",
            );
        }
        Self::from_ast_in(ast, context, Some(scope))
            .map(|statement| {
                statement
                    .as_ref()
                    .map_or_else(SemAst::unresolved_statement, SemAst::insert)
            })
            .map_err(Into::into)
    }
}

impl UpdateStatement {
    pub(crate) fn from_ast_trigger_command(
        ast: &Update,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemAst, ResolutionFailure> {
        DataSource::validate_ast_trigger_command(&ast.relation)?;
        if !ast.returning.is_empty() {
            return SemCreateTrigger::ast_invalid(
                &FullName::from_ast_name(&ast.relation.name)?,
                "SQLite trigger UPDATE does not support RETURNING",
            );
        }
        Self::from_ast_in(ast, context, Some(scope))
            .map(|statement| {
                statement
                    .as_ref()
                    .map_or_else(SemAst::unresolved_statement, SemAst::update)
            })
            .map_err(Into::into)
    }
}

impl DeleteStatement {
    pub(crate) fn from_ast_trigger_command(
        ast: &Delete,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemAst, ResolutionFailure> {
        DataSource::validate_ast_trigger_command(&ast.relation)?;
        if !ast.returning.is_empty() {
            return SemCreateTrigger::ast_invalid(
                &FullName::from_ast_name(&ast.relation.name)?,
                "SQLite trigger DELETE does not support RETURNING",
            );
        }
        Self::from_ast_in(ast, context, Some(scope))
            .map(|statement| SemAst::delete(&statement))
            .map_err(Into::into)
    }
}

impl SemScalarExpr {
    pub(crate) const fn ast_is_untyped_trigger_predicate(ast: &Expr) -> bool {
        match ast {
            Expr::Literal(literal) => Self::ast_literal_is_untyped_trigger_predicate(literal),
            _ => false,
        }
    }
}

impl SemScalarExpr {
    pub(crate) const fn ast_literal_is_untyped_trigger_predicate(ast: &Literal) -> bool {
        matches!(ast, Literal::Null | Literal::String(_))
    }
}
