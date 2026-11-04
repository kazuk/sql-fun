//! Syntax-valued options shared by the common AST and semantic definitions.
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::{DeferSubclause, GeneratedColumnType, RefAct, ResolveType};
use crate::{
    error::AnalysisError,
    syn::{Constraint, Constraint as SynConstraint},
};
/// foreign key match type
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub enum ForeignKeyMatchType {
    /// simple
    Simple,
    /// full
    Full,
    /// partial
    ///
    /// # Note
    ///
    /// partial match type not implemented in PostgreSQL 18, AST value defined but not used.
    ///
    Partial,
}

impl ForeignKeyMatchType {
    /// convert from [`crate::syn::Constraint`]
    pub fn try_from_postgres_constraint(syn: &Constraint) -> Result<Option<Self>, AnalysisError> {
        let value = syn.get_fk_matchtype();
        match value.as_str() {
            "" => Ok(None),
            "s" => Ok(Some(Self::Simple)),
            "f" => Ok(Some(Self::Full)),
            "p" => Ok(Some(Self::Partial)),
            _ => AnalysisError::raise_unexpected_input("fk_matchtype"),
        }
    }
}

/// foreign key update action
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub enum ForeignKeyAction {
    /// no-action
    NoAction,
    /// restrict
    Restrict,
    /// set null value
    SetNull,
    /// set default
    SetDefault,
    /// cascade
    Cascade,
}

impl ForeignKeyAction {
    fn try_from_value(value: &str, source: &str) -> Result<Self, AnalysisError> {
        match value {
            "a" => Ok(Self::NoAction),
            "r" => Ok(Self::Restrict),
            "c" => Ok(Self::Cascade),
            "n" => Ok(Self::SetNull),
            "d" => Ok(Self::SetDefault),
            _ => AnalysisError::raise_unexpected_input(source),
        }
    }

    /// get delete action from [`crate::syn::Constraint`]
    pub fn try_from_delete_postgres_constraint(
        syn: &Constraint,
    ) -> Result<Option<Self>, AnalysisError> {
        let del_action = syn.get_fk_del_action();
        if del_action.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Self::try_from_value(&del_action, "fk_del_action")?))
        }
    }

    /// get update action from [`crate::syn::Constraint`]
    pub fn try_from_update_postgres_constraint(
        syn: &Constraint,
    ) -> Result<Option<Self>, AnalysisError> {
        let upd_action = syn.get_fk_upd_action();
        if upd_action.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Self::try_from_value(&upd_action, "fk_upd_action")?))
        }
    }

    /// convert from [`crate::syn::sqlite::RefAct`]
    #[cfg(feature = "sqlite")]
    pub const fn from_ref_act(value: &RefAct) -> Self {
        match value {
            RefAct::SetNull => Self::SetNull,
            RefAct::SetDefault => Self::SetDefault,
            RefAct::Cascade => Self::Cascade,
            RefAct::Restrict => Self::Restrict,
            RefAct::NoAction => Self::NoAction,
        }
    }
}

/// deferrable / not deferrable, initially deferred
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct Deferrability {
    deferrable: bool,
    init_deferred: bool,
}

impl Deferrability {
    /// Whether the constraint may be deferred.
    pub const fn is_deferrable(&self) -> bool {
        self.deferrable
    }
    /// Whether the constraint is initially deferred.
    pub const fn is_initially_deferred(&self) -> bool {
        self.init_deferred
    }

    pub(crate) const fn new(deferrable: bool, init_deferred: bool) -> Self {
        Self {
            deferrable,
            init_deferred,
        }
    }
    pub(crate) const fn from_postgres_constraint(syn: &Constraint) -> Self {
        let deferrable = syn.get_deferrable();
        let init_deferred = syn.get_initdeferred();
        Self {
            deferrable,
            init_deferred,
        }
    }

