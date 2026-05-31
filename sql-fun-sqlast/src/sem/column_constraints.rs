use sql_fun_core::IVec;

use crate::{
    sem::{
        AnalysisError, AnalysisProblem, ColumnName, FromClause, ParseContext, SemScalarExpr,
        TableName, TypeReference, WithClause,
    },
    syn::{ConstrType, ListOpt, Opt, ScanToken},
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
    /// attr Defferred
    AttrDeferred(AttrDeferredColumnConstraint),
    /// attr Immidiate
    AttrImmidiate(AttrImmidiateColumnConstraint),
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
    /// immidiate constraint
    AttrImmidiate,
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
            Self::AttrImmidiate(_) => ColumnConstraintKind::AttrImmidiate,
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
            Self::AttrImmidiate(c) => c.table_name(),
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
            Self::AttrImmidiate(c) => c.column_name(),
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
            AttrImmidiateColumnConstraint, CheckColumnConstraint, CollateColumnConstraint,
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
            GeneratedColumnMode::Stored,
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
    fn test_kind_attr_immidiate(sem_builder: SemValueBuilder) -> TestResult {
        let c = AttrImmidiateColumnConstraint::new(
            &sem_builder.table_name("table_one"),
            &sem_builder.column_name("column_one"),
        );
        assert_eq!(c.kind(), ColumnConstraintKind::AttrImmidiate);
        let column_constraint = ColumnConstraint::attr_immidiate(c);
        assert_eq!(
            column_constraint.kind(),
            ColumnConstraintKind::AttrImmidiate
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
        assert!(column_constraint.as_attr_immidiate().is_some());
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
                let (c, new_context) = AttrImmidiateColumnConstraint::analyze_constraint(
                    context,
                    table_name,
                    column_name,
                    syn,
                )?;
                context = new_context;
                Self::attr_immidiate(c)
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
    #[case::constr_immediate(constr_attr_immediate(), ColumnConstraintKind::AttrImmidiate)]
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

/// column value collation
///
/// # SQL Dialect / Engine Specific
///
/// - only used in `SQLite` dialect
///
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct CollateColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    collation_name: String,
}

impl CollateColumnConstraint {
    /// create instance
    pub fn new(table_name: &TableName, column_name: &ColumnName, collation_name: &str) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            collation_name: String::from(collation_name),
        }
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
    pub fn collate(value: CollateColumnConstraint) -> Self {
        Self::Collate(value)
    }

    /// get Collate variant
    pub fn as_collate(&self) -> Option<&CollateColumnConstraint> {
        if let Self::Collate(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

/// default value for column
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct DefaultColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    expr: SemScalarExpr,
}

impl DefaultColumnConstraint {
    /// create instance
    pub fn new(table_name: &TableName, column_name: &ColumnName, expr: SemScalarExpr) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            expr,
        }
    }
}

impl ColumnConstraintTrait for DefaultColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::Default
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl DefaultColumnConstraint {
    /// analyze [`crate::syn::Constraint`]
    pub fn analyze_constraint<TParseContext>(
        mut context: TParseContext,
        table_name: &TableName,
        column_name: &ColumnName,
        syn: &crate::syn::Constraint,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(contype) = syn.get_contype().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.contype")?;
            AnalysisError::never()
        };

        if !matches!(contype, ConstrType::ConstrDefault) {
            AnalysisError::raise_unexpected_input("constraint.contype")?;
            AnalysisError::never()
        };

        let Some(default_expr) = syn.get_raw_expr().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.raw_expr")?;
            AnalysisError::never()
        };
        let (expr, new_context) = SemScalarExpr::analyze_scaler_expr(
            context,
            &WithClause::default(),
            &FromClause::default(),
            default_expr,
            tokens,
        )?;
        context = new_context;
        Ok((Self::new(table_name, column_name, expr), context))
    }
}

impl ColumnConstraint {
    /// create Default variant
    pub fn default(value: DefaultColumnConstraint) -> Self {
        Self::Default(value)
    }

