use crate::{sem::ColumnRename, syn::Constraint as SynConstraint};
use sql_fun_core::IVec;

use crate::{
    names::{ColumnName, TableName},
    sem::{
        AnalysisError, AnalysisProblem, Constraint, CreateIndex, ImplicitChange, ParseContext,
        create_index::IndexElementCollection,
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
    pub(crate) fn rename_column(&mut self, rename: &ColumnRename, owned: bool) {
        if owned {
            let mut keys: Vec<_> = self.keys.iter().cloned().collect();
            rename.rename_names(&mut keys);
            self.keys = keys.into();
        }
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        if let Some(table) = &mut self.table {
            table.rename_reference(old, new);
        }
    }

    pub(crate) fn key_columns(&self) -> Vec<ColumnName> {
        self.keys.iter().cloned().collect()
    }

    pub(crate) fn matches_columns(&self, columns: &[ColumnName]) -> bool {
        self.keys.len() == columns.len() && self.keys.iter().all(|name| columns.contains(name))
    }

    pub(crate) fn new(name: &str, table: &TableName, keys: &[ColumnName]) -> Self {
        Self {
            name: name.to_owned(),
            table: Some(table.clone()),
            keys: keys.to_vec().into(),
        }
    }
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn implicit_changes(&self) -> Result<Vec<ImplicitChange>, AnalysisError> {
        let Some(table) = &self.table else {
            AnalysisError::raise_unexpected_none("constraint.table")?
        };
        let key_columns = IndexElementCollection::from_column_names(&self.keys);

        Ok(vec![ImplicitChange::from(CreateIndex::new(
            &crate::names::FullName::new(
                &table.full_name().catalog().cloned(),
                &table.full_name().schema().cloned(),
                &crate::names::LocalName::from(self.name.as_str()),
            ),
            table,
            &key_columns,
        ))])
    }
}

impl<TParseContext> super::AnalyzeConstraint<TParseContext> for UniqueConstraint
where
    TParseContext: ParseContext,
{
    fn analyze(
        context: &mut TParseContext,
        target_table: Option<&TableName>,
        constraint: SynConstraint,
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
                        context.report_analysis_problem(
                            AnalysisProblem::column_not_found_in_table(table_name, key_col_name),
                        )?;
                    }
                }
            } else {
                context.report_analysis_problem(AnalysisProblem::relation_not_found(table_name))?;
            }
        }
        Ok(Constraint::Unique(Self {
            name,
            table: target_table.cloned(),
            keys: keys.into(),
        }))
    }
}
