use crate::{
    names::{ColumnName, TableName},
    sem::{
        AnalysisError, ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait,
        ConflictClause, ParseContext, Validity,
    },
    syn::{ConstrType, Opt},
};

/// `NULL` / `NOT NULL` constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct ColumnNullabilityConstraint {
    table_name: TableName,
    column_name: ColumnName,
    is_not_null: bool,
    conflict_clause: Option<ConflictClause>,
    validity: Option<Validity>,
}

impl ColumnNullabilityConstraint {
    pub(crate) fn rename_column(&mut self, rename: &crate::sem::ColumnRename, owned: bool) {
        if owned {
            rename.rename_name(&mut self.column_name);
        }
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.table_name.rename_reference(old, new);
    }

    /// create instance
    pub fn new(
        table_name: &TableName,
        column_name: &ColumnName,
        is_not_null: bool,
        conflict_clause: Option<ConflictClause>,
        validity: Option<Validity>,
    ) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            is_not_null,
            conflict_clause,
            validity,
        }
    }

    /// get nullability
    pub const fn is_not_null(&self) -> bool {
        self.is_not_null
    }
}

impl ColumnConstraintTrait for ColumnNullabilityConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::Nullability
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl ColumnNullabilityConstraint {
    fn is_not_null_constraint(contype: ConstrType) -> Result<bool, AnalysisError> {
        let not_null = if matches!(contype, ConstrType::ConstrNotnull) {
            true
        } else if matches!(contype, ConstrType::ConstrNull) {
            false
        } else {
            AnalysisError::raise_unexpected_input("constraint.contype")?;
            AnalysisError::never()
        };
        Ok(not_null)
    }
}

impl ColumnNullabilityConstraint {
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

        let not_null = Self::is_not_null_constraint(contype)?;
        Ok((
            Self::new(
                table_name,
                column_name,
                not_null,
                None,
                Some(Validity::from_constraint(syn)),
            ),
            context,
        ))
    }
}

#[cfg(feature = "sqlite")]
impl ColumnNullabilityConstraint {
    /// analyze [`crate::syn::sqlite::NotNullColumnConstraint`]
    pub fn analyze_constraint_sqlite<TParseContext>(
        context: TParseContext,
        table_name: &TableName,
        column_name: &ColumnName,
        syn: &crate::syn::sqlite::NotNullColumnConstraint,
    ) -> Result<(ColumnConstraint, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Ok((
            ColumnConstraint::nullability(Self::new(
                table_name,
                column_name,
                !syn.nullable(),
                syn.conflict_clause().map(ConflictClause::from),
                None,
            )),
            context,
        ))
    }
}

impl ColumnConstraint {
    /// create Nullability variant
    pub const fn nullability(value: ColumnNullabilityConstraint) -> Self {
        Self::Nullability(value)
    }

    /// get Nullability variant
    pub const fn as_nullability(&self) -> Option<&ColumnNullabilityConstraint> {
        if let Self::Nullability(c) = self {
            Some(c)
        } else {
            None
        }
    }
}
