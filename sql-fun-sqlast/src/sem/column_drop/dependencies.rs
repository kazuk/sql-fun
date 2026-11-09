//! Classify known bound dependencies; publication and full closure belong to #1076.

use crate::{
    ast::Dialect,
    context::ParseContext,
    names::{ColumnName, FullName},
    sem::{
        AnalysisError, ColumnDrop, CreateTable, DropDependency, DropDependencyDecision,
        DropDependencyKind, DropKnowledgeGap, RuleReferences, SemScalarExpr, TriggerReferences,
        traversal::SemanticVisitorMut,
    },
};

impl ColumnDrop {
    pub(super) fn analyze_dependencies(
        &mut self,
        context: &impl ParseContext,
        table: &CreateTable,
    ) -> Result<(), AnalysisError> {
        let sqlite = context.dialect() == Dialect::sqlite();
        table.collect_drop_dependencies(self, &self.table.full_name().clone(), sqlite)?;
        for index in context.indexes_for_table(&self.table) {
            if index.depends_on_column(self)? {
                self.named_dependency(
                    self.table.full_name().clone(),
                    index.name(),
                    DropDependencyKind::index(),
                    !sqlite,
                );
            }
        }
        for name in context.relation_names() {
            self.analyze_relation_dependencies(context, &name, sqlite)?;
        }
        Ok(())
    }
    fn analyze_relation_dependencies(
        &mut self,
        context: &impl ParseContext,
        name: &FullName,
        sqlite: bool,
    ) -> Result<(), AnalysisError> {
        if let Some(table) = context.get_table_impl(name) {
            if name != self.table.full_name() {
                table.collect_drop_dependencies(self, name, sqlite)?;
            }
            self.analyze_links(
                context,
                name,
                Some(table.triggers()),
                Some(table.rules()),
                sqlite,
            )?;
        }
        if let Some(view) = context.get_view_impl(name) {
            let mut probe = self.probe();
            view.clone().walk_column_dependencies(&mut probe)?;
            if probe.found {
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
            self.analyze_links(context, name, view.triggers(), view.rules(), sqlite)?;
        }
        Ok(())
    }
    fn analyze_links(
        &mut self,
        context: &impl ParseContext,
        name: &FullName,
        triggers: Option<&TriggerReferences>,
        rules: Option<&RuleReferences>,
        sqlite: bool,
    ) -> Result<(), AnalysisError> {
        if let Some(triggers) = triggers {
            for trigger in triggers.resolve(context, name) {
                let mut probe = self.probe();
                trigger.clone().walk_column_dependencies(&mut probe)?;
                // SQLite retains UPDATE OF names even when the column is absent.
                if probe.found || (!sqlite && trigger.drop_event_depends_on(self)) {
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
                let mut probe = self.probe();
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
    pub(crate) fn matches_name(&self, column: &ColumnName) -> bool {
        let Some(original) = &self.original else {
            return false;
        };
        let Some(bound) = original.name() else {
            return false;
        };
        if self.sqlite {
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
        expression: &SemScalarExpr,
    ) -> Result<bool, AnalysisError> {
        let mut probe = self.probe();
        expression.clone().walk(&mut probe)?;
        self.retain_probe(&probe);
        Ok(probe.found)
    }
    pub(crate) fn probe(&self) -> ColumnDependencyProbe {
        ColumnDependencyProbe {
            relation: self.table.full_name().clone(),
            column: if let Some(original) = &self.original {
                original.name().clone()
            } else {
                None
            },
            found: false,
            unbound: false,
            sqlite: self.sqlite,
            expanded: false,
            expanded_found: false,
            nominal_rows: 0,
        }
    }
    pub(crate) fn retain_probe(&mut self, probe: &ColumnDependencyProbe) {
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
pub(crate) struct ColumnDependencyProbe {
    relation: FullName,
    column: Option<ColumnName>,
    found: bool,
    unbound: bool,
    sqlite: bool,
    expanded: bool,
    expanded_found: bool,
    nominal_rows: usize,
}
impl SemanticVisitorMut for ColumnDependencyProbe {
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
        self.expanded = self.sqlite && expanded;
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
