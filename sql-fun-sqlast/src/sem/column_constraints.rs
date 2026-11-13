mod ddl;

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
    foreign_key_constraint::ForeignKeyColumnConstraint,
    generated_constraint::GeneratedColumnConstraint,
    identity_constraint::IdentityColumnConstraint,
    nullability_constraint::ColumnNullabilityConstraint,
    primary_key_constraint::PrimaryKeyColumnConstraint,
    unique_constraint::UniqueColumnConstraint,
};
use crate::{
    names::{ColumnName, TableName},
    sem::{AnalysisError, ColumnRename, traversal::SemanticVisitor, traversal::SemanticVisitorMut},
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

impl ColumnConstraint {
    pub(crate) fn visit_references<V: SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Default(value) => value.expr().visit(visitor),
            Self::Generated(value) => value.expression().visit(visitor),
            Self::Check(value) => value.expr().visit(visitor),
            Self::ForeignKey(value) => value.visit_references(visitor),
            _ => Ok(()),
        }
    }

    pub(crate) fn rename_column(&mut self, rename: &ColumnRename, owned: bool) {
        match self {
            Self::Nullability(value) => value.rename_column(rename, owned),
            Self::Default(value) => value.rename_column(rename, owned),
            Self::Identity(value) => value.rename_column(rename, owned),
            Self::Generated(value) => value.rename_column(rename, owned),
            Self::Check(value) => value.rename_column(rename, owned),
            Self::PrimaryKey(value) => value.rename_column(rename, owned),
            Self::Unique(value) => value.rename_column(rename, owned),
            Self::Exclusion(value) => value.rename_column(rename, owned),
            Self::ForeignKey(value) => value.rename_column(rename, owned),
            Self::AttrDeferrability(value) => value.rename_column(rename, owned),
            Self::AttrDeferred(value) => value.rename_column(rename, owned),
            Self::AttrImmediate(value) => value.rename_column(rename, owned),
            Self::Collate(value) => value.rename_column(rename, owned),
        }
    }

    pub(crate) fn rename_expression(
        &mut self,
        visitor: &mut impl SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Default(value) => value.rename_expression(visitor),
            Self::Generated(value) => value.rename_expression(visitor),
            Self::Check(value) => value.rename_expression(visitor),
            _ => Ok(()),
        }
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        match self {
            Self::Nullability(value) => value.rename_relation(old, new),
            Self::Default(value) => value.rename_relation(old, new),
            Self::Identity(value) => value.rename_relation(old, new),
            Self::Generated(value) => value.rename_relation(old, new),
            Self::Check(value) => value.rename_relation(old, new),
            Self::PrimaryKey(value) => value.rename_relation(old, new),
            Self::Unique(value) => value.rename_relation(old, new),
            Self::Exclusion(value) => value.rename_relation(old, new),
            Self::ForeignKey(value) => value.rename_relation(old, new),
            Self::AttrDeferrability(value) => value.rename_relation(old, new),
            Self::AttrDeferred(value) => value.rename_relation(old, new),
            Self::AttrImmediate(value) => value.rename_relation(old, new),
            Self::Collate(value) => value.rename_relation(old, new),
        }
    }
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
    use crate::{
        ast::{GeneratedColumnMode, IdentityColumnMode},
        sem::{
            AttrDeferrabilityColumnConstraint, AttrDeferredColumnConstraint,
            AttrImmediateColumnConstraint, CheckColumnConstraint, CollateColumnConstraint,
            ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, DefaultColumnConstraint,
            ExclusionColumnConstraint, ForeignKeyColumnConstraint, GeneratedColumnConstraint,
            IdentityColumnConstraint, PrimaryKeyColumnConstraint, UniqueColumnConstraint,
            column_constraints::ColumnNullabilityConstraint,
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
            None,
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
