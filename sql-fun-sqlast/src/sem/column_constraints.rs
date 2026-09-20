mod attr_constraint;
mod check_constraint;
mod collate_constraint;
mod default_constraint;
mod exclusion_constraint;
mod foreign_key_constraint;
mod generated_constraint;
mod identity_constraint;
mod nullability_constraint;
mod primary_key_constraint;
mod unique_constraint;

pub use self::{
    attr_constraint::{
        AttrDeferrabilityColumnConstraint, AttrDeferredColumnConstraint,
        AttrImmediateColumnConstraint,
    },
    check_constraint::CheckColumnConstraint,
    collate_constraint::CollateColumnConstraint,
    default_constraint::DefaultColumnConstraint,
    exclusion_constraint::ExclusionColumnConstraint,
    foreign_key_constraint::{
        Deferrability, ForeignKeyAction, ForeignKeyColumnConstraint, ForeignKeyMatchType,
    },
    generated_constraint::{GeneratedColumnConstraint, GeneratedColumnMode},
    identity_constraint::{IdentityColumnConstraint, IdentityColumnMode},
    nullability_constraint::ColumnNullabilityConstraint,
    primary_key_constraint::PrimaryKeyColumnConstraint,
    unique_constraint::UniqueColumnConstraint,
};

use sql_fun_core::IVec;

use crate::{
    sem::{AnalysisError, ColumnName, ParseContext, TableName},
    syn::{ConstrType, Opt, ScanToken},
};

/// common behavior for column constraints
pub trait ColumnConstraintTrait {
    /// get kind of constraint
    fn kind(&self) -> ColumnConstraintKind;

    /// applied table name
    fn table_name(&self) -> &TableName;

    /// applied column name
    fn column_name(&self) -> &ColumnName;
}

/// column constraints
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub enum ColumnConstraint {
    /// not null column constraint
    Nullability(ColumnNullabilityConstraint),
    /// default column constraint
    Default(DefaultColumnConstraint),
    /// identity column constraint
    Identity(IdentityColumnConstraint),
    /// generated column constraint
    Generated(GeneratedColumnConstraint),
    /// check column constraint
    Check(CheckColumnConstraint),
    /// primary key column constraint
    PrimaryKey(PrimaryKeyColumnConstraint),
    /// unique column constraint
    Unique(UniqueColumnConstraint),
    /// exclusion column constraint
    Exclusion(ExclusionColumnConstraint),
    /// foreign key column constraint
    ForeignKey(ForeignKeyColumnConstraint),
    /// attr deferrability
    AttrDeferrability(AttrDeferrabilityColumnConstraint),
    /// attr Deferred
    AttrDeferred(AttrDeferredColumnConstraint),
    /// attr Immediate
    AttrImmediate(AttrImmediateColumnConstraint),
    /// collate column constraint
    Collate(CollateColumnConstraint),
}

/// kind of [`ColumnConstraint`]
#[derive(Debug, PartialEq)]
pub enum ColumnConstraintKind {
    /// `NULL` / `NOT NULL` constraint
    Nullability,
    /// default value constraint
    Default,
    /// identity constraint
    Identity,
    /// generated column constraint
    Generated,
    /// check constraint
    Check,
    /// primary key constraint
    PrimaryKey,
    /// unique constraint
    Unique,
    /// exclusion constraint
    Exclusion,
    /// foreign key constraint
    ForeignKey,
    /// deferrable , not deferrable constraint
    AttrDeferrability,
    /// deferred constraint
    AttrDeferred,
    /// immediate constraint
    AttrImmediate,
    /// collate constraint
    Collate,
}