    /// get Default variant
    pub fn as_default(&self) -> Option<&DefaultColumnConstraint> {
        if let Self::Default(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

/// column value constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct CheckColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    expr: SemScalarExpr,
}

impl CheckColumnConstraint {
    /// create instance
    pub fn new(table_name: &TableName, column_name: &ColumnName, expr: SemScalarExpr) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            expr,
        }
    }
}

impl ColumnConstraintTrait for CheckColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::Check
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl CheckColumnConstraint {
    /// analyze [`crate::syn::Constraint`]
    pub fn analyze_constraint<TParseContext>(
        context: TParseContext,
        table_name: &TableName,
        column_name: &ColumnName,
        syn: &crate::syn::Constraint,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(contype) = syn.get_contype().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.contype")?;
            AnalysisError::never()
        };
        if !matches!(contype, ConstrType::ConstrCheck) {
            AnalysisError::raise_unexpected_input("constraint.contype")?;
            AnalysisError::never()
        }
        let Some(check_expr) = syn.get_raw_expr().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.raw_expr")?;
            AnalysisError::never()
        };

        let (expr, new_context) = SemScalarExpr::analyze_scaler_expr(
            context,
            &WithClause::default(),
            &FromClause::default(),
            check_expr,
            tokens,
        )?;
        Ok((Self::new(table_name, column_name, expr), new_context))
    }
}

impl ColumnConstraint {
    /// create Check variant
    pub fn check(value: CheckColumnConstraint) -> Self {
        Self::Check(value)
    }

