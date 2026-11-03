use crate::{
    names::{ColumnName, TableName},
    sem::{
        AnalysisError, AnalysisProblem, AnalyzeConstraint, ColumnRename, Constraint, Deferrability,
        ForeignKeyAction, ForeignKeyMatchType, ForeignKeyTarget, ImplicitChange, ParseContext,
        Validity,
    },
    syn::{Constraint as SynConstraint, ListOpt, Opt},
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
        use crate::sem::{Deferrability, Validity};
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

impl ForeignKeyConstraint {
    fn retain_postgres_attributes(
        &mut self,
        constraint: &SynConstraint,
    ) -> Result<(), AnalysisError> {
        self.match_type = ForeignKeyMatchType::try_from_postgres_constraint(constraint)?;
        self.delete_action = ForeignKeyAction::try_from_delete_postgres_constraint(constraint)?;
        self.update_action = ForeignKeyAction::try_from_update_postgres_constraint(constraint)?;
        self.deferrability = Some(Deferrability::new(
            constraint.get_deferrable(),
            constraint.get_initdeferred(),
        ));
        self.validity = Some(Validity::from_constraint(constraint));
        self.delete_columns = constraint
            .get_fk_del_set_cols()
            .map_values(|value| value.as_string().get_sval())
            .unwrap_or_default()
            .into_iter()
            .map(ColumnName::from)
            .collect();
        Ok(())
    }
}

impl<TParseContext> AnalyzeConstraint<TParseContext> for ForeignKeyConstraint
where
    TParseContext: ParseContext,
{
    fn analyze(
        context: &mut TParseContext,
        source_table: Option<&TableName>,
        constraint: SynConstraint,
    ) -> Result<Constraint, AnalysisError> {
        let name = constraint.get_conname();
        let Some(source_columns) = constraint
            .get_fk_attrs()
            .map_values(|v| v.as_string().get_sval())
        else {
            AnalysisError::raise_unexpected_none("constraint.fk_attrs")?
        };

        let Some(target_table) = constraint.get_pktable().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.pktable")?
        };

        // check table existing
        if let Some(s) = source_table
            && context.get_table(s).is_none()
        {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(s))?;
        }
        let requested = TableName::try_from(target_table)?;
        let target_table = ForeignKeyTarget::resolve_relation(context, &requested)?;
        let explicit_columns = constraint
            .get_pk_attrs()
            .map_values(|v| v.as_string().get_sval())
            .map(|columns| {
                columns
                    .into_iter()
                    .map(ColumnName::from)
                    .collect::<Vec<_>>()
            });
        let (target_columns, target_valid) =
            target_table.referenced_columns(context, &requested, explicit_columns.as_deref())?;

        let source_columns: Vec<_> = source_columns.into_iter().map(ColumnName::from).collect();
        let source_valid = if let Some(table_name) = source_table
            && let Some(table) = context.get_table(table_name).cloned()
        {
            table.check_reference_columns(context, table_name, &source_columns)?
        } else {
            source_table.is_none()
        };
        let mut result = Self::new(
            &name,
            source_table,
            target_table.relation(),
            &source_columns,
            &target_columns,
        );
        result.retain_postgres_attributes(&constraint)?;
        result.restrict_reference_application(source_valid && target_valid);
        Ok(Constraint::foreign(&result))
    }
}

#[cfg(test)]
mod test_analyze {
    use crate::{
        sem::{AnalyzeConstraint, ForeignKeyConstraint},
        test_helpers::foreign_keys as fk,
    };

    #[rstest::rstest]
    #[case::explicit("parent(id)")]
    fn direct_analysis_retains_the_matched_catalog_key(
        #[case] reference: &str,
    ) -> testresult::TestResult {
        let mut context = fk::apply(fk::parents(), "CREATE TABLE a.child(id integer)");
        let syntax = fk::postgres_constraint(&format!(
            "ALTER TABLE a.child ADD FOREIGN KEY(id) REFERENCES {reference}"
        ));
        let result =
            ForeignKeyConstraint::analyze(&mut context, Some(&fk::name("a", "child")), syntax)?;
        let result = result.as_foreign_key().unwrap();
        assert_eq!(result.target_table(), Some(&fk::name("a", "parent")));
        assert_eq!(
            result.target_columns,
            Some(vec![crate::names::ColumnName::from("id")])
        );
        fk::reversed_path(&mut context);
        assert_eq!(result.target_table(), Some(&fk::name("a", "parent")));
        assert!(context.problems().is_empty());
        Ok(())
    }
    #[test]
    fn common_ast_expands_omitted_columns_from_the_bound_parent() {
        use crate::context::TableInfoRead;
        let context = fk::bind(fk::parents(), "child", "parent", true);
        let table = context
            .get_table_impl(fk::name("a", "child").full_name())
            .unwrap();
        let constraint = table.constraint("fk").unwrap().as_foreign_key().unwrap();
        assert_eq!(
            constraint.target_columns,
            Some(vec![crate::names::ColumnName::from("id")])
        );
        assert_eq!(constraint.target_table(), Some(&fk::name("a", "parent")));
        assert!(context.problems().is_empty());
    }
}
