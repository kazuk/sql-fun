use crate::{
    ast::{Deferrability, ForeignKeyAction, ForeignKeyMatchType, Validity},
    context::ParseContext,
    names::{ColumnName, TableName},
    sem::{
        AnalysisError, AnalysisProblem, ColumnRename, CreateTable, ForeignKeyTarget, ImplicitChange,
    },
};

#[derive(Debug, Clone)]
pub struct ForeignKeyConstraint {
    name: String,
    source_table: Option<TableName>,
    target_table: ForeignKeyTarget,
    source_columns: Vec<ColumnName>,
    target_columns: Option<Vec<ColumnName>>,
    match_type: Option<ForeignKeyMatchType>,
    delete_action: Option<ForeignKeyAction>,
    update_action: Option<ForeignKeyAction>,
    deferrability: Option<Deferrability>,
    validity: Option<Validity>,
    delete_columns: Vec<ColumnName>,
    references_valid: bool,
}

impl ForeignKeyConstraint {
    pub(crate) fn validate_added_column_types(
        &mut self,
        context: &mut impl ParseContext,
        pending: &CreateTable,
    ) -> Result<(), AnalysisError> {
        let Some(target) = self.target_table.relation() else {
            return Ok(());
        };
        let Some(columns) = &self.target_columns else {
            return Ok(());
        };
        let referenced = if target == pending.name() {
            Some(pending)
        } else {
            context.get_table_impl(target.full_name())
        };
        let mut mismatches = Vec::new();
        for (source_name, target_name) in self.source_columns.iter().zip(columns) {
            let source = if let Some(column) = pending.find_column(source_name, false) {
                column.get_type()
            } else {
                None
            };
            let target_type = if let Some(table) = referenced
                && let Some(column) = table.find_column(target_name, false)
            {
                column.get_type()
            } else {
                None
            };
            if let (Some(source), Some(target_type)) = (source, target_type)
                && source
                    .implicit_match_context(target_type, context)
                    .is_none()
                && target_type
                    .implicit_match_context(source, context)
                    .is_none()
            {
                // The shared mismatch diagnostic is unimplemented (#1067; parent #523).
                mismatches.push(AnalysisProblem::foreign_key_column_type_mismatch(
                    pending.name(),
                    source_name,
                    source,
                    target,
                    target_name,
                    target_type,
                ));
            }
        }
        self.references_valid &= mismatches.is_empty();
        for problem in mismatches {
            context.report_problem(problem)?;
        }
        Ok(())
    }

    /// Referencing columns in declaration order.
    pub fn source_columns(&self) -> &[ColumnName] {
        &self.source_columns
    }
    /// Referenced columns; omitted primary-key columns are expanded when resolved.
    pub fn target_columns(&self) -> Option<&[ColumnName]> {
        self.target_columns.as_deref()
    }

    pub(crate) fn with_attributes(
        mut self,
        match_type: Option<&ForeignKeyMatchType>,
        delete_action: Option<&ForeignKeyAction>,
        update_action: Option<&ForeignKeyAction>,
        deferrability: Option<&Deferrability>,
        validity: Option<&Validity>,
    ) -> Self {
        self.match_type = match_type.cloned();
        self.delete_action = delete_action.cloned();
        self.update_action = update_action.cloned();
        self.deferrability = deferrability.cloned();
        self.validity = validity.cloned();
        self
    }

    /// Declared foreign-key match semantics.
    pub const fn match_type(&self) -> Option<&ForeignKeyMatchType> {
        self.match_type.as_ref()
    }

    /// Declared action when a referenced row is deleted.
    pub const fn delete_action(&self) -> Option<&ForeignKeyAction> {
        self.delete_action.as_ref()
    }

    /// Declared action when a referenced key is updated.
    pub const fn update_action(&self) -> Option<&ForeignKeyAction> {
        self.update_action.as_ref()
    }

    /// Declared constraint timing.
    pub const fn deferrability(&self) -> Option<&Deferrability> {
        self.deferrability.as_ref()
    }

    /// Declared constraint validation state.
    pub const fn validity(&self) -> Option<&Validity> {
        self.validity.as_ref()
    }

    /// Referencing columns selected by ON DELETE SET NULL/DEFAULT, in declaration order.
    /// An empty list means that no column subset was specified.
    pub fn delete_columns(&self) -> &[ColumnName] {
        &self.delete_columns
    }

    /// Whether the analyzed references allow this CREATE TABLE constraint to be applied.
    pub const fn references_can_apply(&self) -> bool {
        self.references_valid
    }

    pub(crate) const fn restrict_reference_application(&mut self, valid: bool) {
        self.references_valid &= valid;
    }

    pub(crate) fn set_delete_columns(&mut self, columns: &[ColumnName]) {
        self.delete_columns = columns.to_vec();
    }

