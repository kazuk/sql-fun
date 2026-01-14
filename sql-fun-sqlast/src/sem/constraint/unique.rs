use sql_fun_core::IVec;

use crate::{
    sem::{
        AnalysisError, AnalysisProblem, ColumnName, Constraint, CreateIndex, ImplicitChange,
        ParseContext, TableName, create_index::IndexElementCollection,
    },
    syn::ListOpt,
};

#[derive(Debug, Clone)]
pub struct UniqueConstraint {
    name: String,
    table: Option<TableName>,
    keys: IVec<ColumnName>,
}

impl UniqueConstraint {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn implicit_changes(&self) -> Result<Vec<ImplicitChange>, AnalysisError> {
        let Some(table) = &self.table else {
            AnalysisError::raise_unexpected_none("constraint.table")?
        };
        let key_columns = IndexElementCollection::from_column_names(&self.keys);

        Ok(vec![ImplicitChange::from(CreateIndex::new(
            &self.name,
            table,
            &key_columns,
        ))])
    }
}

impl<TParseContext> super::AnalizeConstraint<TParseContext> for UniqueConstraint
where
    TParseContext: ParseContext,
{
    fn analyze(
        context: &mut TParseContext,
        target_table: Option<&TableName>,
        constraint: crate::syn::Constraint,
    ) -> Result<Constraint, AnalysisError> {
        let name = constraint.get_conname();
        let Some(keys) = constraint
            .get_keys()
            .map_values(|v| v.as_string().get_sval())
        else {
            AnalysisError::raise_unexpected_none("constraint.keys")?
        };
        let keys: Vec<_> = keys.into_iter().map(ColumnName::from).collect();

        if let Some(table_name) = target_table {
            if let Some(table) = context.get_table(table_name).cloned() {
                for key_col_name in &keys {
                    if !table.has_column(key_col_name) {
                        context.report_problem(AnalysisProblem::column_not_found_in_table(
                            table_name,
                            key_col_name,
                        ))?;
                    }
                }
            } else {
                context.report_problem(AnalysisProblem::relation_not_found(table_name))?;
            }
        }
        Ok(Constraint::Unique(Self {
            name,
            table: target_table.cloned(),
            keys: keys.into(),
        }))
    }
}
