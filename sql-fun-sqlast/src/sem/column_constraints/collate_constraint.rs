#[cfg(feature = "sqlite")]
use sql_fun_core::IVec;

use crate::sem::{
    ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ColumnName, TableName,
};
#[cfg(feature = "sqlite")]
use crate::{
    SqliteToken,
    sem::{AnalysisError, AnalysisProblem, ParseContext},
};

/// collate constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct CollateColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    collation_name: String,
}

impl CollateColumnConstraint {
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

#[cfg(feature = "sqlite")]
impl CollateColumnConstraint {
    /// analyze [`crate::syn::sqlite::CollateColumnConstraint`]
    pub fn analyze_constraint_sqlite<TParseContext>(
        mut context: TParseContext,
        table_name: &TableName,
        column_name: &ColumnName,
        syn: &crate::syn::sqlite::CollateColumnConstraint,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(ColumnConstraint, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let collation_name = syn.collation_name().value();
        if !Self::is_known_name(collation_name) {
            context.report_analysis_problem(AnalysisProblem::collation_name_not_found(
                collation_name,
            ))?;
        }
        Ok((
            ColumnConstraint::collate(Self::new(table_name, column_name, collation_name)),
            context,
        ))
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