    #[cfg(feature = "sqlite")]
    pub(crate) fn from_defer_subclause(syn: &DeferSubclause) -> Self {
        let deferrable = *syn.deferrable();
        let init_deferred = syn.init_deferred().unwrap_or_default().init_deferred();
        Self {
            deferrable,
            init_deferred,
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
    pub const fn stored() -> Self {
        Self::Stored
    }

    /// create Virtual variant
    pub const fn r#virtual() -> Self {
        Self::Virtual
    }

    const GENERATED_COLUMN_MODE_STORED: &'static str = "s";
    const GENERATED_COLUMN_MODE_VIRTUAL: &'static str = "v";

    /// from [`crate::syn::Constraint`] `generated_kind` value
    pub fn from_postgres_generated_column_mode(value: &str) -> Result<Self, AnalysisError> {
        if value == Self::GENERATED_COLUMN_MODE_STORED {
            Ok(Self::stored())
        } else if value == Self::GENERATED_COLUMN_MODE_VIRTUAL {
            Ok(Self::r#virtual())
        } else {
            AnalysisError::raise_unexpected_input("generated_kind")?;
            AnalysisError::never()
        }
    }
}

#[cfg(test)]
mod test_generated_column_mode_from_pg {
    use testresult::TestResult;

    use crate::syntax_options::GeneratedColumnMode;

    #[rstest::rstest]
    #[case("s", GeneratedColumnMode::Stored)]
    #[case("v", GeneratedColumnMode::Virtual)]
    fn test_generated_column_mode(
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

#[cfg(feature = "sqlite")]
impl From<&GeneratedColumnType> for GeneratedColumnMode {
    fn from(value: &GeneratedColumnType) -> Self {
        match value {
            GeneratedColumnType::Stored => Self::Stored,
            GeneratedColumnType::Virtual => Self::Virtual,
        }
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
    pub const fn always() -> Self {
        Self::Always
    }

    /// create ByDefault variant
    pub const fn by_default() -> Self {
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

    use crate::syntax_options::IdentityColumnMode;

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

#[cfg(feature = "sqlite")]
impl From<&ResolveType> for ConflictClause {
    fn from(value: &ResolveType) -> Self {
        match value {
            ResolveType::Rollback => Self::Rollback,
            ResolveType::Abort => Self::Abort,
            ResolveType::Fail => Self::Fail,
            ResolveType::Ignore => Self::Ignore,
            ResolveType::Replace => Self::Replace,
        }
    }
}

/// validity value
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
    pub const fn from_constraint(syn: &SynConstraint) -> Self {
        Self::new(syn.get_skip_validation(), syn.get_initially_valid())
    }
}

/// deferrable constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct Deferrable {
    init_deferred: bool,
}

impl Deferrable {
    pub(crate) const fn is_initially_deferred(&self) -> bool {
        self.init_deferred
    }
    /// create instance
    pub const fn new(init_deferred: bool) -> Self {
        Self { init_deferred }
    }

    /// from [`crate::syn::Constraint`]
    pub fn try_from_constraint(syn: &SynConstraint) -> Result<Option<Self>, AnalysisError> {
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

/// mode of `CREATE TABLE` statement
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CreateTableMode {
    /// create table mode default
    Default,
    /// create table with `IF NOT EXISTS`
    IfNotExists,
    /// create table with `OR REPLACE`
    ///
    /// # Note
    ///
    /// This valiant is not used in now, `PostgreSQL` and `SQLite` not supports `create table or replace`
    OrReplace,
}

impl CreateTableMode {
    pub(crate) const fn if_not_exists(self) -> bool {
        matches!(self, Self::IfNotExists)
    }
    /// create instance for if not exists
    pub const fn from_if_not_exists(if_not_exists: bool) -> Self {
        if if_not_exists {
            Self::IfNotExists
        } else {
            Self::Default
        }
    }
}

/// PostgreSQL object-kind identifier used by catalog DDL operations.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct ObjectKind(u32);

impl ObjectKind {
    pub(crate) const AGGREGATE: Self = Self(2);
    pub(crate) const COLLATION: Self = Self(8);
    pub(crate) const CONVERSION: Self = Self(9);
    pub(crate) const DATABASE: Self = Self(10);
    pub(crate) const DOMAIN: Self = Self(13);
    pub(crate) const EVENT_TRIGGER: Self = Self(15);
    pub(crate) const FOREIGN_DATA_WRAPPER: Self = Self(17);
    pub(crate) const FOREIGN_SERVER: Self = Self(18);
    pub(crate) const FUNCTION: Self = Self(20);
    pub(crate) const LANGUAGE: Self = Self(22);
    pub(crate) const LARGE_OBJECT: Self = Self(23);
    pub(crate) const MATERIALIZED_VIEW: Self = Self(24);
    pub(crate) const OPERATOR_CLASS: Self = Self(25);
    pub(crate) const OPERATOR: Self = Self(26);
    pub(crate) const OPERATOR_FAMILY: Self = Self(27);
    pub(crate) const PROCEDURE: Self = Self(30);
    pub(crate) const PUBLICATION: Self = Self(31);
    pub(crate) const ROUTINE: Self = Self(35);
    pub(crate) const SCHEMA: Self = Self(37);
    pub(crate) const SEQUENCE: Self = Self(38);
    pub(crate) const SUBSCRIPTION: Self = Self(39);
    pub(crate) const STATISTICS: Self = Self(40);
    pub(crate) const TABLE: Self = Self(42);
    pub(crate) const TABLESPACE: Self = Self(43);
    pub(crate) const TEXT_SEARCH_CONFIGURATION: Self = Self(46);
    pub(crate) const TEXT_SEARCH_DICTIONARY: Self = Self(47);
    pub(crate) const TYPE: Self = Self(50);
    pub(crate) const VIEW: Self = Self(52);

    pub(crate) const fn code(&self) -> u32 {
        self.0
    }
}

impl From<u32> for ObjectKind {
    fn from(value: u32) -> Self {
        Self(value)
    }
}
