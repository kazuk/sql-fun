//! Catalog facts and analyzer-owned decisions for ordinary writable views.
//! Read projections remain in CreateView; these facts are never DML-owned.
use crate::sem::problem::ViewProblem;
use crate::sem::{
    AnalysisError, BaseParseContext, ColumnDefinition, ColumnName, FullName, SemScalarExpr,
    TableName, ViewInsertSupport, ViewUpdateSupport,
};

/// Catalog facts for one ordinary view's write analysis.
/// Retrieved by exact identity through [`crate::sem::ViewInfoRead`].
#[derive(Debug, Clone)]
pub struct ViewWriteInfo {
    name: FullName,
    retained_options: Option<RetainedViewOptions>,
    source: Option<FullName>,
    projections: Vec<(ColumnDefinition, Option<ColumnName>)>,
    expressions: Vec<(ColumnName, SemScalarExpr)>,
    objects: Vec<WriteObject>,
    defaults: Vec<(ColumnName, SemScalarExpr)>,
}

#[derive(Debug, Clone)]
struct RetainedViewOptions {
    _options: Vec<crate::ast::SyntaxOption>,
    _check_option: crate::ast::ViewCheckOption,
}
impl ViewWriteInfo {
    pub(crate) fn retain_options(
        &mut self,
        options: &[crate::ast::SyntaxOption],
        check_option: crate::ast::ViewCheckOption,
    ) {
        self.retained_options = Some(RetainedViewOptions {
            _options: options.to_vec(),
            _check_option: check_option,
        });
    }

    pub(crate) const fn name(&self) -> &FullName {
        &self.name
    }
    pub(crate) fn has_trigger(&self, trigger: &FullName) -> bool {
        self.name.schema() == trigger.schema()
            && self.objects.iter().any(|object| {
                !object.is_rule() && object.name.eq_ignore_ascii_case(trigger.name().as_str())
            })
    }
    pub(crate) fn replace_definition(&mut self, replacement: &Self) -> Result<(), ViewProblem> {
        if self.projections.len() > replacement.projections.len()
            || self
                .projections
                .iter()
                .zip(&replacement.projections)
                .any(|((old, _), (new, _))| {
                    old.name() != new.name() || old.get_type() != new.get_type()
                })
        {
            return Err(ViewProblem::invalid_replacement(&self.name));
        }
        self.source = replacement.source.clone();
        self.retained_options = replacement.retained_options.clone();
        self.projections = replacement.projections.clone();
        self.expressions = replacement.expressions.clone();
        Ok(())
    }

    pub(crate) fn new(
        name: &FullName,
        source: Option<&FullName>,
        projections: &[(ColumnDefinition, Option<ColumnName>)],
        expressions: &[(ColumnName, SemScalarExpr)],
    ) -> Self {
        Self {
            name: name.clone(),
            retained_options: None,
            source: source.cloned(),
            projections: projections.to_vec(),
            expressions: expressions.to_vec(),
            objects: Vec::new(),
            defaults: Vec::new(),
        }
    }

    pub(crate) fn apply(&mut self, change: &ViewWriteChange) -> Result<(), ViewProblem> {
        match &change.action {
            WriteAction::Sequence(actions) => self.apply_sequence(actions),
            WriteAction::Object(change) => self.apply_object(change),
            WriteAction::Default(default) => self.apply_default_change(default),
            WriteAction::Drop(change) => self.drop_object(change),
        }
    }

    fn apply_sequence(&mut self, actions: &[WriteAction]) -> Result<(), ViewProblem> {
        for action in actions {
            self.apply(&ViewWriteChange::new(&self.name, action))?;
        }
        Ok(())
    }

    fn apply_object(&mut self, change: &ObjectChange) -> Result<(), ViewProblem> {
        let exists = self
            .objects
            .iter()
            .any(|old| old.same_identity(&change.object));
        if exists && change.if_not_exists {
            return Ok(());
        }
        if exists && !change.replace {
            return Err(ViewProblem::duplicate_object(
                &self.name,
                &change.object.name,
            ));
        }
        self.objects
            .retain(|old| !old.same_identity(&change.object));
        self.objects.push(change.object.clone());
        Ok(())
    }

    fn apply_default_change(&mut self, default: &ViewDefault) -> Result<(), ViewProblem> {
        if !self
            .projections
            .iter()
            .any(|(column, _)| column.name().as_ref() == Some(&default.column))
        {
            return Err(ViewProblem::unknown_default_column(
                &self.name,
                default.column.as_str(),
            ));
        }
        self.defaults
            .retain(|(column, _)| column != &default.column);
        if let Some(value) = &default.value {
            self.defaults.push((default.column.clone(), value.clone()));
        }
        Ok(())
    }

