#[cfg(feature = "sqlite")]
use crate::syn::sqlite::PrimaryKeyColumnConstraint as SqlitePrimaryKeyColumnConstraint;
use crate::{
    ast::{ConflictClause, Deferrable},
    names::{ColumnName, TableName},
    sem::{
        AnalysisError, ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ColumnRename,
        ParseContext,
    },
    syn::{ConstrType, Constraint, Opt},
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
    pub(crate) fn rename_column(&mut self, rename: &ColumnRename, owned: bool) {
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
        syn: &Constraint,
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

#[cfg(feature = "sqlite")]
impl PrimaryKeyColumnConstraint {
    /// analyze [`crate::syn::sqlite::PrimaryKeyColumnConstraint`]
    pub fn analyze_constraint_sqlite<TParseContext>(
        context: TParseContext,
        table_name: &TableName,
        column_name: &ColumnName,
        syn: &SqlitePrimaryKeyColumnConstraint,
    ) -> Result<(ColumnConstraint, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let constraint = Self::new(
            table_name,
            column_name,
            syn.conflict_clause().map(ConflictClause::from),
            None,
        );
        Ok((ColumnConstraint::primary_key(constraint), context))
    }
}

impl ColumnConstraint {
    /// create PrimaryKey variant
    pub const fn primary_key(value: PrimaryKeyColumnConstraint) -> Self {
        Self::PrimaryKey(value)
    }

    /// get PrimaryKey variant
    pub const fn as_primary_key(&self) -> Option<&PrimaryKeyColumnConstraint> {
        if let Self::PrimaryKey(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

// Analysis owned by the semantic definitions in this module.

impl PrimaryKeyColumnConstraint {
    pub(in crate::sem::column_constraints) fn from_ast_parts(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        conflict: &Option<ConflictClause>,
        deferrable: &Option<Deferrable>,
    ) -> Self {
        Self::new(table, column, conflict.clone(), deferrable.clone())
    }
}
