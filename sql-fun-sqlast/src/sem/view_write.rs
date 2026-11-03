//! Analyzer-owned writable-view decisions from effective semantic definitions.
use crate::sem::ResolutionFailure;
use crate::sem::problem::ViewProblem;
use crate::{
    names::{ColumnName, FullName, TableName},
    sem::{
        AnalysisError, ColumnDefinition, ParseContext, SemScalarExpr, ViewInsertSupport,
        ViewUpdateSupport,
    },
};

/// Writable-view analysis with the caller's full diagnostic environment.
/// Catalog definitions remain unchanged; semantic problems are reported here.
pub struct ViewWriteEvaluation<'a, C: ParseContext> {
    context: &'a mut C,
}
impl<'a, C: ParseContext> ViewWriteEvaluation<'a, C> {
    /// Use the caller's mutable analysis context for lookup and diagnostics.
    pub const fn new(context: &'a mut C) -> Self {
        Self { context }
    }
    /// Evaluate INSERT support; None means a semantic problem was reported.
    /// # Errors
    /// Returns only an unrecoverable analysis or diagnostic delivery failure.
    pub fn insert(&mut self, view: &FullName) -> Result<Option<ViewInsertSupport>, AnalysisError> {
        let result = insert_support(self.context, view);
        ResolutionFailure::finish(result, self.context)
    }
    /// Evaluate UPDATE support; None means a semantic problem was reported.
    /// # Errors
    /// Returns only an unrecoverable analysis or diagnostic delivery failure.
    pub fn update(&mut self, view: &FullName) -> Result<Option<ViewUpdateSupport>, AnalysisError> {
        let result = update_support(self.context, view);
        ResolutionFailure::finish(result, self.context)
    }
    /// Evaluate both operations; None means a semantic problem was reported.
    /// # Errors
    /// Returns only an unrecoverable analysis or diagnostic delivery failure.
    pub fn evaluate(&mut self, view: &FullName) -> Result<Option<ViewWriteSupport>, AnalysisError> {
        let Some(insert) = self.insert(view)? else {
            return Ok(None);
        };
        let Some(update) = self.update(view)? else {
            return Ok(None);
        };
        Ok(Some(ViewWriteSupport { insert, update }))
    }
    pub(crate) fn infer_arbiters(
        &mut self,
        view: &TableName,
        conflict: &crate::sem::InsertConflict,
    ) -> Result<Option<crate::sem::ConflictArbiterInference>, AnalysisError> {
        let result = infer_view_arbiters(self.context, view, conflict);
        ResolutionFailure::finish(result, self.context)
    }
}
#[cfg(test)]
mod tests;

/// Effective operation-specific writable-view analysis results.
pub struct ViewWriteSupport {
    insert: ViewInsertSupport,
    update: ViewUpdateSupport,
}
impl ViewWriteSupport {
    /// Effective INSERT result.
    pub const fn insert(&self) -> &ViewInsertSupport {
        &self.insert
    }
    /// Effective UPDATE result.
    pub const fn update(&self) -> &ViewUpdateSupport {
        &self.update
    }
}

