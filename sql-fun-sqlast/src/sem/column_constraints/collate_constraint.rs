use crate::{
    names::{ColumnName, TableName},
    sem::{
        AnalysisError, AnalysisProblem, ColumnConstraint, ColumnConstraintKind,
        ColumnConstraintTrait, ColumnRename, ParseContext,
    },
};

/// collate constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct CollateColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    collation_name: String,
}

impl CollateColumnConstraint {
    pub(crate) fn rename_column(&mut self, rename: &ColumnRename, owned: bool) {
        if owned {
            rename.rename_name(&mut self.column_name);
        }
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.table_name.rename_reference(old, new);
    }

    pub(crate) fn is_known_name(name: &str) -> bool {
        ["BINARY", "NOCASE", "RTRIM"].contains(&name.to_uppercase().as_str())
    }
    /// create instance
    pub fn new(table_name: &TableName, column_name: &ColumnName, collation_name: &str) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            collation_name: String::from(collation_name),
        }
    }

    /// get collation name
    pub fn collation_name(&self) -> &str {
        &self.collation_name
    }
}

impl ColumnConstraintTrait for CollateColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::Collate
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl ColumnConstraint {
    /// create Collate variant
    pub const fn collate(value: CollateColumnConstraint) -> Self {
        Self::Collate(value)
    }

    /// get Collate variant
    pub const fn as_collate(&self) -> Option<&CollateColumnConstraint> {
        if let Self::Collate(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

// Analysis owned by the semantic definitions in this module.

impl CollateColumnConstraint {
    pub(in crate::sem::column_constraints) fn from_ast_name(
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        name: &str,
    ) -> Result<Self, AnalysisError> {
        if !Self::is_known_name(name) {
            context.report_analysis_problem(AnalysisProblem::collation_name_not_found(name))?;
        }
        Ok(Self::new(table, column, name))
    }
}