impl ColumnConstraintTrait for ColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        match self {
            Self::Nullability(_) => ColumnConstraintKind::Nullability,
            Self::Default(_) => ColumnConstraintKind::Default,
            Self::Identity(_) => ColumnConstraintKind::Identity,
            Self::Generated(_) => ColumnConstraintKind::Generated,
            Self::Check(_) => ColumnConstraintKind::Check,
            Self::PrimaryKey(_) => ColumnConstraintKind::PrimaryKey,
            Self::Unique(_) => ColumnConstraintKind::Unique,
            Self::Exclusion(_) => ColumnConstraintKind::Exclusion,
            Self::ForeignKey(_) => ColumnConstraintKind::ForeignKey,
            Self::AttrDeferrability(_) => ColumnConstraintKind::AttrDeferrability,
            Self::AttrDeferred(_) => ColumnConstraintKind::AttrDeferred,
            Self::AttrImmediate(_) => ColumnConstraintKind::AttrImmediate,
            Self::Collate(_) => ColumnConstraintKind::Collate,
        }
    }

    fn table_name(&self) -> &TableName {
        match self {
            Self::Nullability(c) => c.table_name(),
            Self::Default(c) => c.table_name(),
            Self::Identity(c) => c.table_name(),
            Self::Generated(c) => c.table_name(),
            Self::Check(c) => c.table_name(),
            Self::PrimaryKey(c) => c.table_name(),
            Self::Unique(c) => c.table_name(),
            Self::Exclusion(c) => c.table_name(),
            Self::ForeignKey(c) => c.table_name(),
            Self::AttrDeferrability(c) => c.table_name(),
            Self::AttrDeferred(c) => c.table_name(),
            Self::AttrImmediate(c) => c.table_name(),
            Self::Collate(c) => c.table_name(),
        }
    }

    fn column_name(&self) -> &ColumnName {
        match self {
            Self::Nullability(c) => c.column_name(),
            Self::Default(c) => c.column_name(),
            Self::Identity(c) => c.column_name(),
            Self::Generated(c) => c.column_name(),
            Self::Check(c) => c.column_name(),
            Self::PrimaryKey(c) => c.column_name(),
            Self::Unique(c) => c.column_name(),
            Self::Exclusion(c) => c.column_name(),
            Self::ForeignKey(c) => c.column_name(),
            Self::AttrDeferrability(c) => c.column_name(),
            Self::AttrDeferred(c) => c.column_name(),
            Self::AttrImmediate(c) => c.column_name(),
            Self::Collate(c) => c.column_name(),
        }
    }
}

#[cfg(test)]
mod test_column_constraint_trait_for_column_constraint {
    use super::ColumnNullabilityConstraint;
    use crate::{
        sem::{
            AttrDeferrabilityColumnConstraint, AttrDeferredColumnConstraint,
            AttrImmediateColumnConstraint, CheckColumnConstraint, CollateColumnConstraint,
            ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, DefaultColumnConstraint,
            ExclusionColumnConstraint, ForeignKeyColumnConstraint, GeneratedColumnConstraint,
            GeneratedColumnMode, IdentityColumnConstraint, IdentityColumnMode,
            PrimaryKeyColumnConstraint, UniqueColumnConstraint,
        },
        test_helpers::{SemValueBuilder, sem_builder},
    };
    use testresult::TestResult;

    #[rstest::rstest]
    fn test_kind_nullability(sem_builder: SemValueBuilder) -> TestResult {
        let c = ColumnNullabilityConstraint::new(
            &sem_builder.table_name("table_one"),
            &sem_builder.column_name("column_one"),
            false,
            None,
            None,
        );
        assert_eq!(c.kind(), ColumnConstraintKind::Nullability);
        let column_constraint = ColumnConstraint::nullability(c);
        assert_eq!(column_constraint.kind(), ColumnConstraintKind::Nullability);
        assert_eq!(
            "table_one",
            column_constraint
                .table_name()
                .full_name()
                .local_name()
                .as_str()
        );
        assert_eq!("column_one", column_constraint.column_name().as_str());
        assert!(column_constraint.as_nullability().is_some());
        Ok(())
    }

    #[rstest::rstest]
    fn test_kind_default(sem_builder: SemValueBuilder) -> TestResult {
        let c = DefaultColumnConstraint::new(
            &sem_builder.table_name("table_one"),
            &sem_builder.column_name("column_one"),
            sem_builder.const_int_expr(0),
        );
        assert_eq!(c.kind(), ColumnConstraintKind::Default);
        let column_constraint = ColumnConstraint::default(c);
        assert_eq!(column_constraint.kind(), ColumnConstraintKind::Default);
        assert_eq!(
            "table_one",
            column_constraint
                .table_name()
                .full_name()
                .local_name()
                .as_str()
        );
        assert_eq!("column_one", column_constraint.column_name().as_str());
        assert!(column_constraint.as_default().is_some());
        Ok(())
    }