// Transient projection used only by this evaluation and conflict rewriting. It is never registered.
struct EvaluationView<'a> {
    name: FullName,
    definition: &'a crate::sem::CreateView,
    source: Option<FullName>,
    projections: Vec<(ColumnDefinition, Option<ColumnName>)>,
    expressions: Vec<(ColumnName, SemScalarExpr)>,
}
impl<'a> EvaluationView<'a> {
    fn matching_rules<'c>(
        &self,
        context: &'c impl ParseContext,
        event: WriteEvent,
    ) -> Vec<&'c crate::sem::CreateRule> {
        self.definition
            .rules()
            .resolve(context)
            .into_iter()
            .filter(|rule| rule.event().write_event() == Some(event))
            .collect()
    }
    fn has_conflict_rules(&self, context: &impl ParseContext) -> bool {
        self.definition
            .rules()
            .resolve(context)
            .into_iter()
            .any(crate::sem::CreateRule::blocks_insert_on_conflict)
    }
    fn matching_triggers<'c>(
        &self,
        context: &'c impl ParseContext,
        event: WriteEvent,
    ) -> Vec<&'c crate::sem::CreateTrigger> {
        self.definition
            .triggers()
            .resolve(context, &self.name)
            .into_iter()
            .filter(|trigger| {
                trigger.timing() == crate::sem::TriggerTiming::instead_of()
                    && trigger.granularity() == crate::sem::TriggerGranularity::row()
                    && trigger
                        .events()
                        .iter()
                        .any(|candidate| event.matches_trigger(candidate))
            })
            .collect()
    }
    fn load(context: &'a impl ParseContext, name: &FullName) -> Result<Self, ResolutionFailure> {
        let view = lookup_bound_view(context, name)?.ok_or_else(|| missing_definition(name))?;
        let definition = view
            .ordinary_definition()
            .ok_or_else(|| missing_definition(name))?;
        for identity in definition.rules().identities() {
            if context.rule(identity).is_none() && !rule_is_cancelled(context, identity) {
                return Err(missing_definition(identity.name()));
            }
        }
        for trigger in definition.triggers().names() {
            let identity =
                if crate::ast::Dialect::from(context.metadata()) == crate::ast::Dialect::sqlite() {
                    crate::sem::TriggerIdentity::sqlite(trigger)
                } else {
                    crate::sem::TriggerIdentity::postgres(name, trigger)
                };
            if context.trigger(&identity).is_none() && !context.trigger_is_cancelled(&identity) {
                return Err(missing_definition(trigger));
            }
        }
        let source = automatic_view_source(context, definition.query())?;
        let projections = definition
            .column_lineage()
            .project(definition.result_columns());
        Ok(Self {
            name: name.clone(),
            definition,
            source,
            projections,
            expressions: definition.result_columns().view_expressions(),
        })
    }
    fn decision(
        &self,
        context: &impl ParseContext,
        event: WriteEvent,
        seen: &mut Vec<FullName>,
    ) -> Result<Decision, ResolutionFailure> {
        self.decision_from_facts(context, event, seen)
    }
    fn source_expression(&self, column: &ColumnName) -> Option<&SemScalarExpr> {
        self.expressions.iter().find_map(|(name, expression)| {
            if name == column {
                Some(expression)
            } else {
                None
            }
        })
    }
    fn source_column(&self, name: &ColumnName) -> Option<&ColumnName> {
        self.projections.iter().find_map(|(definition, source)| {
            if definition.name().as_ref() == Some(name) {
                source.as_ref()
            } else {
                None
            }
        })
    }

    fn decision_from_facts(
        &self,
        context: &impl ParseContext,
        event: WriteEvent,
        seen: &mut Vec<FullName>,
    ) -> Result<Decision, ResolutionFailure> {
        if let Some(decision) = self.explicit_write_decision(context, event) {
            return Ok(decision);
        }
        let base = self.automatic_source_decision(context, event, seen)?;
        if !base.writable {
            return Ok(base);
        }
        Ok(self.project_writable_columns(&base))
    }

    fn explicit_write_decision(
        &self,
        context: &impl ParseContext,
        event: WriteEvent,
    ) -> Option<Decision> {
        let rules = self.matching_rules(context, event);
        let unconditional = rules.iter().any(|rule| rule.replaces_unconditionally());
        let triggers = self.matching_triggers(context, event);
        if unconditional {
            let returning = rules
                .iter()
                .any(|rule| rule.replaces_unconditionally() && rule.has_returning());
            return Some(Decision::writable(
                &self.visible_columns(),
                returning,
                !self.has_conflict_rules(context),
            ));
        }
        if !triggers.is_empty() {
            let mut decision = Decision::writable(
                &self.visible_columns(),
                !rules.iter().any(|rule| rule.instead()),
                !self.has_conflict_rules(context),
            );
            if crate::ast::Dialect::from(context.metadata()) == crate::ast::Dialect::sqlite() {
                decision.required_trigger_columns =
                    required_trigger_columns(&triggers, event).map(|columns| {
                        columns
                            .into_iter()
                            .map(|name| {
                                self.projections
                                    .iter()
                                    .filter_map(|(column, _)| column.name().as_ref())
                                    .find(|visible| {
                                        visible.as_str().eq_ignore_ascii_case(name.as_str())
                                    })
                                    .cloned()
                                    .unwrap_or(name)
                            })
                            .collect()
                    });
            }
            return Some(decision);
        }
        if crate::ast::Dialect::from(context.metadata()) == crate::ast::Dialect::sqlite()
            || rules.iter().any(|rule| rule.instead())
        {
            return Some(Decision::read_only());
        }
        None
    }
    fn automatic_source_decision(
        &self,
        context: &impl ParseContext,
        event: WriteEvent,
        seen: &mut Vec<FullName>,
    ) -> Result<Decision, ResolutionFailure> {
        let Some(source) = &self.source else {
            return Ok(Decision::read_only());
        };
        if seen.contains(source) {
            return Ok(Decision::read_only());
        }
        seen.push(source.clone());
        let base = self.source_decision(context, source, event, seen);
        seen.pop();
        base
    }

    fn source_decision(
        &self,
        context: &impl ParseContext,
        source: &FullName,
        event: WriteEvent,
        seen: &mut Vec<FullName>,
    ) -> Result<Decision, ResolutionFailure> {
        let decision = if let Some(table) =
            context.get_table(&crate::names::TableName::from(source.clone()))
        {
            let mut columns = Vec::new();
            while let Some(column) = table.get_column_at(columns.len()) {
                columns.push(column.clone());
            }
            Decision::writable(&columns, true, true)
        } else if let Some(view) = context.get_view(&crate::names::ViewName::from_full_name(source))
        {
            if view.is_materialized() {
                Decision::read_only()
            } else {
                EvaluationView::load(context, source)?.decision(context, event, seen)?
            }
        } else {
            return Err(missing_definition(source));
        };
        Ok(decision)
    }

    fn project_writable_columns(&self, base: &Decision) -> Decision {
        let mut columns = Vec::new();
        let mut assignment_columns = Vec::new();
        for (visible, source_column) in &self.projections {
            let Some(name) = source_column else {
                continue;
            };
            let Some(definition) = base.column(name) else {
                continue;
            };
            columns.push(self.effective_projection(visible, definition));
            if let Some(visible_name) = visible.name() {
                assignment_columns.push(ViewAssignmentColumn::new(
                    visible_name,
                    base.assignment_column(name),
                ));
            }
        }
        // PostgreSQL requires at least one updatable projection, even for DEFAULT VALUES.
        if columns.is_empty() {
            return Decision::read_only();
        }
        let mut decision = Decision::writable(&columns, base.returning, base.on_conflict);
        decision.assignment_columns = assignment_columns;
        decision.rewrites_defaults = true;
        decision
    }

    fn effective_projection(
        &self,
        visible: &ColumnDefinition,
        source: &ColumnDefinition,
    ) -> ColumnDefinition {
        let mut column = ColumnDefinition::new(
            visible.name(),
            visible.get_type(),
            source.is_not_null(),
            source.constraints(),
        );
        self.apply_default(&mut column);
        column
    }

    fn visible_columns(&self) -> Vec<ColumnDefinition> {
        self.projections
            .iter()
            .map(|(column, _)| {
                let mut column = column.clone();
                self.apply_default(&mut column);
                column
            })
            .collect()
    }

    fn apply_default(&self, column: &mut ColumnDefinition) {
        self.definition.apply_column_default(column);
    }
}
#[derive(Debug)]
struct Decision {
    assignment_columns: Vec<ViewAssignmentColumn>,
    rewrites_defaults: bool,
    writable: bool,
    columns: Vec<ColumnDefinition>,
    returning: bool,
    on_conflict: bool,
    required_trigger_columns: Option<Vec<ColumnName>>,
}
impl Decision {
    const fn read_only() -> Self {
        Self {
            assignment_columns: Vec::new(),
            rewrites_defaults: false,
            writable: false,
            columns: Vec::new(),
            returning: false,
            on_conflict: false,
            required_trigger_columns: None,
        }
    }
    fn writable(columns: &[ColumnDefinition], returning: bool, on_conflict: bool) -> Self {
        Self {
            assignment_columns: Vec::new(),
            rewrites_defaults: false,
            writable: true,
            columns: columns.to_vec(),
            returning,
            on_conflict,
            required_trigger_columns: None,
        }
    }
    fn apply_update_defaults(&self, support: &mut ViewUpdateSupport) {
        if !self.rewrites_defaults {
            return;
        }
        for column in &self.columns {
            let restricted = column.constraints().iter().any(|constraint| {
                constraint.as_generated().is_some()
                    || constraint
                        .as_identity()
                        .is_some_and(|identity| identity.is_always())
            });
            if restricted && let Some(name) = column.name() {
                support.forbid_default(name);
            }
        }
    }
    fn assignment_column<'a>(&'a self, name: &'a ColumnName) -> &'a ColumnName {
        self.assignment_columns
            .iter()
            .find(|column| column.name() == name)
            .map_or(name, ViewAssignmentColumn::source_column)
    }
    fn column(&self, name: &ColumnName) -> Option<&ColumnDefinition> {
        self.columns
            .iter()
            .find(|column| column.name().as_ref() == Some(name))
    }
}

