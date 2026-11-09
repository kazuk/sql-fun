//! Classify known bound dependencies; publication and full closure belong to #1076.

use crate::{
    ast::Dialect,
    context::ParseContext,
    names::{ColumnName, FullName},
    sem::{
        AnalysisError, ColumnDrop, CreateTable, DropDependency, DropDependencyDecision,
        DropDependencyKind, DropKnowledgeGap, QueryAccess, RuleReferences, SemScalarExpr,
        TriggerReferences, traversal::SemanticVisitorMut,
    },
};

impl ColumnDrop {
    pub(super) fn analyze_dependencies(
        &mut self,
        context: &impl ParseContext,
        table: &CreateTable,
    ) -> Result<(), AnalysisError> {
        let sqlite = context.dialect() == Dialect::sqlite();
        table.collect_drop_dependencies(context, self, &self.table.full_name().clone())?;
        for index in context.indexes_for_table(&self.table) {
            if index.depends_on_column(context, self)? {
                self.named_dependency(
                    self.table.full_name().clone(),
                    index.name(),
                    DropDependencyKind::index(),
                    !sqlite,
                );
            }
        }
        for name in context.relation_names() {
            self.analyze_relation_dependencies(context, &name)?;
        }
        Ok(())
    }
    fn analyze_relation_dependencies(
        &mut self,
        context: &impl ParseContext,
        name: &FullName,
    ) -> Result<(), AnalysisError> {
        if let Some(table) = context.get_table_impl(name) {
            if name != self.table.full_name() {
                table.collect_drop_dependencies(context, self, name)?;
            }
            self.analyze_links(context, name, Some(table.triggers()), Some(table.rules()))?;
        }
        if let Some(view) = context.get_view_impl(name) {
            let mut probe = self.probe(context);
            view.clone().walk_column_dependencies(&mut probe)?;
            let join_keys = QueryAccess::from_explicit_join_keys(context, view.query())?;
            let join_dependency = join_keys.records().iter().any(|access| {
                access.relation() == self.table.full_name()
                    && access
                        .column_name()
                        .as_ref()
                        .is_some_and(|column| self.matches_name(context, column))
            });
            if probe.found || join_dependency {
                self.named_dependency(name.clone(), name, DropDependencyKind::view(), false);
            } else if probe.expanded_found {
                self.dependencies.push(DropDependency {
                    relation: name.clone(),
                    object: Some(name.clone()),
                    member: String::new(),
                    kind: DropDependencyKind::view(),
                    decision: DropDependencyDecision::Revalidate,
                });
            }
            self.retain_probe(&probe);
            self.analyze_links(context, name, view.triggers(), view.rules())?;
        }
        Ok(())
    }
    fn analyze_links(
        &mut self,
        context: &impl ParseContext,
        name: &FullName,
        triggers: Option<&TriggerReferences>,
        rules: Option<&RuleReferences>,
    ) -> Result<(), AnalysisError> {
        if let Some(triggers) = triggers {
            for trigger in triggers.resolve(context, name) {
                let mut probe = self.probe(context);
                trigger.clone().walk_column_dependencies(&mut probe)?;
                // SQLite retains UPDATE OF names even when the column is absent.
                if probe.found
                    || (context.dialect() != Dialect::sqlite()
                        && trigger.drop_event_depends_on(context, self))
                {
                    self.named_dependency(
                        name.clone(),
                        trigger.identity().name(),
                        DropDependencyKind::trigger(),
                        false,
                    );
                }
                self.retain_probe(&probe);
            }
        }
        if let Some(rules) = rules {
            for rule in rules.resolve(context) {
                let mut probe = self.probe(context);
                rule.clone().walk(&mut probe)?;
                if probe.found {
                    self.named_dependency(
                        name.clone(),
                        rule.identity().name(),
                        DropDependencyKind::rule(),
                        false,
                    );
                }
                self.retain_probe(&probe);
            }
        }
        Ok(())
    }
    pub(crate) fn matches_name(&self, context: &impl ParseContext, column: &ColumnName) -> bool {
        let Some(original) = &self.original else {
            return false;
        };
        let Some(bound) = original.name() else {
            return false;
        };
        if context.dialect() == Dialect::sqlite() {
            bound.as_str().eq_ignore_ascii_case(column.as_str())
        } else {
            bound == column
        }
    }
    fn named_dependency(
        &mut self,
        relation: FullName,
        object: &FullName,
        kind: DropDependencyKind,
        automatic: bool,
    ) {
        self.dependency(relation, object.local_name().as_str(), kind, automatic);
        self.dependencies
            .last_mut()
            .expect("inserted dependency")
            .object = Some(object.clone());
    }
    pub(crate) fn dependency(
        &mut self,
        relation: FullName,
        member: &str,
        kind: DropDependencyKind,
        automatic: bool,
    ) {
        let decision = if automatic {
            DropDependencyDecision::Automatic
        } else if self.cascade {
            DropDependencyDecision::Cascade
        } else {
            DropDependencyDecision::Reject
        };
        self.dependencies.push(DropDependency {
            relation,
            object: None,
            member: member.to_owned(),
            kind,
            decision,
        });
    }
    pub(crate) fn expression_dependency(
        &mut self,
        context: &impl ParseContext,
        expression: &SemScalarExpr,
    ) -> Result<bool, AnalysisError> {
        let mut probe = self.probe(context);
        expression.clone().walk(&mut probe)?;
        self.retain_probe(&probe);
        Ok(probe.found)
    }
    pub(crate) fn probe<'a, C: ParseContext>(
        &self,
        context: &'a C,
    ) -> ColumnDependencyProbe<'a, C> {
        ColumnDependencyProbe {
            relation: self.table.full_name().clone(),
            column: if let Some(original) = &self.original {
                original.name().clone()
            } else {
                None
            },
            found: false,
            unbound: false,
            context,
            expanded: false,
            expanded_found: false,
            nominal_rows: 0,
        }
    }
    pub(crate) fn retain_probe(&mut self, probe: &ColumnDependencyProbe<'_, impl ParseContext>) {
        if probe.unbound
            && !self
                .knowledge_gaps
                .contains(&DropKnowledgeGap::UnboundExpression)
        {
            self.knowledge_gaps
                .push(DropKnowledgeGap::UnboundExpression);
        }
    }
}

