use crate::{
    sem::{
        AnalysisError, AnalysisProblem, AnalyzeConstraint, ColumnName, Constraint,
        ForeignKeyTarget, ImplicitChange, ParseContext, TableName,
    },
    syn::{ListOpt, Opt},
};

#[derive(Debug, Clone)]
pub struct ForeignKeyConstraint {
    name: String,
    source_table: Option<TableName>,
    target_table: ForeignKeyTarget,
    source_columns: Vec<ColumnName>,
    target_columns: Option<Vec<ColumnName>>,
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

    pub(crate) fn rename_column(&mut self, rename: &crate::sem::ColumnRename, owned: bool) {
        if owned {
            rename.rename_names(&mut self.source_columns);
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

impl<TParseContext> AnalyzeConstraint<TParseContext> for ForeignKeyConstraint
where
    TParseContext: ParseContext,
{
    fn analyze(
        context: &mut TParseContext,
        source_table: Option<&TableName>,
        constraint: crate::syn::Constraint,
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
        let target_columns =
            target_table.referenced_columns(context, &requested, explicit_columns.as_deref())?;

        let source_columns: Vec<_> = source_columns.into_iter().map(ColumnName::from).collect();
        if let Some(table_name) = source_table
            && let Some(table) = context.get_table(table_name).cloned()
        {
            for key_col_name in &source_columns {
                if !table.has_column(key_col_name) {
                    context.report_analysis_problem(AnalysisProblem::column_not_found_in_table(
                        table_name,
                        key_col_name,
                    ))?;
                }
            }
        }

        Ok(Constraint::foreign(&Self::new(
            &name,
            source_table,
            target_table.relation(),
            &source_columns,
            &target_columns,
        )))
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
            Some(vec![crate::sem::ColumnName::from("id")])
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
            Some(vec![crate::sem::ColumnName::from("id")])
        );
        assert_eq!(constraint.target_table(), Some(&fk::name("a", "parent")));
        assert!(context.problems().is_empty());
    }
}