    #[rstest::rstest]
    fn test_kind_identity(sem_builder: SemValueBuilder) -> TestResult {
        let c = IdentityColumnConstraint::new(
            &sem_builder.table_name("table_one"),
            &sem_builder.column_name("column_one"),
            IdentityColumnMode::Always,
        );
        assert_eq!(c.kind(), ColumnConstraintKind::Identity);
        let column_constraint = ColumnConstraint::identity(c);
        assert_eq!(column_constraint.kind(), ColumnConstraintKind::Identity);
        assert_eq!(
            "table_one",
            column_constraint
                .table_name()
                .full_name()
                .local_name()
                .as_str()
        );
        assert_eq!("column_one", column_constraint.column_name().as_str());
        assert!(column_constraint.as_identity().is_some());
        Ok(())
    }

    #[rstest::rstest]
    fn test_kind_generated(sem_builder: SemValueBuilder) -> TestResult {
        let c = GeneratedColumnConstraint::new(
            &sem_builder.table_name("table_one"),
            &sem_builder.column_name("column_one"),
            sem_builder.const_int_expr(0),
            Some(GeneratedColumnMode::Stored),
        );
        assert_eq!(c.kind(), ColumnConstraintKind::Generated);
        let column_constraint = ColumnConstraint::generated(c);
        assert_eq!(column_constraint.kind(), ColumnConstraintKind::Generated);
        assert_eq!(
            "table_one",
            column_constraint
                .table_name()
                .full_name()
                .local_name()
                .as_str()
        );
        assert_eq!("column_one", column_constraint.column_name().as_str());
        assert!(column_constraint.as_generated().is_some());
        Ok(())
    }

    #[rstest::rstest]
    fn test_kind_check(sem_builder: SemValueBuilder) -> TestResult {
        let c = CheckColumnConstraint::new(
            &sem_builder.table_name("table_one"),
            &sem_builder.column_name("column_one"),
            sem_builder.const_int_expr(0),
        );
        assert_eq!(c.kind(), ColumnConstraintKind::Check);
        let column_constraint = ColumnConstraint::check(c);
        assert_eq!(column_constraint.kind(), ColumnConstraintKind::Check);
        assert_eq!(
            "table_one",
            column_constraint
                .table_name()
                .full_name()
                .local_name()
                .as_str()
        );
        assert_eq!("column_one", column_constraint.column_name().as_str());
        assert!(column_constraint.as_check().is_some());
        Ok(())
    }

    #[rstest::rstest]
    fn test_kind_primary_key(sem_builder: SemValueBuilder) -> TestResult {
        let c = PrimaryKeyColumnConstraint::new(
            &sem_builder.table_name("table_one"),
            &sem_builder.column_name("column_one"),
            None,
            None,
        );
        assert_eq!(c.kind(), ColumnConstraintKind::PrimaryKey);
        let column_constraint = ColumnConstraint::primary_key(c);
        assert_eq!(column_constraint.kind(), ColumnConstraintKind::PrimaryKey);
        assert_eq!(
            "table_one",
            column_constraint
                .table_name()
                .full_name()
                .local_name()
                .as_str()
        );
        assert_eq!("column_one", column_constraint.column_name().as_str());
        assert!(column_constraint.as_primary_key().is_some());
        Ok(())
    }

    #[rstest::rstest]
    fn test_kind_unique(sem_builder: SemValueBuilder) -> TestResult {
        let c = UniqueColumnConstraint::new(
            &sem_builder.table_name("table_one"),
            &sem_builder.column_name("column_one"),
            None,
        );
        assert_eq!(c.kind(), ColumnConstraintKind::Unique);
        let column_constraint = ColumnConstraint::unique(c);
        assert_eq!(column_constraint.kind(), ColumnConstraintKind::Unique);
        assert_eq!(
            "table_one",
            column_constraint
                .table_name()
                .full_name()
                .local_name()
                .as_str()
        );
        assert_eq!("column_one", column_constraint.column_name().as_str());
        assert!(column_constraint.as_unique().is_some());
        Ok(())
    }

    #[rstest::rstest]
    fn test_kind_exclusion(sem_builder: SemValueBuilder) -> TestResult {
        let c = ExclusionColumnConstraint::new(
            &sem_builder.table_name("table_one"),
            &sem_builder.column_name("column_one"),
        );
        assert_eq!(c.kind(), ColumnConstraintKind::Exclusion);
        let column_constraint = ColumnConstraint::exclusion(c);
        assert_eq!(column_constraint.kind(), ColumnConstraintKind::Exclusion);
        assert_eq!(
            "table_one",
            column_constraint
                .table_name()
                .full_name()
                .local_name()
                .as_str()
        );
        assert_eq!("column_one", column_constraint.column_name().as_str());
        assert!(column_constraint.as_exclusion().is_some());
        Ok(())
    }