    fn drop_object(&mut self, change: &ObjectChange) -> Result<(), ViewProblem> {
        if !change.if_not_exists
            && !self
                .objects
                .iter()
                .any(|old| old.same_identity(&change.object))
        {
            return Err(ViewProblem::missing_object(&self.name, &change.object.name));
        }
        self.objects
            .retain(|old| !old.same_identity(&change.object));
        Ok(())
    }

    fn matching_objects(&self, event: WriteEvent) -> impl Iterator<Item = &WriteObject> {
        self.objects
            .iter()
            .filter(move |object| object.enabled && object.events & event.mask() != 0)
    }

    fn has_rules(&self) -> bool {
        self.objects.iter().any(|object| {
            object.is_rule()
                && object.events & (WriteEvent::Insert.mask() | WriteEvent::Update.mask()) != 0
        })
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

    fn decision(
        &self,
        context: &impl BaseParseContext,
        event: WriteEvent,
        seen: &mut Vec<FullName>,
    ) -> Result<Decision, FullName> {
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
        context: &impl BaseParseContext,
        event: WriteEvent,
    ) -> Option<Decision> {
        let rules: Vec<_> = self
            .matching_objects(event)
            .filter(|object| object.is_rule())
            .collect();
        let unconditional = rules.iter().any(|object| object.replaces_unconditionally());
        let trigger = self
            .matching_objects(event)
            .any(|object| object.is_instead_trigger());
        if unconditional {
            let returning = rules
                .iter()
                .any(|object| object.replaces_unconditionally() && object.returning);
            return Some(Decision::writable(
                &self.visible_columns(),
                returning,
                false,
            ));
        }
        if trigger {
            let mut decision = Decision::writable(
                &self.visible_columns(),
                !rules.iter().any(|object| object.is_instead_rule()),
                !self.has_rules(),
            );
            if crate::ast::Dialect::from(context.metadata()) == crate::ast::Dialect::sqlite() {
                decision.required_trigger_columns = self.trigger_columns(event);
            }
            return Some(decision);
        }
        if crate::ast::Dialect::from(context.metadata()) == crate::ast::Dialect::sqlite() {
            return Some(Decision::read_only());
        }
        // Conditional INSTEAD rules prevent automatic rewriting, even when
        // no row would satisfy their condition. Triggers were considered above.
        if rules.iter().any(|object| object.is_instead_rule()) {
            return Some(Decision::read_only());
        }
        None
    }

    fn automatic_source_decision(
        &self,
        context: &impl BaseParseContext,
        event: WriteEvent,
        seen: &mut Vec<FullName>,
    ) -> Result<Decision, FullName> {
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
        context: &impl BaseParseContext,
        source: &FullName,
        event: WriteEvent,
        seen: &mut Vec<FullName>,
    ) -> Result<Decision, FullName> {
        let decision = if let Some(table) = context.get_table_impl(source) {
            let mut columns = Vec::new();
            while let Some(column) = table.get_column_at(columns.len()) {
                columns.push(column.clone());
            }
            Decision::writable(&columns, true, true)
        } else if let Some(view) = context.get_view_impl(source) {
            if view.is_materialized() {
                Decision::read_only()
            } else {
                context
                    .view_write_info(source)
                    .ok_or_else(|| source.clone())?
                    .decision(context, event, seen)?
            }
        } else {
            Decision::read_only()
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

    fn trigger_columns(&self, event: WriteEvent) -> Option<Vec<ColumnName>> {
        let mut names = Vec::new();
        for object in self.matching_objects(event) {
            if let Some(columns) = object.trigger_columns() {
                if columns.is_empty() {
                    return None;
                }
                for name in columns {
                    names.push(ColumnName::from(name.as_str()));
                }
            }
        }
        Some(names)
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
        if let Some((_, value)) = self
            .defaults
            .iter()
            .find(|(name, _)| column.name().as_ref() == Some(name))
        {
            column.replace_view_default(&self.name, value);
        }
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

pub(crate) fn insert_support(
    context: &impl BaseParseContext,
    view: &FullName,
) -> Result<ViewInsertSupport, ViewProblem> {
    if let Some(support) = context.view_insert_support(view) {
        return Ok(support.clone());
    }
    let decision = match context.view_write_info(view) {
        Some(info) => info.decision(context, WriteEvent::insert(), &mut vec![view.clone()]),
        None => Err(view.clone()),
    };
    let decision = match decision {
        Ok(decision) => decision,
        Err(missing) => return Err(ViewProblem::missing_metadata(&missing)),
    };
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
pub(crate) fn update_support(
    context: &impl BaseParseContext,
    view: &FullName,
) -> Result<ViewUpdateSupport, ViewProblem> {
    if let Some(support) = context.view_update_support(view) {
        return Ok(support.clone());
    }
    let decision = match context.view_write_info(view) {
        Some(info) => info.decision(context, WriteEvent::update(), &mut vec![view.clone()]),
        None => Err(view.clone()),
    };
    let decision = match decision {
        Ok(decision) => decision,
        Err(missing) => return Err(ViewProblem::missing_metadata(&missing)),
    };
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

/// A qualified schema change retained for writable-view catalog analysis.
/// It records metadata, without executing trigger functions or rule actions.
#[derive(Debug, Clone)]
pub struct ViewWriteChange {
    view: FullName,
    action: WriteAction,
}
impl ViewWriteChange {
    pub(crate) fn new(view: &FullName, action: &WriteAction) -> Self {
        Self {
            view: view.clone(),
            action: action.clone(),
        }
    }
    /// Number of retained PostgreSQL rule actions, if this declares a rule.
    /// Bodies are catalog evidence; they are not executed by this analyzer.
    pub const fn rule_action_count(&self) -> Option<usize> {
        self.action.rule_action_count()
    }

    /// The exact view identity affected by this change.
    pub const fn view(&self) -> &FullName {
        &self.view
    }
}

#[derive(Debug, Clone)]
pub(crate) enum WriteAction {
    Sequence(Vec<Self>),
    #[expect(
        dead_code,
        reason = "legacy rule metadata construction disconnected by #600; replaced by #604"
    )]
    Object(ObjectChange),
    Default(Box<ViewDefault>),
    Drop(ObjectChange),
}
impl WriteAction {
    const fn rule_action_count(&self) -> Option<usize> {
        if let Self::Object(object) = self {
            object.object.rule_action_count()
        } else {
            None
        }
    }

    pub(crate) fn sequence(actions: &[Self]) -> Self {
        Self::Sequence(actions.to_vec())
    }
    pub(crate) fn trigger_declaration(&self) -> Option<(&str, bool)> {
        if let Self::Object(change) = self
            && !change.object.is_rule()
        {
            return Some((&change.object.name, change.if_not_exists));
        }
        None
    }
    #[expect(
        dead_code,
        reason = "legacy rule metadata construction disconnected by #600; replaced by #604"
    )]
    pub(crate) fn object(object: &WriteObject, replace: bool, if_not_exists: bool) -> Self {
        Self::Object(ObjectChange {
            object: object.clone(),
            replace,
            if_not_exists,
        })
    }
    pub(crate) fn default(column: &ColumnName, value: Option<&SemScalarExpr>) -> Self {
        Self::Default(Box::new(ViewDefault {
            column: column.clone(),
            value: value.cloned(),
        }))
    }
    pub(crate) fn drop_object(object: &WriteObject, if_exists: bool) -> Self {
        Self::Drop(ObjectChange {
            object: object.clone(),
            replace: false,
            if_not_exists: if_exists,
        })
    }
}
#[derive(Debug, Clone)]
pub(crate) struct ObjectChange {
    object: WriteObject,
    replace: bool,
    if_not_exists: bool,
}
#[derive(Debug, Clone)]
pub(crate) struct ViewDefault {
    column: ColumnName,
    value: Option<SemScalarExpr>,
}

pub(crate) const TRIGGER_EVENT_INSERT: i32 = 4;
pub(crate) const TRIGGER_EVENT_DELETE: i32 = 8;
pub(crate) const TRIGGER_EVENT_UPDATE: i32 = 16;

#[derive(Debug, Clone, Copy)]
pub(crate) enum WriteEvent {
    Insert,
    Update,
    #[expect(
        dead_code,
        reason = "legacy rule metadata construction disconnected by #600; replaced by #604"
    )]
    Delete,
}
impl WriteEvent {
    pub(crate) const fn insert() -> Self {
        Self::Insert
    }
    pub(crate) const fn update() -> Self {
        Self::Update
    }
    #[expect(
        dead_code,
        reason = "legacy rule metadata construction disconnected by #600; replaced by #604"
    )]
    pub(crate) const fn delete() -> Self {
        Self::Delete
    }
    const fn mask(self) -> i32 {
        match self {
            Self::Insert => TRIGGER_EVENT_INSERT,
            Self::Update => TRIGGER_EVENT_UPDATE,
            Self::Delete => TRIGGER_EVENT_DELETE,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WriteObject {
    name: String,
    events: i32,
    kind: WriteObjectKind,
    returning: bool,
    enabled: bool,
}
#[derive(Debug, Clone)]
enum WriteObjectKind {
    #[expect(
        dead_code,
        reason = "legacy trigger metadata is disconnected by #598/#599; replaced by #555"
    )]
    Trigger(TriggerFacts),
    Rule(RuleFacts),
}
#[derive(Debug, Clone)]
struct TriggerFacts {
    instead: bool,
    // Retain column/event applicability; trigger execution is outside static analysis.
    columns: Vec<String>,
}
#[derive(Debug, Clone)]
struct RuleFacts {
    instead: bool,
    conditional: bool,
    actions: Vec<crate::syn::Node>,
}
impl WriteObject {
    const fn rule_action_count(&self) -> Option<usize> {
        if let WriteObjectKind::Rule(facts) = &self.kind {
            Some(facts.actions.len())
        } else {
            None
        }
    }