fn insert_support(
    context: &impl ParseContext,
    view: &FullName,
) -> Result<ViewInsertSupport, ResolutionFailure> {
    if lookup_bound_view(context, view)?.is_some_and(crate::sem::ViewLike::is_materialized) {
        return Ok(ViewInsertSupport::read_only());
    }
    if let Some(support) = context.view_insert_support(view) {
        return Ok(support.clone());
    }
    let decision = EvaluationView::load(context, view)?.decision(
        context,
        WriteEvent::insert(),
        &mut vec![view.clone()],
    )?;
    if decision.writable {
        let mut support = ViewInsertSupport::writable(
            &decision.columns,
            decision.returning,
            decision.on_conflict
                && crate::ast::Dialect::from(context.metadata()) != crate::ast::Dialect::sqlite(),
        );
        support.set_assignment_columns(&decision.assignment_columns);
        Ok(support)
    } else {
        Ok(ViewInsertSupport::read_only())
    }
}
fn update_support(
    context: &impl ParseContext,
    view: &FullName,
) -> Result<ViewUpdateSupport, ResolutionFailure> {
    if lookup_bound_view(context, view)?.is_some_and(crate::sem::ViewLike::is_materialized) {
        return Ok(ViewUpdateSupport::read_only());
    }
    if let Some(support) = context.view_update_support(view) {
        return Ok(support.clone());
    }
    let decision = EvaluationView::load(context, view)?.decision(
        context,
        WriteEvent::update(),
        &mut vec![view.clone()],
    )?;
    if decision.writable {
        let mut support = ViewUpdateSupport::writable(&decision.columns, decision.returning);
        support.set_assignment_columns(&decision.assignment_columns);
        decision.apply_update_defaults(&mut support);
        support.set_required_trigger_columns(decision.required_trigger_columns.as_deref());
        Ok(support)
    } else {
        Ok(ViewUpdateSupport::read_only())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WriteEvent {
    Insert,
    Update,
    Delete,
}
impl WriteEvent {
    const fn matches_trigger(self, event: &crate::sem::TriggerEvent) -> bool {
        matches!(
            (self, event),
            (Self::Insert, crate::sem::TriggerEvent::Insert)
                | (Self::Update, crate::sem::TriggerEvent::Update(_))
                | (Self::Delete, crate::sem::TriggerEvent::Delete)
        )
    }
    pub(crate) const fn insert() -> Self {
        Self::Insert
    }
    pub(crate) const fn update() -> Self {
        Self::Update
    }
    pub(crate) const fn delete() -> Self {
        Self::Delete
    }
}

fn required_trigger_columns(
    triggers: &[&crate::sem::CreateTrigger],
    event: WriteEvent,
) -> Option<Vec<ColumnName>> {
    let mut columns = Vec::new();
    for trigger in triggers {
        for candidate in trigger.events() {
            if event.matches_trigger(candidate) {
                if candidate.columns().is_empty() {
                    return None;
                }
                for column in candidate.columns() {
                    if !columns.contains(column.name()) {
                        columns.push(column.name().clone());
                    }
                }
            }
        }
    }
    Some(columns)
}

fn infer_view_arbiters(
    context: &impl ParseContext,
    view: &TableName,
    conflict: &crate::sem::InsertConflict,
) -> Result<crate::sem::ConflictArbiterInference, ResolutionFailure> {
    let info = EvaluationView::load(context, view.full_name())?;
    info.decision(
        context,
        WriteEvent::insert(),
        &mut vec![view.full_name().clone()],
    )?;
    if info
        .matching_rules(context, WriteEvent::insert())
        .iter()
        .any(|rule| rule.instead())
        || !info
            .matching_triggers(context, WriteEvent::insert())
            .is_empty()
    {
        return Ok(crate::sem::ConflictArbiterInference::complete(&[]));
    }
    let Some(source) = &info.source else {
        return Ok(crate::sem::ConflictArbiterInference::complete(&[]));
    };
    let mut mapped = conflict.clone();
    if !mapped.map_view_columns(context, &info, source)? {
        return Ok(crate::sem::ConflictArbiterInference::complete(&[]));
    }
    let relation = TableName::from(source.clone());
    let inference = if context.get_table(&relation).is_some() {
        context.infer_conflict_arbiters(&relation, &mapped)?
    } else {
        infer_view_arbiters(context, &relation, &mapped)?
    };
    if context
        .get_table(&crate::names::TableName::from(source.clone()))
        .is_some()
    {
        match inference.into_result() {
            Ok(arbiters) => Ok(crate::sem::ConflictArbiterInference::complete(
                &arbiters
                    .into_iter()
                    .map(|arbiter| arbiter.with_view_source(&relation))
                    .collect::<Vec<_>>(),
            )),
            Err(problem) => Ok(crate::sem::ConflictArbiterInference::problem(&problem)),
        }
    } else {
        Ok(inference)
    }
}

impl crate::sem::InsertConflict {
    fn map_view_columns(
        &mut self,
        context: &impl ParseContext,
        view: &EvaluationView<'_>,
        source: &FullName,
    ) -> Result<bool, AnalysisError> {
        let mut mapper = ViewColumnMapper {
            context,
            view,
            source,
            complete: true,
        };
        self.map_view_expressions(&mut mapper)?;
        Ok(mapper.complete)
    }
}
struct ViewColumnMapper<'a, C> {
    context: &'a C,
    view: &'a EvaluationView<'a>,
    source: &'a FullName,
    complete: bool,
}
impl<C: ParseContext> crate::sem::traversal::SemanticVisitorMut for ViewColumnMapper<'_, C> {
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        let Some(column) = expression.bound_view_column(&self.view.name) else {
            return Ok(());
        };
        let Some(mapped) = self.view.source_column(column) else {
            if let Some(expansion) = self.view.source_expression(column) {
                // Traversal is post-order: the replacement is already bound to
                // the source and must not be interpreted as another view field.
                *expression = expansion.clone();
            } else {
                self.complete = false;
            }
            return Ok(());
        };
        let definition = if let Some(table) = self
            .context
            .get_table(&TableName::from(self.source.clone()))
        {
            table.get_column_def(mapped).cloned()
        } else if let Some(view) = self
            .context
            .get_view(&crate::names::ViewName::from_full_name(self.source))
        {
            view.get_column_def(mapped)
        } else {
            None
        };
        let alias = crate::names::AliasName::from(self.source.name().as_str());
        *expression =
            SemScalarExpr::column_reference(crate::sem::ColumnReferenceExpr::table_column(
                &alias,
                mapped,
                &TableName::from(self.source.clone()),
                definition.as_ref(),
                false,
            ));
        Ok(())
    }
}

