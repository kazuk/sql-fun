mod ddl;

pub use self::{
    check::CheckConstraint, foreign::ForeignKeyConstraint, primary::PrimaryKeyConstraint,
    unique::UniqueConstraint,
};
use crate::{
    ast::{Column as AstColumn, ColumnConstraint as AstColumnConstraint},
    names::{ColumnName, TableName},
    sem::{
        AnalysisError, ColumnRename, ImplicitChange, ParseContext, traversal::SemanticVisitorMut,
    },
};
use std::collections::HashMap;

mod check;
mod foreign;
mod primary;
mod unique;

/// Analyzed table constraint.
#[derive(Debug, Clone)]
pub enum Constraint {
    /// Table CHECK expression and applicability.
    Check(CheckConstraint),
    /// primary key constraint
    PrimaryKey(PrimaryKeyConstraint),
    /// unique constraint
    Unique(UniqueConstraint),
    /// foreign key constraint
    Foreign(ForeignKeyConstraint),
}

impl Constraint {
    /// Inspect a table CHECK, including a rejected definition's retained expression.
    pub const fn as_check(&self) -> Option<&CheckConstraint> {
        if let Self::Check(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub(crate) fn walk_check(
        &mut self,
        visitor: &mut impl SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        if let Self::Check(value) = self {
            value.walk(visitor)?;
        }
        Ok(())
    }
}

impl Constraint {
    pub(crate) const fn references_can_apply(&self) -> bool {
        if let Self::Check(value) = self {
            value.can_apply()
        } else if let Self::Foreign(value) = self {
            value.references_can_apply()
        } else {
            true
        }
    }

    pub(crate) const fn restrict_reference_application(&mut self, valid: bool) {
        if let Self::Foreign(value) = self {
            value.restrict_reference_application(valid);
        }
    }

    pub(crate) fn set_foreign_key_delete_columns(&mut self, columns: &[ColumnName]) {
        if let Self::Foreign(value) = self {
            value.set_delete_columns(columns);
        }
    }

    pub(crate) fn rename_column(
        &mut self,
        rename: &ColumnRename,
        owned: bool,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Check(value) => value.rename_column(rename)?,
            Self::PrimaryKey(value) => value.rename_column(rename, owned),
            Self::Unique(value) => value.rename_column(rename, owned),
            Self::Foreign(value) => value.rename_column(rename, owned),
        }
        Ok(())
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        match self {
            Self::Check(value) => value.rename_relation(old, new),
            Self::PrimaryKey(value) => value.rename_relation(old, new),
            Self::Unique(value) => value.rename_relation(old, new),
            Self::Foreign(value) => value.rename_relation(old, new),
        }
    }

    pub(crate) fn unique_key_columns(&self) -> Vec<ColumnName> {
        match self {
            Self::PrimaryKey(key) => key.keys().to_vec(),
            Self::Unique(key) => key.key_columns(),
            Self::Foreign(_) | Self::Check(_) => Vec::new(),
        }
    }

    pub(crate) fn matches_unique_columns(&self, columns: &[ColumnName]) -> bool {
        match self {
            Self::PrimaryKey(key) => {
                key.keys().len() == columns.len()
                    && key.keys().iter().all(|name| columns.contains(name))
            }
            Self::Unique(key) => key.matches_columns(columns),
            Self::Foreign(_) | Self::Check(_) => false,
        }
    }

    pub(crate) fn primary_key(value: &PrimaryKeyConstraint) -> Self {
        Self::PrimaryKey(value.clone())
    }
    pub(crate) fn unique(value: &UniqueConstraint) -> Self {
        Self::Unique(value.clone())
    }
    pub(crate) fn foreign(value: &ForeignKeyConstraint) -> Self {
        Self::Foreign(value.clone())
    }
    /// Inspect the analyzed foreign-key constraint, including its target binding.
    pub const fn as_foreign_key(&self) -> Option<&ForeignKeyConstraint> {
        if let Self::Foreign(value) = self {
            Some(value)
        } else {
            None
        }
    }

    /// get constraint name
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Check(check) => check.name(),
            Self::PrimaryKey(pk) => pk.name(),
            Self::Unique(u) => u.name(),
            Self::Foreign(f) => f.name(),
        }
    }

    /// get constraint derive index definition
    pub fn implicit_changes(&self) -> Result<Vec<ImplicitChange>, AnalysisError> {
        match self {
            Self::Check(_) => Ok(Vec::new()),
            Self::PrimaryKey(pk) => pk.implicit_changes(),
            Self::Unique(u) => u.implicit_changes(),
            Self::Foreign(f) => f.implicit_changes(),
        }
    }

    /// return true if constraint is primary key
    #[must_use]
    pub const fn is_primary_key_constraint(&self) -> bool {
        matches!(self, Self::PrimaryKey(_))
    }

    /// get primary key reference
    #[must_use]
    pub const fn as_primary_key_constraint(&self) -> Option<&PrimaryKeyConstraint> {
        match self {
            Self::PrimaryKey(p) => Some(p),
            _ => None,
        }
    }
}

// The shared namespace for explicitly named relation constraints.

#[derive(Default)]
pub(crate) struct ConstraintNames {
    occurrences: HashMap<String, usize>,
    first_duplicate: Option<String>,
}

impl ConstraintNames {
    /// Reserve a catalog name. Unnamed constraints never conflict.
    pub(crate) fn reserve(&mut self, name: &str) -> bool {
        if name.is_empty() {
            return true;
        }
        let count = self.occurrences.entry(name.to_owned()).or_default();
        *count += 1;
        if *count > 1 {
            if self.first_duplicate.is_none() {
                self.first_duplicate = Some(name.to_owned());
            }
            return false;
        }
        true
    }

    pub(crate) fn is_duplicate(&self, name: &str) -> bool {
        self.occurrences.get(name).is_some_and(|count| *count > 1)
    }

    pub(crate) fn first_duplicate(&self) -> Option<&str> {
        self.first_duplicate.as_deref()
    }
}

// Analysis owned by the semantic definitions in this module.

impl ConstraintNames {
    pub(crate) fn ast_constraint_has_catalog_name(
        ast: &AstColumnConstraint,
        context: &impl ParseContext,
    ) -> bool {
        match ast {
            AstColumnConstraint::PrimaryKey(..)
            | AstColumnConstraint::Unique(_)
            | AstColumnConstraint::Check(_)
            | AstColumnConstraint::ForeignKey(_)
            | AstColumnConstraint::Exclusion => true,
            AstColumnConstraint::Nullability(true, ..) => Self::supports_named_not_null(context),
            _ => false,
        }
    }

    fn supports_named_not_null(context: &impl ParseContext) -> bool {
        let Some(version) = context.metadata().engine_version() else {
            return false;
        };
        let Ok(version) = version
            .to_string()
            .parse::<::sql_fun_core::PostgresVersion>()
        else {
            return false;
        };
        version.major() >= 18
    }
}

impl ConstraintNames {
    pub(crate) fn reserve_ast_column(
        ast: &AstColumn,
        context: &impl ParseContext,
        names: &mut Self,
    ) {
        for definition in ast.constraints() {
            if !definition.is_check()
                && Self::ast_constraint_has_catalog_name(definition.constraint(), context)
            {
                names.reserve(definition.name().as_deref().unwrap_or_default());
            }
        }
    }
}
