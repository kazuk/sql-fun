mod ddl;
use crate::{
    names::ColumnName,
    sem::{
        AnalysisError, ColumnDefinition, ColumnRename, ResultColumnCollection, SelectStatement,
        SemScalarExpr,
    },
};

/// Resolved origin of each View output column, in projection order.
/// This is semantic lineage, independent of operation-specific write capabilities.
#[derive(Debug, Clone)]
pub struct ViewColumnLineage(Vec<Option<ColumnName>>);
impl ViewColumnLineage {
    pub(crate) fn rename_column(
        &mut self,
        rename: &ColumnRename,
        query: &SelectStatement,
    ) -> Result<(), AnalysisError> {
        for (index, lineage) in self.0.iter_mut().enumerate() {
            if let Some(lineage) = lineage
                && let Some(column) = query.result_columns().get_column_at(index)
                && let Some(expression) = column.expression()
            {
                let mut finder = ColumnOrigin {
                    rename,
                    found: false,
                };
                expression.clone().walk(&mut finder)?;
                if finder.found {
                    rename.rename_name(lineage);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn new(columns: &[Option<ColumnName>]) -> Self {
        Self(columns.to_vec())
    }
    pub(crate) fn from_query(query: &SelectStatement) -> Self {
        Self(
            query
                .result_columns()
                .view_projection()
                .into_iter()
                .map(|(_, source)| source)
                .collect(),
        )
    }
    /// Proven source column for this output position; expression projections have no direct origin.
    pub fn source_column(&self, index: usize) -> Option<&ColumnName> {
        self.0.get(index)?.as_ref()
    }
    pub(crate) fn project(
        &self,
        columns: &ResultColumnCollection,
    ) -> Vec<(ColumnDefinition, Option<ColumnName>)> {
        columns
            .view_projection()
            .into_iter()
            .enumerate()
            .map(|(index, (column, _))| (column, self.source_column(index).cloned()))
            .collect()
    }
}

/// A resolved option of an ordinary View definition.
#[derive(Debug, Clone)]
pub struct ViewOption {
    namespace: Option<String>,
    name: String,
    value: Option<SemScalarExpr>,
}
impl ViewOption {
    pub(crate) fn new(namespace: Option<&str>, name: &str, value: Option<&SemScalarExpr>) -> Self {
        Self {
            namespace: namespace.map(str::to_owned),
            name: name.to_owned(),
            value: value.cloned(),
        }
    }
    /// Optional qualified option namespace.
    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }
    /// Option name as declared.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Resolved option value, absent for a valueless option.
    pub const fn value(&self) -> Option<&SemScalarExpr> {
        self.value.as_ref()
    }
}
/// CHECK OPTION declaration retained by the View definition.
#[derive(Debug, Clone, Copy, Default)]
pub enum ViewCheckOption {
    /// No CHECK OPTION was supplied.
    #[default]
    None,
    /// LOCAL CHECK OPTION.
    Local,
    /// CASCADED CHECK OPTION.
    Cascaded,
}
impl ViewCheckOption {
    /// No declared check option.
    pub const fn none() -> Self {
        Self::None
    }
    /// LOCAL option.
    pub const fn local() -> Self {
        Self::Local
    }
    /// CASCADED option.
    pub const fn cascaded() -> Self {
        Self::Cascaded
    }
}

struct ColumnOrigin<'a> {
    rename: &'a ColumnRename,
    found: bool,
}
impl crate::sem::traversal::SemanticVisitorMut for ColumnOrigin<'_> {
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        self.found |= expression.matches_column(self.rename);
        Ok(())
    }
}