fn rule_is_cancelled(
    context: &dyn crate::context::RuleInfoRead,
    identity: &crate::sem::RuleIdentity,
) -> bool {
    if let Some(entry) = context.local_rule(identity) {
        entry.is_none()
    } else if let Some(parent) = context.rule_parent() {
        rule_is_cancelled(parent, identity)
    } else {
        false
    }
}

fn automatic_view_source(
    context: &impl ParseContext,
    query: &crate::sem::SelectStatement,
) -> Result<Option<FullName>, AnalysisError> {
    use crate::sem::SelectStatement;
    if !projection_preserves_rows(context, query)? {
        return Ok(None);
    }
    match query {
        SelectStatement::Query(query)
            if query.with().is_empty() && query.limit().is_none() && query.offset().is_none() =>
        {
            automatic_view_source(context, query.body())
        }
        SelectStatement::Simple(node) if node.with_clause().is_empty() => {
            let clauses = query.select_clauses().expect("simple SELECT clauses");
            if clauses.distinct().is_none()
                && clauses.group().is_empty()
                && clauses.having().is_none()
                && clauses.windows().is_empty()
            {
                Ok(query
                    .source_scope()
                    .expect("simple SELECT sources")
                    .single_relation_name())
            } else {
                Ok(None)
            }
        }
        _ => Ok(None),
    }
}