    #[expect(
        dead_code,
        reason = "legacy trigger constructor is disconnected by #598/#599; replaced by #555"
    )]
    pub(crate) fn trigger(name: &str, events: i32, instead: bool, columns: &[String]) -> Self {
        Self {
            name: name.to_owned(),
            events,
            kind: WriteObjectKind::Trigger(TriggerFacts {
                instead,
                columns: columns.to_vec(),
            }),
            returning: false,
            enabled: true,
        }
    }
    pub(crate) fn rule(
        name: &str,
        event: WriteEvent,
        instead: bool,
        conditional: bool,
        returning: bool,
        actions: &[crate::syn::Node],
    ) -> Self {
        Self {
            name: name.to_owned(),
            events: event.mask(),
            kind: WriteObjectKind::Rule(RuleFacts {
                instead,
                conditional,
                actions: actions.to_vec(),
            }),
            returning,
            enabled: true,
        }
    }
    fn trigger_columns(&self) -> Option<&[String]> {
        if let WriteObjectKind::Trigger(facts) = &self.kind
            && facts.instead
        {
            return Some(&facts.columns);
        }
        None
    }
    const fn is_rule(&self) -> bool {
        matches!(self.kind, WriteObjectKind::Rule(_))
    }
    const fn is_instead_trigger(&self) -> bool {
        matches!(&self.kind, WriteObjectKind::Trigger(facts) if facts.instead)
    }
    const fn is_instead_rule(&self) -> bool {
        matches!(&self.kind, WriteObjectKind::Rule(facts) if facts.instead)
    }
    const fn replaces_unconditionally(&self) -> bool {
        matches!(&self.kind, WriteObjectKind::Rule(facts) if facts.instead && !facts.conditional)
    }
    fn same_identity(&self, other: &Self) -> bool {
        self.name == other.name && self.is_rule() == other.is_rule()
    }
}

