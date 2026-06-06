use crate::{
    sem::{
        AnalysisError, ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ColumnName,
        ConflictClause, ParseContext, TableName,
    },
    syn::{ConstrType, Opt},
};

/// unique value constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct UniqueColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    conflict_clause: Option<ConflictClause>,
}

impl UniqueColumnConstraint {
    /// create instance
    pub fn new(
        table_name: &TableName,
        column_name: &ColumnName,
        conflict_clause: Option<ConflictClause>,
    ) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            conflict_clause,
        }
    }
}

impl ColumnConstraintTrait for UniqueColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::Unique
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl UniqueColumnConstraint {
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
        if !matches!(contype, ConstrType::ConstrUnique) {
            AnalysisError::raise_unexpected_input("constraint.contype")?;
            AnalysisError::never()
        }

        Ok((Self::new(table_name, column_name, None), context))
    }
}

#[cfg(feature = "sqlite")]
impl UniqueColumnConstraint {
    /// analyze [`crate::syn::sqlite::UniqueColumnConstraint`]
    pub fn analyze_constraint_sqlite<TParseContext>(
        context: TParseContext,
        table_name: &TableName,
        column_name: &ColumnName,
        syn: &crate::syn::sqlite::UniqueColumnConstraint,
    ) -> Result<(ColumnConstraint, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Ok((
            ColumnConstraint::unique(Self::new(
                table_name,
                column_name,
                syn.conflict_clause().map(ConflictClause::from),
            )),
            context,
        ))
    }
}

impl ColumnConstraint {
    /// create Unique variant
    pub const fn unique(value: UniqueColumnConstraint) -> Self {
        Self::Unique(value)
    }

    /// get Unique variant
    pub const fn as_unique(&self) -> Option<&UniqueColumnConstraint> {
        if let Self::Unique(c) = self {
            Some(c)
        } else {
            None
        }
    }
}
