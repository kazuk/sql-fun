use crate::{
    names::{ColumnName, TableName},
    sem::{
        AnalysisError, ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ColumnRename,
        ParseContext,
    },
    syn::{ConstrType, Constraint, Opt},
};

// FIXME: exclusion constraint applies column?
/// exclusion column constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct ExclusionColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
}

impl ExclusionColumnConstraint {
    pub(crate) fn rename_column(&mut self, rename: &ColumnRename, owned: bool) {
        if owned {
            rename.rename_name(&mut self.column_name);
        }
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.table_name.rename_reference(old, new);
    }

    /// create instance
    pub fn new(table_name: &TableName, column_name: &ColumnName) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
        }
    }
}

impl ColumnConstraintTrait for ExclusionColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::Exclusion
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl ExclusionColumnConstraint {
    /// analyze [`crate::syn::Constraint`]
    pub fn analyze_constraint<TParseContext>(
        context: TParseContext,
        table_name: &TableName,
        column_name: &ColumnName,
        syn: &Constraint,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(contype) = syn.get_contype().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.contype")?;
            AnalysisError::never()
        };
        if !matches!(contype, ConstrType::ConstrExclusion) {
            AnalysisError::raise_unexpected_input("contype not exclusion")?;
            AnalysisError::never()
        }
        Ok((Self::new(table_name, column_name), context))
    }
}

impl ColumnConstraint {
    /// create Exclusion variant
    pub const fn exclusion(value: ExclusionColumnConstraint) -> Self {
        Self::Exclusion(value)
    }

    /// get Exclusion variant
    pub const fn as_exclusion(&self) -> Option<&ExclusionColumnConstraint> {
        if let Self::Exclusion(c) = self {
            Some(c)
        } else {
            None
        }
    }
}
