use crate::{
    sem::{
        AnalysisError, AnalysisProblem, AnalyzeConstraint, ColumnName, Constraint, ImplicitChange,
        ParseContext, TableName,
    },
    syn::{ListOpt, Opt},
};

#[derive(Debug, Clone)]
pub struct ForeignKeyConstraint {
    name: String,
    #[expect(dead_code)]
    source_table: Option<TableName>,
    #[expect(dead_code)]
    target_table: TableName,
    #[expect(dead_code)]
    source_columns: Vec<ColumnName>,
    #[expect(dead_code)]
    target_columns: Option<Vec<ColumnName>>,
}

impl ForeignKeyConstraint {
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
            context.report_problem(AnalysisProblem::relation_not_found(s))?;
        }
        let target_table = TableName::try_from(target_table)?;
        if context.get_table(&target_table).is_none() {
            context.report_problem(AnalysisProblem::relation_not_found(&target_table))?;
        }

        let target_columns = if let Some(target_columns) = constraint
            .get_pk_attrs()
            .map_values(|v| v.as_string().get_sval())
        {
            let target_columns: Vec<_> = target_columns.into_iter().map(ColumnName::from).collect();
            Some(target_columns)
        } else {
            context
                .get_table(&target_table)
                .map(|table| table.primary_key_columns().into_iter().cloned().collect())
        };

        let source_columns: Vec<_> = source_columns.into_iter().map(ColumnName::from).collect();
        if let Some(table_name) = source_table
            && let Some(table) = context.get_table(table_name).cloned()
        {
            for key_col_name in &source_columns {
                if !table.has_column(key_col_name) {
                    context.report_problem(AnalysisProblem::column_not_found_in_table(
                        table_name,
                        key_col_name,
                    ))?;
                }
            }
        }

        if let Some(table) = context.get_table(&target_table).cloned()
            && let Some(cols) = &target_columns
        {
            for key_col_name in cols {
                if !table.has_column(key_col_name) {
                    context.report_problem(AnalysisProblem::column_not_found_in_table(
                        &target_table,
                        key_col_name,
                    ))?;
                }
            }
        }

        Ok(Constraint::Foreign(Self {
            name,
            source_table: source_table.cloned(),
            target_table,
            source_columns,
            target_columns,
        }))
    }
}
