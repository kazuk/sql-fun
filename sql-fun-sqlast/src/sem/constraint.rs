mod foreign;
mod primary;
mod unique;

use crate::{
    sem::{AnalysisError, ParseContext, TableName, alter_table::AlterObjSubCommand},
    syn::{ConstrType, Opt},
};

use super::ImplicitChange;

pub use self::{
    foreign::ForeignKeyConstraint, primary::PrimaryKeyConstraint, unique::UniqueConstraint,
};

/// analyze [`crate::syn::Constraint`] variants
pub trait AnalizeConstraint<TParseContext>
where
    TParseContext: ParseContext,
{
    /// analyze [`crate::syn::Constraint`] variants
    fn analyze(
        context: &mut TParseContext,
        target_table: Option<&TableName>,
        constraint: crate::syn::Constraint,
    ) -> Result<Constraint, AnalysisError>;
}

/// analyzed [`crate::syn::Constraint`]
#[derive(Debug, Clone)]
pub enum Constraint {
    /// primary key constraint
    PrimaryKey(PrimaryKeyConstraint),
    /// unique constraint
    Unique(UniqueConstraint),
    /// foreign key constraint
    Foreign(ForeignKeyConstraint),
}

impl Constraint {
    /// get constraint name
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::PrimaryKey(pk) => pk.name(),
            Self::Unique(u) => u.name(),
            Self::Foreign(f) => f.name(),
        }
    }

    /// get constraint derive index definition
    pub fn implicit_changes(&self) -> Result<Vec<ImplicitChange>, AnalysisError> {
        match self {
            Self::PrimaryKey(pk) => pk.implicit_changes(),
            Self::Unique(u) => u.implicit_changes(),
            Self::Foreign(f) => f.implicit_changes(),
        }
    }

    /// return true if constraint is primary key
    #[must_use]
    pub fn is_primary_key_constraint(&self) -> bool {
        matches!(self, Self::PrimaryKey(_))
    }

    /// get primary key reference
    #[must_use]
    pub fn as_primary_key_constraint(&self) -> Option<&PrimaryKeyConstraint> {
        match self {
            Self::PrimaryKey(p) => Some(p),
            _ => None,
        }
    }
}

impl Constraint {
    /// analyze [`crate::syn::AlterTableCmd`]
    pub fn analyze_alter_obj_add_constraint<TParseContext>(
        context: &mut TParseContext,
        target_table: &TableName,
        cmd: crate::syn::AlterTableCmd,
    ) -> Result<AlterObjSubCommand, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let _constraint_name = cmd.get_name();
        let Some(constraint) = cmd.get_def().as_constraint().as_inner() else {
            AnalysisError::raise_unexpected_none("alter_table_cmd.def as constraint")?
        };
        let constraint = Self::analyze(context, Some(target_table), constraint)?;
        Ok(AlterObjSubCommand::AddConstraint(constraint))
    }
}

#[cfg(test)]
mod test_analyze_alter_obj_add_constraint {
    use testresult::TestResult;

    use crate::{
        sem::{Constraint, CreateTable, FullName, TableName},
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

    #[rstest::rstest]
    fn test_analyze_alter_obj_add_constraint(mut test_context: TestParseContext) -> TestResult {
        let builder = SynBuilder::new();
        let constraint = builder.constraint_primary_key();
        let alter_table_cmd = builder.alter_table_cmd_add_constraint(constraint);
        let table_name = TableName::from(FullName::with_schema("public", "table1"));
        let create_table = CreateTable::new(table_name.clone(), vec![]);
        test_context.set_get_table_impl_result(table_name.full_name(), &Some(create_table));
        let _result = Constraint::analyze_alter_obj_add_constraint(
            &mut test_context,
            &table_name,
            alter_table_cmd,
        )?;
        Ok(())
    }
}

impl<TParseContext> AnalizeConstraint<TParseContext> for Constraint
where
    TParseContext: ParseContext,
{
    fn analyze(
        context: &mut TParseContext,
        target_table: Option<&TableName>,
        constraint: crate::syn::Constraint,
    ) -> Result<Constraint, AnalysisError> {
        let Some(constraint_type) = constraint.get_contype().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.contype")?
        };

        match constraint_type {
            ConstrType::ConstrNull => todo!(),
            ConstrType::ConstrNotnull => todo!(),
            ConstrType::ConstrDefault => todo!(),
            ConstrType::ConstrIdentity => todo!(),
            ConstrType::ConstrGenerated => todo!(),
            ConstrType::ConstrCheck => todo!(),
            ConstrType::ConstrPrimary => {
                PrimaryKeyConstraint::analyze(context, target_table, constraint)
            }
            ConstrType::ConstrUnique => {
                UniqueConstraint::analyze(context, target_table, constraint)
            }
            ConstrType::ConstrExclusion => todo!(),
            ConstrType::ConstrForeign => {
                ForeignKeyConstraint::analyze(context, target_table, constraint)
            }
            ConstrType::ConstrAttrDeferrable => todo!(),
            ConstrType::ConstrAttrNotDeferrable => todo!(),
            ConstrType::ConstrAttrDeferred => todo!(),
            ConstrType::ConstrAttrImmediate => todo!(),
            ConstrType::OptionNone => todo!(),
            ConstrType::Undefined => todo!(),
        }
    }
}