/// Observes bound expressions on private clones without altering the input catalog.
pub(crate) struct ColumnDependencyProbe<'a, C> {
    relation: FullName,
    column: Option<ColumnName>,
    found: bool,
    unbound: bool,
    context: &'a C,
    expanded: bool,
    expanded_found: bool,
    nominal_rows: usize,
}
impl<C: ParseContext> SemanticVisitorMut for ColumnDependencyProbe<'_, C> {
    fn walk_source_column_values(&self) -> bool {
        false
    }
    fn projection(
        &mut self,
        _name: &mut Option<ColumnName>,
        _explicit: bool,
        expanded: bool,
        _value: &mut SemScalarExpr,
    ) -> Result<(), AnalysisError> {
        self.expanded = self.context.dialect() == Dialect::sqlite() && expanded;
        Ok(())
    }
    fn enter_expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        if expression
            .as_whole_row()
            .is_some_and(|row| row.relation().is_some())
        {
            self.nominal_rows += 1;
        }
        Ok(())
    }
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        if expression
            .as_whole_row()
            .is_some_and(|row| row.relation().is_some())
        {
            self.nominal_rows -= 1;
        }
        if self.nominal_rows != 0 {
            return Ok(());
        }
        // Nominal rows depend on the relation, not every field in their captured shape.
        // A member selection instead depends on just its bound exposed field.
        if let Some(value) = expression.selected_row_field() {
            value.clone().walk(self)?;
        }
        if let Some(reference) = expression.table_column_reference() {
            let matches = reference.relation() == &self.relation
                && reference.bound_column() == self.column.as_ref();
            if std::mem::take(&mut self.expanded) {
                self.expanded_found |= matches;
            } else {
                self.found |= matches;
            }
        }
        self.unbound |= !expression.check_references_resolved();
        Ok(())
    }
}