    #[rstest::rstest]
    fn test_kind_foreign(sem_builder: SemValueBuilder) -> TestResult {
        let c = ForeignKeyColumnConstraint::new(
            &sem_builder.table_name("table_one"),
            &sem_builder.column_name("column_one"),
            &sem_builder.table_name("pk_table"),
            &sem_builder.column_name("pk_col"),
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(c.kind(), ColumnConstraintKind::ForeignKey);
        let column_constraint = ColumnConstraint::foreign_key(c);
        assert_eq!(column_constraint.kind(), ColumnConstraintKind::ForeignKey);
        assert_eq!(
            "table_one",
            column_constraint
                .table_name()
                .full_name()
                .local_name()
                .as_str()
        );
        assert_eq!("column_one", column_constraint.column_name().as_str());
        assert!(column_constraint.as_foreign_key().is_some());
        Ok(())
    }

    #[rstest::rstest]
    fn test_kind_attr_deferrability(sem_builder: SemValueBuilder) -> TestResult {
        let c = AttrDeferrabilityColumnConstraint::new(
            &sem_builder.table_name("table_one"),
            &sem_builder.column_name("column_one"),
            false,
        );
        assert_eq!(c.kind(), ColumnConstraintKind::AttrDeferrability);
        let column_constraint = ColumnConstraint::attr_deferrability(c);
        assert_eq!(
            column_constraint.kind(),
            ColumnConstraintKind::AttrDeferrability
        );
        assert_eq!(
            "table_one",
            column_constraint
                .table_name()
                .full_name()
                .local_name()
                .as_str()
        );
        assert_eq!("column_one", column_constraint.column_name().as_str());
        assert!(column_constraint.as_attr_deferrability().is_some());
        Ok(())
    }

    #[rstest::rstest]
    fn test_kind_attr_deferred(sem_builder: SemValueBuilder) -> TestResult {
        let c = AttrDeferredColumnConstraint::new(
            &sem_builder.table_name("table_one"),
            &sem_builder.column_name("column_one"),
        );
        assert_eq!(c.kind(), ColumnConstraintKind::AttrDeferred);
        let column_constraint = ColumnConstraint::attr_deferred(c);
        assert_eq!(column_constraint.kind(), ColumnConstraintKind::AttrDeferred);
        assert_eq!(
            "table_one",
            column_constraint
                .table_name()
                .full_name()
                .local_name()
                .as_str()
        );
        assert_eq!("column_one", column_constraint.column_name().as_str());
        assert!(column_constraint.as_attr_deferred().is_some());
        Ok(())
    }

    #[rstest::rstest]
    fn test_kind_attr_immediate(sem_builder: SemValueBuilder) -> TestResult {
        let c = AttrImmediateColumnConstraint::new(
            &sem_builder.table_name("table_one"),
            &sem_builder.column_name("column_one"),
        );
        assert_eq!(c.kind(), ColumnConstraintKind::AttrImmediate);
        let column_constraint = ColumnConstraint::attr_immediate(c);
        assert_eq!(
            column_constraint.kind(),
            ColumnConstraintKind::AttrImmediate
        );
        assert_eq!(
            "table_one",
            column_constraint
                .table_name()
                .full_name()
                .local_name()
                .as_str()
        );
        assert_eq!("column_one", column_constraint.column_name().as_str());
        assert!(column_constraint.as_attr_immediate().is_some());
        Ok(())
    }

    #[rstest::rstest]
    fn test_kind_collate(sem_builder: SemValueBuilder) -> TestResult {
        let c = CollateColumnConstraint::new(
            &sem_builder.table_name("table_one"),
            &sem_builder.column_name("column_one"),
            "collation",
        );
        assert_eq!(c.kind(), ColumnConstraintKind::Collate);
        let column_constraint = ColumnConstraint::collate(c);
        assert_eq!(column_constraint.kind(), ColumnConstraintKind::Collate);
        assert_eq!(
            "table_one",
            column_constraint
                .table_name()
                .full_name()
                .local_name()
                .as_str()
        );
        assert_eq!("column_one", column_constraint.column_name().as_str());
        assert!(column_constraint.as_collate().is_some());
        Ok(())
    }
}

impl ColumnConstraint {
    pub(crate) fn analyze_constraint<TParseContext>(
        mut context: TParseContext,
        table_name: &TableName,
        column_name: &ColumnName,
        syn: &crate::syn::Constraint,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(con_type) = syn.get_contype().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.contype")?;
            AnalysisError::never()
        };

