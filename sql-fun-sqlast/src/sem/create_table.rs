mod ddl;
mod serial;
pub use self::column_def::ColumnDefinition;
pub(crate) use self::serial::SerialSequences;
use crate::{
    ast::{CreateTableMode, Dialect},
    context::{GetSourcePathWithSpan, ParseContext},
    names::{ColumnName, FullName, TableName},
    sem::{
        AlterTable, AnalysisError, AnalysisProblem, CheckConstraint, ColumnAttributeAction,
        ColumnDrop, ColumnRename, Constraint, ConstraintNames, CreateIndex, CreateSequence,
        ForeignKeyConstraint, ImplicitChange, IndirectionExpr, InsertArbiter, Kind,
        OwnerSpecification, RuleIdentity, RuleReferences, SemScalarExpr, SourcePathWithSpan,
        TriggerReferences, TypeReference,
        alter_table::AlterObjSubCommand,
        problem::CreationProblem,
        traversal::{RelationRenamer, SemanticVisitorMut},
    },
};
use std::collections::HashMap;

mod column_def;

/// columns in table
#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColumnCollection(Vec<ColumnDefinition>);

impl ColumnCollection {
    pub(crate) fn check_scope_columns(&self) -> Self {
        // Bind CHECK to row fields, not recursively retained column CHECK bodies.
        Self(
            self.0
                .iter()
                .map(|column| {
                    ColumnDefinition::new(
                        column.name(),
                        column.get_type(),
                        column.is_not_null(),
                        &[],
                    )
                })
                .collect(),
        )
    }

    pub(crate) fn with_declared_column(mut self, column: &ColumnDefinition) -> Self {
        if let Some(name) = column.name()
            && let Some(existing) = self.get_column_mut(name)
        {
            *existing = column.clone();
            return self;
        }
        self.0.push(column.clone());
        self
    }

    pub(crate) fn common_function_shape(outputs: &[Self]) -> Option<Self> {
        let first = outputs.first()?;
        if outputs.iter().any(|output| output.names() != first.names()) {
            return None;
        }
        Some(Self(
            first
                .0
                .iter()
                .enumerate()
                .map(|(index, column)| {
                    let ty = column.get_type().filter(|ty| {
                        !ty.is_simple_infer_arg()
                            && !ty.is_common_infer_arg()
                            && outputs
                                .iter()
                                .all(|output| output.0[index].get_type() == Some(*ty))
                    });
                    ColumnDefinition::new(column.name(), ty, column.is_not_null(), &[])
                })
                .collect(),
        ))
    }

    pub(crate) fn refresh_function_output(&mut self, columns: &Self) {
        // Ordinality belongs to the invocation and follows the function's fields.
        let trailing = self.0.get(columns.0.len()..).unwrap_or_default().to_vec();
        *self = columns.clone();
        self.0.extend(trailing);
    }

    pub(crate) fn fmt_type_shape(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "row(")?;
        for (index, column) in self.0.iter().enumerate() {
            if index > 0 {
                write!(f, ", ")?;
            }
            if let Some(ty) = column.get_type() {
                write!(f, "{ty}")?;
            } else {
                write!(f, "?")?;
            }
        }
        write!(f, ")")
    }

    fn inline_primary_key_columns(&self) -> Vec<&ColumnName> {
        self.0
            .iter()
            .filter(|column| column.has_inline_primary_key())
            .filter_map(|column| column.name().as_ref())
            .collect()
    }

    pub(crate) fn project_expression(
        &self,
        base: &SemScalarExpr,
    ) -> Vec<(ColumnName, SemScalarExpr)> {
        self.0
            .iter()
            .map(|field| {
                (
                    field.name().as_ref().expect("composite field name").clone(),
                    IndirectionExpr::projected_member(base, field),
                )
            })
            .collect()
    }

    pub(crate) fn find_column(&self, name: &ColumnName, sqlite: bool) -> Option<&ColumnDefinition> {
        self.0.iter().find(|column| {
            column.name().as_ref().is_some_and(|candidate| {
                if sqlite {
                    candidate.as_str().eq_ignore_ascii_case(name.as_str())
                } else {
                    candidate == name
                }
            })
        })
    }
    pub(crate) fn rename_column(&mut self, rename: &ColumnRename) {
        for column in &mut self.0 {
            column.rename_column_name(rename);
        }
    }

    pub(crate) fn declarations_can_apply(&self) -> bool {
        self.0.iter().all(ColumnDefinition::declaration_can_apply)
    }

    fn rename_expressions(
        &mut self,
        visitor: &mut impl SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        for column in &mut self.0 {
            column.rename_expressions(visitor)?;
        }
        Ok(())
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        for column in &mut self.0 {
            column.rename_relation(old, new);
        }
    }

    pub(crate) fn column_at(&self, index: usize) -> Option<&ColumnDefinition> {
        self.0.get(index)
    }

    /// add column
    #[must_use]
    pub const fn new(columns: Vec<ColumnDefinition>) -> Self {
        Self(columns)
    }

    /// Ordered field targets, only when every declaration has a resolved type.
    pub(crate) fn field_types(&self) -> Option<Vec<TypeReference>> {
        self.0
            .iter()
            .map(ColumnDefinition::get_type_cloned)
            .collect()
    }

    pub(crate) fn matches_type_structure_with(
        &self,
        other: &Self,
        matches: impl Fn(&TypeReference, &TypeReference) -> bool,
    ) -> bool {
        self.0.len() == other.0.len()
            && self.0.iter().zip(&other.0).all(|(left, right)| {
                match (left.get_type_cloned(), right.get_type_cloned()) {
                    (Some(left), Some(right)) => matches(&left, &right),
                    _ => false,
                }
            })
    }

    /// Whether the collection contains no columns.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Named columns in declaration order, preserving duplicate names.
    #[must_use]
    pub fn names(&self) -> Vec<ColumnName> {
        self.0
            .iter()
            .filter_map(|column| column.name().clone())
            .collect()
    }
}

