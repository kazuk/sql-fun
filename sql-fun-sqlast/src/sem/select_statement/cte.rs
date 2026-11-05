use crate::{
    names::ColumnName,
    sem::{ColumnDefinition, SelectStatement},
};
impl SelectStatement {
    pub(crate) fn resolved_column(
        column: &ColumnName,
        index: Option<usize>,
        skip: usize,
        query: &Self,
    ) -> Option<ColumnDefinition> {
        let columns = query.result_columns();
        let result = if let Some(index) = index {
            columns.get_column_at(index)
        } else {
            columns.get_column_with_skip_count(column, skip)
        };
        result.map(|column| column.create_column_definition())
    }
}