pub(crate) fn infer_view_arbiters(
    context: &impl BaseParseContext,
    view: &TableName,
    conflict: &crate::sem::InsertConflict,
) -> Result<crate::sem::ConflictArbiterInference, AnalysisError> {
    let Some(info) = context.view_write_info(view.full_name()) else {
        return Ok(crate::sem::ConflictArbiterInference::missing_view_metadata(
            view.full_name(),
        ));
    };
    if let Err(missing) = info.decision(
        context,
        WriteEvent::insert(),
        &mut vec![view.full_name().clone()],
    ) {
        return Ok(crate::sem::ConflictArbiterInference::missing_view_metadata(
            &missing,
        ));
    }
    if info
        .matching_objects(WriteEvent::insert())
        .any(|object| object.is_instead_rule() || object.is_instead_trigger())
    {
        return Ok(crate::sem::ConflictArbiterInference::complete(&[]));
    }
    let Some(source) = &info.source else {
        return Ok(crate::sem::ConflictArbiterInference::complete(&[]));
    };
    let mut mapped = conflict.clone();
    if !mapped.map_view_columns(context, info, source)? {
        return Ok(crate::sem::ConflictArbiterInference::complete(&[]));
    }
    let relation = TableName::from(source.clone());
    let inference = context.infer_conflict_arbiters(&relation, &mapped)?;
    if context.get_table_impl(source).is_some() {
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
    pub(crate) fn map_view_columns(
        &mut self,
        context: &impl BaseParseContext,
        view: &ViewWriteInfo,
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
    view: &'a ViewWriteInfo,
    source: &'a FullName,
    complete: bool,
}
impl<C: BaseParseContext> crate::sem::traversal::SemanticVisitor for ViewColumnMapper<'_, C> {
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
        let definition = if let Some(table) = self.context.get_table_impl(self.source) {
            table.get_column_def(mapped).cloned()
        } else if let Some(view) = self.context.get_view_impl(self.source) {
            view.get_column_def(mapped)
        } else {
            None
        };
        let alias = crate::sem::AliasName::from(self.source.name().as_str());
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

pub(crate) fn projection_preserves_rows(
    context: &impl BaseParseContext,
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
impl<C: BaseParseContext> crate::sem::traversal::SemanticVisitor for ProjectionRows<'_, C> {
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
