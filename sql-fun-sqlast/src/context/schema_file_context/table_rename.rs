//! Stage dependent metadata before committing one catalog identity transition.
use crate::context::schema_file_context::{SchemaCatalog, SchemaFileContext};
use crate::{
    context::{
        FunctionInfoRead, IndexInfoRead, ObjectInfoRead, OwnershipInfoRead, ParseContext,
        SequenceInfoRead, TableInfoRead, TypeInfoRead, ViewInfoRead,
    },
    sem::{
        AlterTable, AnalysisError, CreateFunction, CreateIndex, CreateRule, CreateSequence,
        CreateTable, CreateTrigger, FullName, OverloadVariant, RuleIdentity, RuleReferences,
        TableName, TriggerIdentity, TriggerReferences, TypeDefinition, ViewLike,
    },
};

impl SchemaFileContext {
    pub(super) fn apply_table_rename(
        mut self,
        alteration: &AlterTable,
        destination: &TableName,
    ) -> Result<Self, AnalysisError> {
        let Some(original) = alteration.resolved_target() else {
            return AnalysisError::raise_unexpected_none("applicable rename target");
        };
        let Some(mut definition) = self.get_table_impl(original.full_name()).cloned() else {
            return AnalysisError::raise_unexpected_none("bound rename table");
        };
        definition.apply_alter(alteration)?;
        definition.rename_relation(original, destination);
        definition.rename_expressions(original, destination)?;
        let updates = RenameUpdates::prepare(&self, original, destination)?;
        // All fallible work precedes the commit. A frozen parent is never changed.
        let key = self.catalog.relation_identity(original.full_name());
        self.catalog.renamed_relations.insert(key.clone());
        self.catalog.renamed_tables.insert(key.clone());
        if self.dialect() == crate::ast::Dialect::postgres() {
            self.catalog.renamed_types.insert(key);
            self.catalog.types.insert_table_ref(destination);
        }
        self.catalog
            .store_table(destination.full_name(), &definition);
        updates.commit(&mut self.catalog, destination);
        Ok(self)
    }
}

#[derive(Default)]
pub(super) struct RenameUpdates {
    tables: Vec<(FullName, CreateTable)>,
    views: Vec<(FullName, ViewLike)>,
    triggers: Vec<(TriggerIdentity, CreateTrigger)>,
    rules: Vec<(RuleIdentity, CreateRule)>,
    types: Vec<(FullName, TypeDefinition)>,
    routines: Vec<(FullName, Vec<CreateFunction>)>,
    routine_properties: Vec<((FullName, OverloadVariant), bool)>,
    indexes: Vec<CreateIndex>,
    sequences: Vec<CreateSequence>,
}

impl RenameUpdates {
    fn prepare(
        context: &SchemaFileContext,
        old: &TableName,
        new: &TableName,
    ) -> Result<Self, AnalysisError> {
        let mut updates = Self::default();
        updates.collect_relations(context, &RenameChange::relation(old, new))?;
        updates.collect_types(context, old, new);
        updates.collect_routines(context, old, new);
        updates.indexes = context
            .indexes_for_table(old)
            .into_iter()
            .cloned()
            .collect();
        for index in &mut updates.indexes {
            index.rename_relation(old, new)?;
        }
        updates.sequences = context.sequences_owned_by(old);
        Ok(updates)
    }

    pub(super) fn prepare_column(
        context: &SchemaFileContext,
        rename: &crate::sem::ColumnRename,
    ) -> Result<Self, AnalysisError> {
        let mut updates = Self::default();
        updates.collect_relations(context, &RenameChange::column(rename))?;
        updates.indexes = context
            .indexes_for_table(rename.table())
            .into_iter()
            .cloned()
            .collect();
        for index in &mut updates.indexes {
            index.rename_column(rename)?;
        }
        Ok(updates)
    }
    fn collect_relations(
        &mut self,
        context: &SchemaFileContext,
        change: &RenameChange<'_>,
    ) -> Result<(), AnalysisError> {
        for name in context.relation_names() {
            if let Some(table) = context.get_table_impl(&name) {
                self.collect_links(context, &name, table.triggers(), table.rules(), change)?;
                if &name != change.original().full_name() {
                    let mut table = table.clone();
                    change.table(&mut table)?;
                    self.tables.push((name.clone(), table));
                }
            }
            if let Some(view) = context.get_view_impl(&name) {
                if let (Some(triggers), Some(rules)) = (view.triggers(), view.rules()) {
                    self.collect_links(context, &name, triggers, rules, change)?;
                }
                let mut view = view.clone();
                change.view(context, &mut view)?;
                self.views.push((name, view));
            }
        }
        Ok(())
    }
    fn collect_links(
        &mut self,
        context: &SchemaFileContext,
        name: &FullName,
        triggers: &TriggerReferences,
        rules: &RuleReferences,
        change: &RenameChange<'_>,
    ) -> Result<(), AnalysisError> {
        for trigger in triggers.resolve(context, name) {
            let identity = trigger.identity().clone();
            let mut trigger = trigger.clone();
            change.trigger(context, &mut trigger)?;
            self.triggers.push((identity, trigger));
        }
        for rule in rules.resolve(context) {
            let identity = rule.identity().clone();
            let mut rule = rule.clone();
            change.rule(&mut rule)?;
            self.rules.push((identity, rule));
        }
        Ok(())
    }