#[cfg(test)]
mod column_collection_tests {
    use crate::{
        names::ColumnName,
        sem::create_table::{ColumnCollection, ColumnDefinition},
    };

    #[test]
    fn names_keep_order_and_duplicates_without_unnamed_columns() {
        let b = ColumnName::from("b");
        let a = ColumnName::from("a");
        let columns = ColumnCollection::new(vec![
            ColumnDefinition::new(&Some(b.clone()), None, None, &[]),
            ColumnDefinition::new(&None, None, None, &[]),
            ColumnDefinition::new(&Some(a.clone()), None, None, &[]),
            ColumnDefinition::new(&Some(b.clone()), None, None, &[]),
        ]);
        assert_eq!(columns.names(), vec![b.clone(), a, b]);
        assert!(ColumnCollection::default().names().is_empty());
    }
}

impl ColumnCollection {
    pub(crate) fn scalar_function_output(name: &ColumnName, ty: &TypeReference) -> Self {
        Self(vec![ColumnDefinition::new(
            &Some(name.clone()),
            Some(ty),
            None,
            &[],
        )])
    }

    pub(crate) fn row_type_columns(&self) -> Self {
        Self(
            self.0
                .iter()
                .map(|column| {
                    ColumnDefinition::new(
                        column.name(),
                        column.get_type_cloned().as_ref(),
                        None,
                        &[],
                    )
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod test_table_row_returns_do_not_inherit_storage_constraints {
    use crate::{
        sem::PgBuiltInType,
        test_helpers::query::{output_type, table_function_query},
    };

    #[test]
    fn table_row_returns_do_not_inherit_storage_constraints() {
        use crate::{
            context::BaseParseContext,
            names::{ColumnName, FullName, TableName},
        };
        let (query, context) = table_function_query("SELECT r.a FROM row_fun() r", false);
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(output_type(&query, "a"), Some(PgBuiltInType::int4()));
        let column = ColumnName::from("a");
        assert_eq!(
            query.get_result_column_def(&column).unwrap().is_not_null(),
            None
        );
        let table = context
            .get_table(&TableName::from(FullName::with_schema(
                "public", "row_item",
            )))
            .unwrap();
        assert_eq!(
            table.get_column_def(&column).unwrap().is_not_null(),
            Some(true)
        );
    }
}

impl ColumnCollection {
    /// find mutateble column by name
    pub fn get_column_mut(&mut self, column_name: &ColumnName) -> Option<&mut ColumnDefinition> {
        self.0
            .iter_mut()
            .find(|c| c.name().as_ref() == Some(column_name))
    }

    /// get column definition by name
    #[must_use]
    pub fn get_column_def(&self, column_name: &ColumnName) -> Option<&ColumnDefinition> {
        self.0
            .iter()
            .find(|c| c.name().as_ref() == Some(column_name))
    }

    /// test column existing
    #[must_use]
    pub fn has_column(&self, column: &ColumnName) -> bool {
        self.0.iter().any(|c| c.name().as_ref() == Some(column))
    }

    /// get column definition by index
    #[must_use]
    pub fn get_at(&self, column_index: usize) -> Option<&ColumnDefinition> {
        self.0.get(column_index)
    }

    /// get column definition with skip count
    #[must_use]
    pub fn get_column_def_with_skip_count(
        &self,
        column_name: &ColumnName,
        skip_count: usize,
    ) -> Option<&ColumnDefinition> {
        self.0[skip_count..]
            .iter()
            .find(|c| c.name().as_ref() == Some(column_name))
    }

    fn apply_inline_primary_key_nullability(&mut self) {
        for column in &mut self.0 {
            column.apply_primary_key_nullability();
        }
    }

    /// append column definition
    pub fn push(&mut self, column: &ColumnDefinition) {
        self.0.push(column.clone());
    }
}

/// `create table` SQL statement
#[derive(Debug, Clone)]
pub struct CreateTable {
    serial_sequences: Vec<CreateSequence>,
    name: TableName,
    columns: ColumnCollection,
    triggers: TriggerReferences,
    rules: RuleReferences,
    alter_statements: Vec<AlterTable>,
    drop_statements: Vec<DropTable>,
    owner: Option<OwnerSpecification>,
    cluster_on: Option<String>,
    row_security_enabled: bool,
    row_security_forced: bool,
    constraints: HashMap<String, Constraint>,
    mode: CreateTableMode,
    temporary: bool,
    source: SourcePathWithSpan,
}

#[cfg(test)]
mod test_create_table_clone {
    use crate::{
        context::{BaseParseContext, TableInfoWrite},
        names::ColumnName,
        sem::ParseContext,
        test_helpers::{TestParseContext, schema_layer, table_clone as test_helpers},
    };
    use std::path::Path;

    #[test]
    fn clone_and_catalog_insertion_preserve_primary_key_columns() -> testresult::TestResult {
        let table = test_helpers::table_with_primary_key()?;
        let id = ColumnName::from("id");
        assert_eq!(table.primary_key_columns(), vec![&id]);

        let cloned = table.clone();
        assert_eq!(cloned.primary_key_columns(), vec![&id]);

        let mut context = schema_layer(
            Path::new("schema.sql"),
            &TestParseContext::default().freeze(),
        );
        context.insert_create_table(cloned.name().full_name(), &cloned);
        let inserted = context.get_table(cloned.name()).expect("inserted table");
        assert_eq!(inserted.primary_key_columns(), vec![&id]);
        Ok(())
    }
}

impl GetSourcePathWithSpan for CreateTable {
    fn get_source_path_with_span(&self) -> &SourcePathWithSpan {
        &self.source
    }
}

impl CreateTable {
    pub(crate) fn for_alter_analysis(&self, target: &TableName) -> Self {
        let mut table = self.clone();
        table.name = target.clone();
        table
    }

    pub(crate) fn record_added_sequences(&mut self, sequences: &[CreateSequence]) {
        self.serial_sequences.extend_from_slice(sequences);
    }

    pub(crate) fn apply_added_column(
        &mut self,
        column: &ColumnDefinition,
        constraints: &[Constraint],
    ) -> Result<(), AnalysisError> {
        self.append_declared_column(column);
        for constraint in constraints {
            self.record_declared_constraint(constraint)?;
        }
        Ok(())
    }

    pub(crate) fn reserve_constraint_names(&self, names: &mut ConstraintNames) {
        for constraint in self.constraints.values() {
            names.reserve(constraint.name());
        }
        for column in &self.columns.0 {
            column.reserve_check_names(names);
        }
    }

    pub(crate) fn append_declared_column(&mut self, column: &ColumnDefinition) {
        self.columns.0.push(column.clone());
    }

    pub(crate) fn has_non_generated_column_after_drop(
        &self,
        context: &impl ParseContext,
        change: &ColumnDrop,
    ) -> bool {
        self.columns.0.iter().any(|column| {
            !column.has_generation()
                && column
                    .name()
                    .as_ref()
                    .is_some_and(|name| !change.matches_name(context, name))
        })
    }
    pub(crate) fn stage_column_removal(&mut self, column: &ColumnName) {
        self.columns
            .0
            .retain(|definition| definition.name().as_ref() != Some(column));
    }
    pub(crate) const fn declared_columns(&self) -> &ColumnCollection {
        &self.columns
    }

    pub(crate) fn has_constraint_name(&self, name: &str) -> bool {
        self.constraint(name).is_some()
            || self
                .columns
                .0
                .iter()
                .any(|column| column.has_check_name(name))
    }

    pub(crate) fn replace_declared_column(&mut self, column: &ColumnDefinition) {
        let name = column.name().as_ref().expect("declared column name");
        *self.columns.get_column_mut(name).expect("declared column") = column.clone();
    }

    pub(crate) fn find_column(&self, name: &ColumnName, sqlite: bool) -> Option<&ColumnDefinition> {
        self.columns.find_column(name, sqlite)
    }
    pub(crate) fn rename_column_dependencies(
        &mut self,
        rename: &ColumnRename,
        owned: bool,
    ) -> Result<(), AnalysisError> {
        for column in &mut self.columns.0 {
            column.rename_column_dependencies(rename, owned)?;
        }
        for constraint in self.constraints.values_mut() {
            constraint.rename_column(rename, owned)?;
        }
        Ok(())
    }

    pub(crate) fn retain_serial_sequences(&mut self, sequences: Vec<CreateSequence>) {
        self.serial_sequences = sequences;
    }

    pub(crate) fn permits_serial_creation(
        &self,
        context: &mut impl ParseContext,
        indexes: &[CreateIndex],
    ) -> Result<bool, AnalysisError> {
        for sequence in &self.serial_sequences {
            if !context.permits_relation_creation(
                sequence.name(),
                Kind::sequence(),
                false,
                false,
            )? {
                return Ok(false);
            }
            if CreateIndex::contains_name(indexes, sequence.name()) {
                context.report_problem(AnalysisProblem::creation(CreationProblem::relation(
                    sequence.name(),
                    Kind::index(),
                    Kind::sequence(),
                    self.source.span(),
                )))?;
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(crate) fn serial_sequences(&self) -> &[CreateSequence] {
        &self.serial_sequences
    }

    /// Whether analyzed column and foreign-key declarations permit catalog application.
    pub(crate) fn declarations_can_apply(&self) -> bool {
        self.columns.declarations_can_apply()
            && self
                .constraints
                .values()
                .all(Constraint::references_can_apply)
    }

    pub(crate) fn rename_expressions(
        &mut self,
        old: &TableName,
        new: &TableName,
    ) -> Result<(), AnalysisError> {
        let mut visitor = RelationRenamer::new(old, new);
        self.columns.rename_expressions(&mut visitor)?;
        for constraint in self.constraints.values_mut() {
            constraint.walk_check(&mut visitor)?;
        }
        Ok(())
    }

    /// Applied ALTER statements in source order, retaining their original bindings and spans.
    pub fn alter_statements(&self) -> &[AlterTable] {
        &self.alter_statements
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.columns.rename_relation(old, new);
        self.rules.rename_relation(old, new);
        for constraint in self.constraints.values_mut() {
            constraint.rename_relation(old, new);
        }
    }

    fn rename_identity(&mut self, new: &TableName) {
        let old = self.name.clone();
        self.rename_relation(&old, new);
        self.name = new.clone();
    }

    /// Rule links owned by this relation; resolve against the effective context.
    pub const fn rules(&self) -> &RuleReferences {
        &self.rules
    }
    pub(crate) fn associate_rule(&mut self, identity: &RuleIdentity) {
        self.rules.associate(identity);
    }
    pub(crate) fn dissociate_rule(&mut self, identity: &RuleIdentity) {
        self.rules.dissociate(identity);
    }

    /// Retained ownership; None means no ownership information was supplied.
    pub const fn owner(&self) -> Option<&OwnerSpecification> {
        self.owner.as_ref()
    }
    /// Commit an ownership change, leaving the rest of the definition intact.
    pub(crate) fn change_owner(&mut self, owner: &OwnerSpecification) {
        self.owner = Some(owner.clone());
    }

    /// Fully qualified trigger references associated with this relation.
    pub const fn triggers(&self) -> &TriggerReferences {
        &self.triggers
    }
    pub(crate) fn associate_trigger(&mut self, name: &FullName) {
        self.triggers.associate(name);
    }
    pub(crate) fn dissociate_trigger(&mut self, name: &FullName) {
        self.triggers.dissociate(name);
    }

    /// new instance with column definitions
    #[must_use]
    pub fn new(
        name: &TableName,
        columns: Vec<ColumnDefinition>,
        mode: CreateTableMode,
        temporary: bool,
        source: &SourcePathWithSpan,
    ) -> Self {
        Self {
            name: name.clone(),
            serial_sequences: Vec::new(),
            triggers: TriggerReferences::default(),
            rules: RuleReferences::default(),
            columns: ColumnCollection(columns),
            alter_statements: Default::default(),
            drop_statements: Default::default(),
            owner: Default::default(),
            cluster_on: Default::default(),
            row_security_enabled: false,
            row_security_forced: false,
            constraints: Default::default(),
            mode,
            temporary,
            source: source.clone(),
        }
    }

    pub(crate) fn declared_indexes(
        &self,
        name: &FullName,
    ) -> Result<Vec<CreateIndex>, AnalysisError> {
        let mut indexes = Vec::new();
        for constraint in self.constraints.values() {
            if !constraint.name().is_empty() {
                for change in constraint.implicit_changes()? {
                    indexes.push(change.into_index().with_table_identity(name));
                }
            }
        }
        Ok(indexes)
    }

    /// get table name
    #[must_use]
    pub const fn name(&self) -> &TableName {
        &self.name
    }

    /// get [`CreateTableMode`]
    pub const fn mode(&self) -> &CreateTableMode {
        &self.mode
    }

    /// is temporary table
    pub const fn temporary(&self) -> bool {
        self.temporary
    }

    /// get column definition by column name
    #[must_use]
    pub fn get_column_def(&self, column_name: &ColumnName) -> Option<&ColumnDefinition> {
        self.columns.get_column_def(column_name)
    }

    /// Look up a column definition by *original* column name, ignoring the first `skip_count` columns.
    ///
    /// This is used to implement `PostgreSQL`'s behavior for a table alias with a column alias list:
    ///
    /// ```sql
    /// SELECT ... FROM users AS u(id, name);
    /// ```
    ///
    /// When a column alias list is present, the original names of the first N columns (N = the length
    /// of the alias list) are not visible through that alias. Only the alias names are visible for
    /// those columns. Columns after the first N keep their original names.
    ///
    /// For example, if `users` has at least two columns and the 1st column is named `user_id`,
    /// then `u.user_id` is not accessible when `skip_count >= 1` (e.g. `u(id, name)` makes `skip_count = 2`).
    /// However, columns after the first `skip_count` columns are still accessible by their original names.
    ///
    #[must_use]
    pub fn get_column_def_with_skip_count(
        &self,
        column_name: &ColumnName,
        skip_count: usize,
    ) -> Option<&ColumnDefinition> {
        self.columns
            .get_column_def_with_skip_count(column_name, skip_count)
    }

    pub(crate) fn function_row_columns(&self) -> ColumnCollection {
        // A function returns the row type, without the table's storage constraints.
        self.columns.row_type_columns()
    }

    /// get column definition by index
    #[must_use]
    pub fn get_column_at(&self, column_index: usize) -> Option<&ColumnDefinition> {
        self.columns.get_at(column_index)
    }

    pub(crate) fn column_names(&self) -> Vec<ColumnName> {
        self.columns.names()
    }

    /// check column existing
    #[must_use]
    pub fn has_column(&self, column: &ColumnName) -> bool {
        self.columns.has_column(column)
    }

    pub(crate) fn apply_column_attributes(
        &mut self,
        column: &ColumnName,
        action: &ColumnAttributeAction,
    ) {
        if let Some(column) = self.columns.get_column_mut(column) {
            action.apply(&self.name, column);
        }
    }

    /// Whether `PostgreSQL` row-level security is enabled on this table.
    pub const fn row_security_enabled(&self) -> bool {
        self.row_security_enabled
    }

    /// Whether the table owner is subject to row-level security when enabled.
    /// This flag remains independent of whether row-level security is enabled.
    pub const fn row_security_forced(&self) -> bool {
        self.row_security_forced
    }

    /// apply `alter table` statement
    pub fn apply_alter(
        &mut self,
        alter_table: &AlterTable,
    ) -> Result<Vec<ImplicitChange>, AnalysisError> {
        if !alter_table.can_apply() {
            return Ok(Vec::new());
        }
        self.alter_statements.push(alter_table.clone());
        let mut implicit_changes = Vec::new();
        for cmd in alter_table.commands() {
            match cmd {
                AlterObjSubCommand::RowSecurity(change) => change.action().apply(
                    &mut self.row_security_enabled,
                    &mut self.row_security_forced,
                ),
                AlterObjSubCommand::ColumnDefinition(_) | AlterObjSubCommand::ColumnType(_) => {
                    todo!("ALTER COLUMN TYPE catalog application (#1057)")
                }
                AlterObjSubCommand::ColumnAttributes(_) => cmd.apply_column_attributes(self),
                AlterObjSubCommand::ChangeOwner(new_owner) => {
                    if new_owner.can_apply() {
                        self.change_owner(new_owner);
                    }
                }
                AlterObjSubCommand::SetColumnDefault(column_name, sem_scalar_expr) => {
                    if let Some(column) = self.columns.get_column_mut(column_name) {
                        column.set_default(sem_scalar_expr);
                    }
                }
                AlterObjSubCommand::DropColumnDefault(_) => todo!(),
                AlterObjSubCommand::AddConstraint(constraint) => {
                    if alter_table.addition_indexes().is_some() {
                        self.record_declared_constraint(constraint)?;
                    } else {
                        implicit_changes.extend(self.apply_added_constraint(constraint)?);
                    }
                }
                AlterObjSubCommand::ClusterOn(index_name) => {
                    self.cluster_on = Some(index_name.clone())
                }
                AlterObjSubCommand::RenameTo(name) => self.rename_identity(name),
                AlterObjSubCommand::AddColumn(addition) => {
                    addition.apply_to(self)?;
                }
                AlterObjSubCommand::AlterColumn(_, _) => {
                    todo!()
                }
                AlterObjSubCommand::RenameColumn(rename) => {
                    if rename.can_apply() {
                        self.columns.rename_column(rename);
                    }
                }
                AlterObjSubCommand::DropColumn(_) => {
                    todo!("DROP COLUMN catalog application (#1076)")
                }
            }
        }

        if alter_table.addition_indexes().is_some() {
            self.apply_primary_key_nullability();
        }
        Ok(implicit_changes)
    }

    pub(crate) fn apply_primary_key_nullability(&mut self) {
        self.columns.apply_inline_primary_key_nullability();
        let keys: Vec<_> = self.primary_key_columns().into_iter().cloned().collect();
        for key in keys {
            if let Some(column) = self.columns.get_column_mut(&key) {
                column.set_not_null(Some(true));
            }
        }
    }

    fn apply_added_constraint(
        &mut self,
        constraint: &Constraint,
    ) -> Result<Vec<ImplicitChange>, AnalysisError> {
        if constraint.as_check().is_some() {
            self.record_declared_constraint(constraint)?;
        } else {
            self.constraints
                .insert(constraint.name().to_owned(), constraint.clone());
        }
        constraint.implicit_changes()
    }

    pub(crate) fn record_declared_constraint(
        &mut self,
        constraint: &Constraint,
    ) -> Result<(), AnalysisError> {
        let key = if constraint.name().is_empty() {
            // An internal map slot identifies an unnamed declaration; it is never
            // exposed as a fabricated SQL constraint/index name.
            format!("<unnamed:{}>", self.constraints.len())
        } else {
            constraint.name().to_owned()
        };
        let key = if self.constraints.contains_key(&key)
            && constraint
                .as_check()
                .is_some_and(|check| !check.can_apply())
        {
            format!("<rejected-check:{}>", self.constraints.len())
        } else {
            key
        };
        if self.constraints.contains_key(&key) {
            return Err(AnalysisError::unexpected_input_error(
                "duplicate constraint name",
            ));
        }
        self.constraints.insert(key, constraint.clone());
        Ok(())
    }
}

#[cfg(test)]
mod test_create_result_and_catalog_retain_composite_and_unnamed_foreign_keys {
    use crate::{
        context::TableInfoRead,
        names::ColumnName,
        test_helpers::{foreign_keys as fk, ownership::apply_ownership_sql},
    };

    #[test]
    fn create_result_and_catalog_retain_composite_and_unnamed_foreign_keys()
    -> testresult::TestResult {
        let context = fk::apply(
            fk::context(),
            "CREATE TABLE a.parent(x integer, y integer, PRIMARY KEY(x,y), UNIQUE(y,x))",
        );
        let pair = apply_ownership_sql(
            context,
            "CREATE TABLE a.child(x integer, y integer, CONSTRAINT fk FOREIGN KEY(y,x) REFERENCES parent(y,x), FOREIGN KEY(x,y) REFERENCES parent, FOREIGN KEY(y,x) REFERENCES parent(y,x))",
        )?;
        let result = pair.ast().as_create_table().unwrap();
        assert_eq!(result.foreign_keys().count(), 3);
        let constraint = result.constraint("fk").unwrap().as_foreign_key().unwrap();
        assert_eq!(
            constraint.source_columns(),
            &[ColumnName::from("y"), ColumnName::from("x")]
        );
        assert_eq!(
            constraint.target_columns(),
            Some([ColumnName::from("y"), ColumnName::from("x")].as_slice())
        );
        let context = pair.into_context();
        let child = context
            .get_table_impl(fk::name("a", "child").full_name())
            .unwrap();
        assert_eq!(child.foreign_keys().count(), 3);
        assert_eq!(
            child
                .foreign_keys()
                .filter(|key| key.name().is_empty())
                .count(),
            2
        );
        assert!(child.foreign_keys().any(|key| key.target_columns()
            == Some([ColumnName::from("x"), ColumnName::from("y")].as_slice())));
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        Ok(())
    }
}

impl CreateTable {
    /// Inspect analyzed table CHECK constraints, including unnamed and rejected definitions.
    pub fn checks(&self) -> impl Iterator<Item = &CheckConstraint> {
        self.constraints.values().filter_map(Constraint::as_check)
    }

    /// Inspect retained table-level foreign keys, including unnamed declarations.
    ///
    /// Iteration order is unspecified; column order within each key is retained.
    pub fn foreign_keys(&self) -> impl Iterator<Item = &ForeignKeyConstraint> {
        self.constraints
            .values()
            .filter_map(Constraint::as_foreign_key)
    }

    /// Look up an explicitly named table constraint.
    pub fn constraint(&self, name: &str) -> Option<&Constraint> {
        self.constraints
            .get(name)
            .filter(|constraint| constraint.name() == name)
    }

    pub(crate) fn infer_insert_arbiters(
        &self,
        destination: &TableName,
        columns: &[ColumnName],
    ) -> Vec<InsertArbiter> {
        let mut arbiters = Vec::new();
        for constraint in self.constraints.values() {
            if constraint.matches_unique_columns(columns) {
                arbiters.push(InsertArbiter::table_constraint(destination, constraint));
            }
        }
        // Named inline keys also occur in the table constraint collection.
        // Use a column identity only when no table key already identifies it.
        if arbiters.is_empty()
            && columns.len() == 1
            && let Some(column) = self.columns.get_column_def(&columns[0])
            && column.has_unique_key()
        {
            arbiters.push(InsertArbiter::column_constraint(&columns[0]));
        }
        arbiters
    }

    /// get primary key column names
    #[must_use]
    pub fn primary_key_columns(&self) -> Vec<&ColumnName> {
        if let Some(pk) = self
            .constraints
            .iter()
            .find_map(|(_, c)| c.as_primary_key_constraint())
        {
            pk.keys().iter().collect()
        } else {
            self.columns.inline_primary_key_columns()
        }
    }
}

#[cfg(test)]
mod test_primary_key_columns {
    use crate::{context::TableInfoRead, names::ColumnName, test_helpers::foreign_keys as fk};

    #[rstest::rstest]
    #[case::unnamed_inline("id integer PRIMARY KEY", vec![ColumnName::from("id")])]
    #[case::named_inline("id integer CONSTRAINT pk PRIMARY KEY", vec![ColumnName::from("id")])]
    #[case::unnamed_table("id integer, PRIMARY KEY(id)", vec![ColumnName::from("id")])]
    #[case::named_table("id integer, CONSTRAINT pk PRIMARY KEY(id)", vec![ColumnName::from("id")])]
    #[case::composite("id integer, other integer, PRIMARY KEY(other, id)", vec![ColumnName::from("other"), ColumnName::from("id")])]
    #[case::absent("id integer", Vec::<ColumnName>::new())]
    fn primary_key_columns_cover_inline_and_table_declarations(
        #[case] definition: &str,
        #[case] expected: Vec<ColumnName>,
    ) {
        let context = fk::apply(
            fk::context(),
            &format!("CREATE TABLE a.parent({definition})"),
        );
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let table = context
            .get_table_impl(fk::name("a", "parent").full_name())
            .unwrap();
        let actual: Vec<_> = table.primary_key_columns().into_iter().cloned().collect();
        assert_eq!(actual, expected);
    }
}

impl CreateTable {
    pub(crate) fn push_drop_statement(&mut self, drop_table: &DropTable) {
        self.drop_statements.push(drop_table.clone());
    }
}

/// `DROP TABLE` statement
#[derive(Debug, Clone)]
pub struct DropTable {
    if_exists: bool,
    table_names: Vec<TableName>,
    cascade: bool,
}

impl DropTable {
    /// Create a `DROP TABLE` statement.
    #[must_use]
    pub fn new(table_names: &[TableName], if_exists: bool, cascade: bool) -> Self {
        Self {
            if_exists,
            table_names: table_names.to_vec(),
            cascade,
        }
    }

    /// True when `IF EXISTS` or same meaning specified.
    pub const fn if_exists(&self) -> bool {
        self.if_exists
    }

    /// get target table name.
    pub fn table_names(&self) -> &[TableName] {
        &self.table_names
    }

    /// get cascade
    pub const fn cascade(&self) -> bool {
        self.cascade
    }

    /// get restrict
    pub const fn restrict(&self) -> bool {
        !self.cascade
    }
}

impl CreateTable {
    pub(crate) fn resolve_reference_columns(
        &self,
        context: &mut impl ParseContext,
        requested: &TableName,
        explicit: Option<&[ColumnName]>,
    ) -> Result<(Vec<ColumnName>, bool), AnalysisError> {
        let columns = if let Some(columns) = explicit {
            columns.to_vec()
        } else {
            self.primary_key_columns().into_iter().cloned().collect()
        };
        // SQLite checks parent-key validity when preparing DML, not CREATE TABLE.
        let valid = if context.dialect() == Dialect::postgres() {
            if explicit.is_none() && columns.is_empty() {
                context.report_analysis_problem(
                    AnalysisProblem::foreign_key_referenced_primary_key_missing(requested),
                )?;
                return Ok((columns, false));
            }
            self.check_reference_columns(context, requested, &columns)?
        } else {
            true
        };
        Ok((columns, valid))
    }

    pub(crate) fn check_reference_columns(
        &self,
        context: &mut impl ParseContext,
        requested: &TableName,
        columns: &[ColumnName],
    ) -> Result<bool, AnalysisError> {
        let mut valid = true;
        for column in columns {
            if self
                .find_column(column, context.dialect() == Dialect::sqlite())
                .is_none()
            {
                valid = false;
                context.report_analysis_problem(AnalysisProblem::column_not_found_in_table(
                    requested, column,
                ))?;
            }
        }
        Ok(valid)
    }
}

#[cfg(test)]
mod test_foreign_key_reference_columns {
    use crate::{
        context::TableInfoRead,
        test_helpers::{foreign_keys as fk, ownership::apply_ownership_sql},
    };

    #[test]
    fn omitted_reference_columns_require_a_postgres_primary_key() -> testresult::TestResult {
        let context = fk::apply(fk::context(), "CREATE TABLE a.parent(id integer)");
        let pair = apply_ownership_sql(
            context,
            "CREATE TABLE a.child(parent_id integer, FOREIGN KEY(parent_id) REFERENCES parent)",
        )?;
        let context = pair.into_context();
        assert_eq!(
            context.problems().len(),
            1,
            "missing primary key must produce a structured diagnostic"
        );
        assert_eq!(
            context.problems()[0].code(),
            "foreign_key_referenced_primary_key_missing"
        );
        assert!(
            context
                .get_table_impl(fk::name("a", "child").full_name())
                .is_none()
        );
        Ok(())
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_foreign_key_source_columns_ignore_ascii_case() -> testresult::TestResult {
        let context = fk::apply_sqlite(fk::sqlite_context(), "CREATE TABLE parent(id PRIMARY KEY)");
        let sql =
            r#"CREATE TABLE child("ID", CONSTRAINT fk FOREIGN KEY("id") REFERENCES parent(id))"#;
        let context = fk::apply_sqlite(context, sql);
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let child = context
            .get_table_impl(fk::name("main", "child").full_name())
            .unwrap();
        assert!(
            child
                .constraint("fk")
                .unwrap()
                .as_foreign_key()
                .unwrap()
                .references_can_apply()
        );
        let oracle = ::rusqlite::Connection::open_in_memory()?;
        oracle.execute_batch("CREATE TABLE parent(id PRIMARY KEY)")?;
        oracle.execute_batch(sql)?;
        Ok(())
    }
}

impl CreateTable {
    pub(crate) fn collect_drop_dependencies(
        &self,
        context: &impl ParseContext,
        change: &mut ColumnDrop,
        owner: &FullName,
    ) -> Result<(), AnalysisError> {
        let owned = owner == change.table().full_name();
        for (identity, constraint) in &self.constraints {
            constraint.collect_drop_dependencies(context, change, owner, identity, owned)?;
        }
        for column in &self.columns.0 {
            column.collect_drop_dependencies(context, change, owner, owned)?;
        }
        Ok(())
    }
}