    pub(crate) fn rename_column(&mut self, rename: &ColumnRename, owned: bool) {
        if owned {
            rename.rename_names(&mut self.source_columns);
            rename.rename_names(&mut self.delete_columns);
        }
        if self
            .target_table
            .relation()
            .is_some_and(|table| rename.matches_table(table))
            && let Some(columns) = &mut self.target_columns
        {
            rename.rename_names(columns);
        }
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        if let Some(table) = &mut self.source_table {
            table.rename_reference(old, new);
        }
        self.target_table.rename_relation(old, new);
    }

    pub(crate) fn new(
        name: &str,
        source_table: Option<&TableName>,
        target_table: Option<&TableName>,
        source_columns: &[ColumnName],
        target_columns: &Option<Vec<ColumnName>>,
    ) -> Self {
        Self {
            name: name.to_owned(),
            source_table: source_table.cloned(),
            target_table: ForeignKeyTarget::new(target_table),
            source_columns: source_columns.to_vec(),
            target_columns: target_columns.clone(),
            match_type: None,
            delete_action: None,
            update_action: None,
            deferrability: None,
            validity: None,
            delete_columns: Vec::new(),
            references_valid: true,
        }
    }
    /// Canonical referenced relation, or `None` after failed target lookup.
    ///
    /// This binding is retained independently of later search-path changes.
    pub const fn target_table(&self) -> Option<&TableName> {
        self.target_table.relation()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn implicit_changes(&self) -> Result<Vec<ImplicitChange>, AnalysisError> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod test_foreign_key_attributes_and_dependencies {
    use crate::{
        context::{ParseContext, TableInfoRead},
        names::ColumnName,
        test_helpers::foreign_keys as fk,
    };

    #[rstest::rstest]
    #[case::create(fk::Declaration::create())]
    #[case::alter(fk::Declaration::alter())]
    fn table_foreign_key_attributes_survive_catalog_reads(#[case] declaration: fk::Declaration) {
        use crate::ast::{Deferrability, Validity};
        let context = declaration.bind(
            fk::parents(),
            "child",
            "parent(id) MATCH FULL ON DELETE CASCADE ON UPDATE SET NULL DEFERRABLE INITIALLY DEFERRED",
        );
        let table = context
            .get_table_impl(fk::name("a", "child").full_name())
            .unwrap();
        let constraint = table.constraint("fk").unwrap().as_foreign_key().unwrap();
        assert_eq!(constraint.source_columns(), &[ColumnName::from("id")]);
        assert_eq!(
            constraint.target_columns(),
            Some([ColumnName::from("id")].as_slice())
        );
        assert_eq!(
            ::serde_json::to_value(constraint.match_type()).unwrap(),
            "Full"
        );
        assert_eq!(
            ::serde_json::to_value(constraint.delete_action()).unwrap(),
            "Cascade"
        );
        assert_eq!(
            ::serde_json::to_value(constraint.update_action()).unwrap(),
            "SetNull"
        );
        assert_eq!(
            constraint.deferrability(),
            Some(&Deferrability::new(true, true))
        );
        assert_eq!(constraint.validity(), Some(&Validity::new(false, true)));
        assert!(context.problems().is_empty(), "{:?}", context.problems());
    }

    #[test]
    fn create_foreign_key_column_renames_update_owned_and_incoming_columns() {
        let context = fk::Declaration::create().bind(fk::parents(), "child", "parent(id)");
        let parent = context.freeze();
        let context = fk::apply(
            fk::child(&parent),
            "ALTER TABLE a.parent RENAME COLUMN id TO renamed",
        );
        let context = fk::apply(context, "ALTER TABLE a.child RENAME COLUMN id TO parent_id");
        let child = context
            .get_table_impl(fk::name("a", "child").full_name())
            .unwrap();
        let constraint = child.constraint("fk").unwrap().as_foreign_key().unwrap();
        assert_eq!(
            constraint.source_columns(),
            &[ColumnName::from("parent_id")]
        );
        assert_eq!(
            constraint.target_columns(),
            Some([ColumnName::from("renamed")].as_slice())
        );
        let original = parent
            .get_table_impl(fk::name("a", "child").full_name())
            .unwrap();
        assert_eq!(
            original
                .constraint("fk")
                .unwrap()
                .as_foreign_key()
                .unwrap()
                .source_columns(),
            &[ColumnName::from("id")]
        );
        let context = fk::apply(context, "SELECT parent_id FROM a.child");
        assert!(context.problems().is_empty(), "{:?}", context.problems());
    }

    #[test]
    fn delete_column_subset_survives_create_and_rename() {
        let context = fk::apply(
            fk::parents(),
            "CREATE TABLE a.child(id integer, extra integer, CONSTRAINT fk FOREIGN KEY(id) REFERENCES parent(id) ON DELETE SET NULL (id))",
        );
        let context = fk::apply(context, "ALTER TABLE a.child RENAME COLUMN id TO parent_id");
        let child = context
            .get_table_impl(fk::name("a", "child").full_name())
            .unwrap();
        let constraint = child.constraint("fk").unwrap().as_foreign_key().unwrap();
        assert_eq!(
            constraint.delete_columns(),
            &[ColumnName::from("parent_id")]
        );
        assert!(context.problems().is_empty(), "{:?}", context.problems());
    }
}