        let constraint = match con_type {
            ConstrType::ConstrNull | ConstrType::ConstrNotnull => {
                let (null_constraint, new_context) =
                    ColumnNullabilityConstraint::analyze_constraint(
                        context,
                        table_name,
                        column_name,
                        syn,
                    )?;
                context = new_context;
                Self::nullability(null_constraint)
            }
            ConstrType::ConstrDefault => {
                let (c, new_context) = DefaultColumnConstraint::analyze_constraint(
                    context,
                    table_name,
                    column_name,
                    syn,
                    tokens,
                )?;
                context = new_context;
                Self::default(c)
            }
            ConstrType::ConstrIdentity => {
                let (c, new_context) = IdentityColumnConstraint::analyze_constraint(
                    context,
                    table_name,
                    column_name,
                    syn,
                )?;
                context = new_context;
                Self::identity(c)
            }
            ConstrType::ConstrGenerated => {
                let (c, new_context) = GeneratedColumnConstraint::analyze_constraint(
                    context,
                    table_name,
                    column_name,
                    syn,
                    tokens,
                )?;
                context = new_context;
                Self::generated(c)
            }
            ConstrType::ConstrCheck => {
                let (c, new_context) = CheckColumnConstraint::analyze_constraint(
                    context,
                    table_name,
                    column_name,
                    syn,
                    tokens,
                )?;
                context = new_context;
                Self::check(c)
            }
            ConstrType::ConstrPrimary => {
                let (pk_constraint, new_context) = PrimaryKeyColumnConstraint::analyze_constraint(
                    context,
                    table_name,
                    column_name,
                    syn,
                )?;
                context = new_context;
                Self::primary_key(pk_constraint)
            }
            ConstrType::ConstrUnique => {
                let (uniq_constraint, new_context) = UniqueColumnConstraint::analyze_constraint(
                    context,
                    table_name,
                    column_name,
                    syn,
                )?;
                context = new_context;
                Self::unique(uniq_constraint)
            }
            ConstrType::ConstrExclusion => {
                let (c, new_context) = ExclusionColumnConstraint::analyze_constraint(
                    context,
                    table_name,
                    column_name,
                    syn,
                )?;
                context = new_context;
                Self::exclusion(c)
            }
            ConstrType::ConstrForeign => {
                let (c, new_context) = ForeignKeyColumnConstraint::analyze_constraint(
                    context,
                    table_name,
                    column_name,
                    syn,
                )?;
                context = new_context;
                Self::foreign_key(c)
            }
            ConstrType::ConstrAttrDeferrable | ConstrType::ConstrAttrNotDeferrable => {
                let (c, new_context) = AttrDeferrabilityColumnConstraint::analyze_constraint(
                    context,
                    table_name,
                    column_name,
                    syn,
                )?;
                context = new_context;
                Self::attr_deferrability(c)
            }
            ConstrType::ConstrAttrDeferred => {
                let (c, new_context) = AttrDeferredColumnConstraint::analyze_constraint(
                    context,
                    table_name,
                    column_name,
                    syn,
                )?;
                context = new_context;
                Self::attr_deferred(c)
            }
            ConstrType::ConstrAttrImmediate => {
                let (c, new_context) = AttrImmediateColumnConstraint::analyze_constraint(
                    context,
                    table_name,
                    column_name,
                    syn,
                )?;
                context = new_context;
                Self::attr_immediate(c)
            }
            ConstrType::OptionNone | ConstrType::Undefined => {
                AnalysisError::raise_unexpected_input("constraint.contype")?;
                AnalysisError::never()
            }
        };
        Ok((constraint, context))
    }
}