fn projection_preserves_rows(
    context: &impl ParseContext,
    query: &crate::sem::SelectStatement,
) -> Result<bool, AnalysisError> {
    let mut check = ProjectionRows {
        context,
        preserves: true,
        depth: 0,
    };
    query.result_columns().clone().walk(&mut check)?;
    Ok(check.preserves)
}
struct ProjectionRows<'a, C> {
    context: &'a C,
    preserves: bool,
    depth: usize,
}
impl<C: ParseContext> crate::sem::traversal::SemanticVisitorMut for ProjectionRows<'_, C> {
    fn enter_query(&mut self) {
        self.depth += 1;
    }
    fn leave_query(&mut self) {
        self.depth -= 1;
    }
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        if self.depth == 0 {
            self.preserves &= expression.preserves_view_rows(self.context);
        }
        Ok(())
    }
}

/// A view-visible column and its underlying assignment key within a destination.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ViewAssignmentColumn {
    name: ColumnName,
    source: ColumnName,
}
impl ViewAssignmentColumn {
    /// Record a proven alias correspondence, without choosing a relation.
    pub fn new(name: &ColumnName, source: &ColumnName) -> Self {
        Self {
            name: name.clone(),
            source: source.clone(),
        }
    }
    /// Name visible in the view.
    pub const fn name(&self) -> &ColumnName {
        &self.name
    }
    /// Key used to detect assignments to the same underlying column.
    pub const fn source_column(&self) -> &ColumnName {
        &self.source
    }
}

fn missing_definition(name: &FullName) -> ResolutionFailure {
    ResolutionFailure::view(&ViewProblem::missing_metadata(name))
}

fn lookup_bound_view<'a>(
    context: &'a impl ParseContext,
    name: &FullName,
) -> Result<Option<&'a crate::sem::ViewLike>, ResolutionFailure> {
    if name.schema().is_none() {
        return Err(ResolutionFailure::view(&ViewProblem::missing_metadata(
            name,
        )));
    }
    Ok(context.get_view(&crate::names::ViewName::from_full_name(name)))
}
