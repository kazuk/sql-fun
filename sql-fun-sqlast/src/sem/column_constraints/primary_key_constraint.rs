use crate::{
    sem::{
        AnalysisError, ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ColumnName,
        ConflictClause, Deferrable, ParseContext, TableName,
    },
    syn::{ConstrType, Opt},
};

/// primary key constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct PrimaryKeyColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    conflict_clause: Option<ConflictClause>,
    deferrable: Option<Deferrable>,
}

impl PrimaryKeyColumnConstraint {
    /// create instance
    pub fn new(
        table_name: &TableName,
        column_name: &ColumnName,
        conflict_clause: Option<ConflictClause>,
        deferrable: Option<Deferrable>,
    ) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            conflict_clause,
            deferrable,
        }
    }
}

impl ColumnConstraintTrait for PrimaryKeyColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::PrimaryKey
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl PrimaryKeyColumnConstraint {
    /// analyze [`crate::syn::Constraint`]
    pub fn analyze_constraint<TParseContext>(
        context: TParseContext,
        table_name: &TableName,
        column_name: &ColumnName,
        syn: &crate::syn::Constraint,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(contype) = syn.get_contype().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.contype")?;
            AnalysisError::never()
        };
        if !matches!(contype, ConstrType::ConstrPrimary) {
            AnalysisError::raise_unexpected_input("contype not primary key")?;
            AnalysisError::never()
        }

        Ok((
            Self::new(
                table_name,
                column_name,
                None,
                Deferrable::try_from_constraint(syn)?,
            ),
            context,
        ))
    }
}

impl ColumnConstraint {
    /// create PrimaryKey variant
    pub fn primary_key(value: PrimaryKeyColumnConstraint) -> Self {
        Self::PrimaryKey(value)
    }

    /// get PrimaryKey variant
    pub fn as_primary_key(&self) -> Option<&PrimaryKeyColumnConstraint> {
        if let Self::PrimaryKey(c) = self {
            Some(c)
        } else {
            None
        }
    }
}