#[cfg(test)]
mod column_constraint_analyze_constraint_tests {
    use crate::{
        sem::{
            ColumnConstraint, ColumnName, FullName, TableName,
            column_constraints::{ColumnConstraintKind, ColumnConstraintTrait},
        },
        syn::Constraint,
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

    use testresult::TestResult;

    fn constr_not_null() -> Constraint {
        SynBuilder::new().constraint_not_null()
    }

    fn constr_null() -> Constraint {
        SynBuilder::new().constraint_null()
    }

    fn constr_default() -> Constraint {
        SynBuilder::new().constraint_default()
    }

    fn constr_identity() -> Constraint {
        SynBuilder::new().constraint_identity("d")
    }

    fn constr_generated() -> Constraint {
        SynBuilder::new().constraint_generated("s")
    }

    fn constr_check() -> Constraint {
        SynBuilder::new().constraint_check()
    }

    fn constr_primary() -> Constraint {
        SynBuilder::new().constraint_primary_key()
    }

    fn constr_unique() -> Constraint {
        SynBuilder::new().constraint_unique()
    }

    fn constr_exclusion() -> Constraint {
        SynBuilder::new().constraint_exclusion()
    }

    fn constr_foreign() -> Constraint {
        SynBuilder::new().constraint_foreign()
    }

    fn constr_attr_deferrable() -> Constraint {
        SynBuilder::new().constraint_attr_deferrable()
    }

    fn constr_attr_not_deferrable() -> Constraint {
        SynBuilder::new().constraint_attr_not_deferrable()
    }

    fn constr_attr_deferred() -> Constraint {
        SynBuilder::new().constraint_attr_deferred()
    }

    fn constr_attr_immediate() -> Constraint {
        SynBuilder::new().constraint_attr_immediate()
    }

    #[rstest::rstest]
    #[case::constr_not_null(constr_not_null(), ColumnConstraintKind::Nullability)]
    #[case::constr_null(constr_null(), ColumnConstraintKind::Nullability)]
    #[case::constr_default(constr_default(), ColumnConstraintKind::Default)]
    #[case::constr_identity(constr_identity(), ColumnConstraintKind::Identity)]
    #[case::constr_generated(constr_generated(), ColumnConstraintKind::Generated)]
    #[case::constr_check(constr_check(), ColumnConstraintKind::Check)]
    #[case::constr_primary(constr_primary(), ColumnConstraintKind::PrimaryKey)]
    #[case::constr_unique(constr_unique(), ColumnConstraintKind::Unique)]
    #[case::constr_exclusion(constr_exclusion(), ColumnConstraintKind::Exclusion)]
    #[case::constr_foreign(constr_foreign(), ColumnConstraintKind::ForeignKey)]
    #[case::constr_deferrable(constr_attr_deferrable(), ColumnConstraintKind::AttrDeferrability)]
    #[case::constr_deferrable(
        constr_attr_not_deferrable(),
        ColumnConstraintKind::AttrDeferrability
    )]
    #[case::constr_deferred(constr_attr_deferred(), ColumnConstraintKind::AttrDeferred)]
    #[case::constr_immediate(constr_attr_immediate(), ColumnConstraintKind::AttrImmediate)]
    fn test_analyze_constraint(
        test_context: TestParseContext,
        #[case] syn: Constraint,
        #[case] kind: ColumnConstraintKind,
    ) -> TestResult {
        let table_name = TableName::from(FullName::from_local_name("table1"));
        let column_name = ColumnName::new("col1");
        let tokens = Vec::default().into();
        let (constraint, _new_context) = ColumnConstraint::analyze_constraint(
            test_context,
            &table_name,
            &column_name,
            &syn,
            &tokens,
        )?;
        assert_eq!(constraint.kind(), kind);
        Ok(())
    }
}

/// Conflict clause
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub enum ConflictClause {
    /// Rollback
    Rollback,
    /// Abort
    Abort,
    /// fail
    Fail,
    /// ignore
    Ignore,
    /// replace
    Replace,
}

/// Validity clause
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct Validity {
    skip_validation: bool,
    initially_valid: bool,
}

impl Validity {
    /// create instance
    pub const fn new(skip_validation: bool, initially_valid: bool) -> Self {
        Self {
            skip_validation,
            initially_valid,
        }
    }

    /// create from [`crate::syn::Constraint`]
    pub fn from_constraint(syn: &crate::syn::Constraint) -> Self {
        Self::new(syn.get_skip_validation(), syn.get_initially_valid())
    }
}

/// deferrable constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct Deferrable {
    init_deferred: bool,
}

impl Deferrable {
    /// create instance
    pub const fn new(init_deferred: bool) -> Self {
        Self { init_deferred }
    }

    /// from [`crate::syn::Constraint`]
    pub fn try_from_constraint(
        syn: &crate::syn::Constraint,
    ) -> Result<Option<Self>, AnalysisError> {
        let r = if syn.get_deferrable() {
            Some(Self::new(syn.get_initdeferred()))
        } else {
            if syn.get_initdeferred() {
                AnalysisError::raise_unexpected_input("defferable: false, but initdeferred")?;
            }
            None
        };
        Ok(r)
    }
}