    /// get check variant
    pub fn as_check(&self) -> Option<&CheckColumnConstraint> {
        if let Self::Check(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

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

impl ColumnConstraint {
    /// create Unique variant
    pub fn unique(value: UniqueColumnConstraint) -> Self {
        Self::Unique(value)
    }

    /// get Unique variant
    pub fn as_unique(&self) -> Option<&UniqueColumnConstraint> {
        if let Self::Unique(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

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
    fn is_not_null(contype: ConstrType) -> Result<bool, AnalysisError> {
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

        let not_null = Self::is_not_null(contype)?;
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

impl ColumnConstraint {
    /// create Nullability variant
    pub fn nullability(value: ColumnNullabilityConstraint) -> Self {
        Self::Nullability(value)
    }

    /// get Nullability variant
    pub fn as_nullability(&self) -> Option<&ColumnNullabilityConstraint> {
        if let Self::Nullability(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

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
    pub fn new(skip_validation: bool, initially_valid: bool) -> Self {
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
    pub fn new(init_deferred: bool) -> Self {
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

/// Identity column mode
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub enum IdentityColumnMode {
    /// generate value always
    Always,
    /// generate value when not value supplied
    ByDefault,
}

impl IdentityColumnMode {
    /// create Always variant
    pub fn always() -> Self {
        Self::Always
    }

    /// create ByDefault variant
    pub fn by_default() -> Self {
        Self::ByDefault
    }

    const IDENTITY_MODE_ALWAYS: &'static str = "a";
    const IDENTITY_MODE_BY_DEFAULT: &'static str = "d";

    /// from [`crate::syn::Constraint`] `generated_when` value
    pub fn from_postgres_generate_when(value: &str) -> Result<Self, AnalysisError> {
        if value == Self::IDENTITY_MODE_ALWAYS {
            Ok(Self::always())
        } else if value == Self::IDENTITY_MODE_BY_DEFAULT {
            Ok(Self::by_default())
        } else {
            AnalysisError::raise_unexpected_input("constraint.generated_when")?;
            AnalysisError::never()
        }
    }
}

#[cfg(test)]
mod identity_column_mode_from_postgres_generate_when {
    use testresult::TestResult;

    use crate::sem::column_constraints::IdentityColumnMode;

    #[rstest::rstest]
    #[case("a", IdentityColumnMode::Always)]
    #[case("d", IdentityColumnMode::ByDefault)]
    fn test_from_pg_generate_when(
        #[case] value: &str,
        #[case] expect: IdentityColumnMode,
    ) -> TestResult {
        let actual = IdentityColumnMode::from_postgres_generate_when(value)?;
        assert_eq!(actual, expect);
        Ok(())
    }

    #[test]
    #[should_panic]
    fn test_invalid_generate_when() {
        let _ = IdentityColumnMode::from_postgres_generate_when("x").unwrap();
    }
}

/// identity column constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct IdentityColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    generated_when: IdentityColumnMode,
}

impl IdentityColumnConstraint {
    /// create instance
    pub fn new(
        table_name: &TableName,
        column_name: &ColumnName,
        generated_when: IdentityColumnMode,
    ) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            generated_when,
        }
    }
}

impl ColumnConstraintTrait for IdentityColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::Identity
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl IdentityColumnConstraint {
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
        if !matches!(contype, ConstrType::ConstrIdentity) {
            AnalysisError::raise_unexpected_input("contype not identity")?;
            AnalysisError::never()
        }
        let generated_when =
            IdentityColumnMode::from_postgres_generate_when(&syn.get_generated_when())?;
        Ok((Self::new(table_name, column_name, generated_when), context))
    }
}

impl ColumnConstraint {
    /// create identity variant
    pub fn identity(value: IdentityColumnConstraint) -> Self {
        Self::Identity(value)
    }

    /// get identity variant
    pub fn as_identity(&self) -> Option<&IdentityColumnConstraint> {
        if let Self::Identity(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

/// generated column mode
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub enum GeneratedColumnMode {
    /// generated value store in storage
    Stored,
    /// generated value computation at every read, not in storage
    Virtual,
}

impl GeneratedColumnMode {
    /// create Stored variant
    pub fn stored() -> Self {
        Self::Stored
    }

    /// create Virtual variant
    pub fn r#virtual() -> Self {
        Self::Virtual
    }

    const GENERATED_COLUMN_MODE_STORED: &'static str = "s";
    const GENERATED_COLUMN_MODE_VIRTUAL: &'static str = "v";

    /// from [`crate::syn::Constraint`] `generated_when` value
    pub fn from_postgres_generated_column_mode(value: &str) -> Result<Self, AnalysisError> {
        if value == Self::GENERATED_COLUMN_MODE_STORED {
            Ok(Self::stored())
        } else if value == Self::GENERATED_COLUMN_MODE_VIRTUAL {
            Ok(Self::r#virtual())
        } else {
            AnalysisError::raise_unexpected_input("generated_when")?;
            AnalysisError::never()
        }
    }
}

#[cfg(test)]
mod test_generated_column_mode_from_pg {
    use testresult::TestResult;

    use crate::sem::column_constraints::GeneratedColumnMode;

    #[rstest::rstest]
    #[case("s", GeneratedColumnMode::Stored)]
    #[case("v", GeneratedColumnMode::Virtual)]
    fn test_generated_colum_mode(
        #[case] value: &str,
        #[case] expected: GeneratedColumnMode,
    ) -> TestResult {
        let actual = GeneratedColumnMode::from_postgres_generated_column_mode(value)?;
        assert_eq!(expected, actual);
        Ok(())
    }

    #[test]
    #[should_panic]
    fn test_invalid_generated_column_mode() {
        GeneratedColumnMode::from_postgres_generated_column_mode("x").unwrap();
    }
}

/// generated column / computed column constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct GeneratedColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    expr: SemScalarExpr,
    generated_when: GeneratedColumnMode,
}

impl GeneratedColumnConstraint {
    /// create instance
    pub fn new(
        table_name: &TableName,
        column_name: &ColumnName,
        expr: SemScalarExpr,
        generated_when: GeneratedColumnMode,
    ) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            expr,
            generated_when,
        }
    }
}

impl ColumnConstraintTrait for GeneratedColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::Generated
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl GeneratedColumnConstraint {
    /// analyze [`crate::syn::Constraint`]
    pub fn analyze_constraint<TParseContext>(
        mut context: TParseContext,
        table_name: &TableName,
        column_name: &ColumnName,
        syn: &crate::syn::Constraint,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(contype) = syn.get_contype().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.contype")?;
            AnalysisError::never()
        };
        if !matches!(contype, ConstrType::ConstrGenerated) {
            AnalysisError::raise_unexpected_input("contype not generated")?;
            AnalysisError::never()
        }

        let Some(raw_expr) = syn.get_raw_expr().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.raw_expr")?;
            AnalysisError::never()
        };

        let generated_when =
            GeneratedColumnMode::from_postgres_generated_column_mode(&syn.get_generated_when())?;
        let (expr, new_context) = SemScalarExpr::analyze_scaler_expr(
            context,
            &WithClause::default(),
            &FromClause::default(),
            raw_expr,
            tokens,
        )?;
        context = new_context;
        Ok((
            Self::new(table_name, column_name, expr, generated_when),
            context,
        ))
    }
}

impl ColumnConstraint {
    /// create Genereted variant
    pub fn generated(value: GeneratedColumnConstraint) -> Self {
        Self::Generated(value)
    }

    /// get Generated variant
    pub fn as_generated(&self) -> Option<&GeneratedColumnConstraint> {
        if let Self::Generated(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

// FIXME: exclusion constraint applies column?
/// exclusion column constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct ExclusionColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
}

impl ExclusionColumnConstraint {
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
        syn: &crate::syn::Constraint,
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
    pub fn exclusion(value: ExclusionColumnConstraint) -> Self {
        Self::Exclusion(value)
    }

    /// get Exclusion variant
    pub fn as_exclusion(&self) -> Option<&ExclusionColumnConstraint> {
        if let Self::Exclusion(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

/// foreign key constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct ForeignKeyColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    primary_key_table: TableName,
    primary_key_column: ColumnName,
    primary_key_column_type: Option<TypeReference>, // TODO: fill fields
}

impl ForeignKeyColumnConstraint {
    /// create instance
    pub fn new(
        table_name: &TableName,
        column_name: &ColumnName,
        primary_key_table: &TableName,
        primary_key_column: &ColumnName,
        primary_key_column_type: Option<&TypeReference>,
    ) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            primary_key_table: primary_key_table.clone(),
            primary_key_column: primary_key_column.clone(),
            primary_key_column_type: primary_key_column_type.cloned(),
        }
    }

    /// get primary key table name
    pub fn primary_key_table(&self) -> &TableName {
        &self.primary_key_table
    }

    /// get primary key column name
    pub fn primary_key_column(&self) -> &ColumnName {
        &self.primary_key_column
    }

    /// get primary key column type
    pub fn primary_key_column_type(&self) -> Option<&TypeReference> {
        self.primary_key_column_type.as_ref()
    }
}

impl ColumnConstraintTrait for ForeignKeyColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::ForeignKey
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl ForeignKeyColumnConstraint {
    /// analyze [`crate::syn::Constraint`]
    pub fn analyze_constraint<TParseContext>(
        mut context: TParseContext,
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
        if !matches!(contype, ConstrType::ConstrForeign) {
            AnalysisError::raise_unexpected_input("contype not foreign")?;
            AnalysisError::never()
        }

        let Some(pktable) = syn.get_pktable().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.pktable")?;
            AnalysisError::never()
        };
        let primary_key_table = TableName::try_from(pktable)?;
        let Some(pkattr) = syn.get_pk_attrs().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.pk_attrs")?;
            AnalysisError::never()
        };
        if pkattr.len() != 1 {
            AnalysisError::raise_unexpected_input("constraint.pk_attrs.len!=1")?;
            AnalysisError::never()
        }
        let primary_key_column = ColumnName::try_from(&pkattr[0])?;

        let primary_key_column_type = if let Some(pk_table) = context.get_table(&primary_key_table)
        {
            if let Some(pk_col_def) = pk_table.get_column_def(column_name) {
                pk_col_def.get_type()
            } else {
                context.report_problem(AnalysisProblem::column_not_found_in_table(
                    &primary_key_table,
                    &primary_key_column,
                ))?;
                None
            }
        } else {
            context.report_problem(AnalysisProblem::table_not_found(&primary_key_table))?;
            None
        };

        Ok((
            Self::new(
                table_name,
                column_name,
                &primary_key_table,
                &primary_key_column,
                primary_key_column_type,
            ),
            context,
        ))
    }
}

impl ColumnConstraint {
    /// create ForeignKey variant
    pub fn foreign_key(value: ForeignKeyColumnConstraint) -> Self {
        Self::ForeignKey(value)
    }

    /// get ForeignKey variant
    pub fn as_foreign_key(&self) -> Option<&ForeignKeyColumnConstraint> {
        if let Self::ForeignKey(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

/// deferrable / not deferrable constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct AttrDeferrabilityColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    deferrable: bool,
}

impl AttrDeferrabilityColumnConstraint {
    /// create instance
    pub fn new(table_name: &TableName, column_name: &ColumnName, deferrable: bool) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            deferrable,
        }
    }
}

impl ColumnConstraintTrait for AttrDeferrabilityColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::AttrDeferrability
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl AttrDeferrabilityColumnConstraint {
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
        let attr_deferrable = if matches!(contype, ConstrType::ConstrAttrDeferrable) {
            true
        } else if matches!(contype, ConstrType::ConstrAttrNotDeferrable) {
            false
        } else {
            AnalysisError::raise_unexpected_input("contype not attr deferrable ")?;
            AnalysisError::never()
        };
        Ok((Self::new(table_name, column_name, attr_deferrable), context))
    }
}

impl ColumnConstraint {
    /// create AttrDeferrability variant
    pub fn attr_deferrability(value: AttrDeferrabilityColumnConstraint) -> Self {
        Self::AttrDeferrability(value)
    }

    /// get AttrDeferrability variant
    pub fn as_attr_deferrability(&self) -> Option<&AttrDeferrabilityColumnConstraint> {
        if let Self::AttrDeferrability(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

/// deferred constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct AttrDeferredColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
}

impl AttrDeferredColumnConstraint {
    /// create instance
    pub fn new(table_name: &TableName, column_name: &ColumnName) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
        }
    }
}

impl ColumnConstraintTrait for AttrDeferredColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::AttrDeferred
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl AttrDeferredColumnConstraint {
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
        if !matches!(contype, ConstrType::ConstrAttrDeferred) {
            AnalysisError::raise_unexpected_input("contype not attr deferred")?;
            AnalysisError::never()
        }
        Ok((Self::new(table_name, column_name), context))
    }
}

impl ColumnConstraint {
    /// create AttrDeferred variant
    pub fn attr_deferred(value: AttrDeferredColumnConstraint) -> Self {
        Self::AttrDeferred(value)
    }

    /// get AttrDeferred variant
    pub fn as_attr_deferred(&self) -> Option<&AttrDeferredColumnConstraint> {
        if let Self::AttrDeferred(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

/// immediate constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct AttrImmidiateColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
}

impl AttrImmidiateColumnConstraint {
    /// create instance
    pub fn new(table_name: &TableName, column_name: &ColumnName) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
        }
    }
}

impl ColumnConstraintTrait for AttrImmidiateColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::AttrImmidiate
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl AttrImmidiateColumnConstraint {
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
        if !matches!(contype, ConstrType::ConstrAttrImmediate) {
            AnalysisError::raise_unexpected_input("contype not attr immediate")?;
            AnalysisError::never()
        }
        Ok((Self::new(table_name, column_name), context))
    }
}

impl ColumnConstraint {
    /// create AttrImmidiate variant
    pub fn attr_immidiate(value: AttrImmidiateColumnConstraint) -> Self {
        Self::AttrImmidiate(value)
    }

    /// get AttrImmidiate variant
    pub fn as_attr_immidiate(&self) -> Option<&AttrImmidiateColumnConstraint> {
        if let Self::AttrImmidiate(c) = self {
            Some(c)
        } else {
            None
        }
    }
}