    fn collect_types(&mut self, context: &SchemaFileContext, old: &TableName, new: &TableName) {
        for reference in context.type_references() {
            if reference.is_array() == Some(false)
                && let Some(name) = reference.full_name()
                && name != old.full_name()
                && let Some(mut definition) = context.get_type_impl(name).cloned()
            {
                let before = definition.clone();
                definition.rename_relation(old, new);
                if definition != before {
                    self.types.push((name.clone(), definition));
                }
            }
        }
    }

    fn collect_routines(&mut self, context: &SchemaFileContext, old: &TableName, new: &TableName) {
        for name in context.declared_routine_names() {
            let mut definitions = context.routine_definitions(&name);
            let mut changed = false;
            let mut properties = Vec::new();
            for definition in &mut definitions {
                properties.extend(
                    definition
                        .overloads()
                        .renamed_row_properties(context, &name, old, new),
                );
                changed |= definition.rename_relation(old, new);
            }
            if changed {
                self.routine_properties.extend(properties);
                self.routines.push((name, definitions));
            }
        }
    }

    pub(super) fn commit(self, catalog: &mut SchemaCatalog, new: &TableName) {
        for (identity, trigger) in self.triggers {
            catalog.triggers.insert(identity, None);
            catalog
                .triggers
                .insert(trigger.identity().clone(), Some(trigger));
        }
        for (identity, rule) in self.rules {
            catalog.rules.insert(identity, None);
            catalog.rules.insert(rule.identity().clone(), Some(rule));
        }
        for (name, definitions) in self.routines {
            catalog
                .function_view_rows
                .retain(|(function, _), _| function != &name);
            catalog.replaced_routines.insert(name.clone());
            catalog.functions.replace(&name, &definitions);
        }
        catalog.function_view_rows.extend(self.routine_properties);
        for (name, definition) in self.types {
            catalog.types.replace_definition(&name, &definition);
        }
        for (name, table) in self.tables {
            catalog.store_table(&name, &table);
        }
        for (name, view) in self.views {
            catalog.views.insert(&name, &view);
        }
        for index in self.indexes {
            catalog.store_index(&index);
        }
        for mut sequence in self.sequences {
            sequence.set_owned_by(new);
            catalog.store_sequence(sequence.name(), &sequence);
        }
    }
}

// The catalog inventory and staging rules are shared by both identity edits.
enum RenameChange<'a> {
    Relation(&'a TableName, &'a TableName),
    Column(&'a crate::sem::ColumnRename),
}
impl<'a> RenameChange<'a> {
    const fn relation(old: &'a TableName, new: &'a TableName) -> Self {
        Self::Relation(old, new)
    }
    const fn column(rename: &'a crate::sem::ColumnRename) -> Self {
        Self::Column(rename)
    }
    const fn original(&self) -> &TableName {
        match self {
            Self::Relation(old, _) => old,
            Self::Column(rename) => rename.table(),
        }
    }
    fn table(&self, table: &mut CreateTable) -> Result<(), AnalysisError> {
        match self {
            Self::Relation(old, new) => {
                table.rename_relation(old, new);
                table.rename_expressions(old, new)
            }
            Self::Column(rename) => table.rename_column_dependencies(rename, false),
        }
    }
    fn view(&self, context: &SchemaFileContext, view: &mut ViewLike) -> Result<(), AnalysisError> {
        match self {
            Self::Relation(old, new) => view.rename_relation(old, new),
            Self::Column(rename) if rename.rewrites_outputs() => {
                rename.rewrite_sqlite_view(context, view)
            }
            Self::Column(rename) => view.rename_column(rename),
        }
    }
    fn trigger(
        &self,
        context: &SchemaFileContext,
        trigger: &mut CreateTrigger,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Relation(old, new) => trigger.rename_relation(old, new),
            Self::Column(rename) if rename.rewrites_outputs() => {
                rename.rewrite_sqlite_trigger(context, trigger)
            }
            Self::Column(rename) => trigger.rename_column(rename),
        }
    }
    fn rule(&self, rule: &mut CreateRule) -> Result<(), AnalysisError> {
        match self {
            Self::Relation(old, new) => rule.rename_relation(old, new),
            Self::Column(rename) => rule.rename_column(rename),
        }
    }
}
