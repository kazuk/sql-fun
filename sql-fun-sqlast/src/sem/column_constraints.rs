use sql_fun_core::IVec;

use crate::{
    sem::{AnalysisError, FromClause, ParseContext, SemScalarExpr, TableName, WithClause},
    syn::{ConstrType, Opt, ScanToken},
};

#[cfg(feature = "sqlite")]
use crate::{SqliteToken, sem::AnalysisProblem, syn::sqlite::ResolveType};

/// column constraints
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub enum ColumnConstraint {
    /// primary key column constraint
    PrimaryKey(PrimaryKeyColumnConstraint),
    /// not null column constraint
    NotNull(ColumnNullableConstraint),
    /// unique column constraint
    Unique(UniqueColumnConstraint),
    /// check column constraint
    Check(CheckColumnConstraint),
    /// default column constraint
    Default(DefaultColumnConstraint),
    /// collate column constraint
    Collate(CollateColumnConstraint),
}

impl ColumnConstraint {
    /// get Default variant
    pub fn as_default(&self) -> Option<&DefaultColumnConstraint> {
        match self {
            Self::Default(c) => Some(c),
            _ => None,
        }
    }

    /// get Collate variant
    pub fn as_collate(&self) -> Option<&CollateColumnConstraint> {
        match self {
            Self::Collate(c) => Some(c),
            _ => None,
        }
    }

    /// get Check variant
    pub fn as_check(&self) -> Option<&CheckColumnConstraint> {
        match self {
            Self::Check(c) => Some(c),
            _ => None,
        }
    }
}

impl ColumnConstraint {
    pub(crate) fn analyze_constraint<TParseContext>(
        mut context: TParseContext,
        table_name: &TableName,
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
            ConstrType::Undefined => todo!(),
            ConstrType::ConstrNull | ConstrType::ConstrNotnull => {
                let (null_constraint, new_context) =
                    ColumnNullableConstraint::analyze_constraint(context, table_name, syn)?;
                context = new_context;
                Self::NotNull(null_constraint)
            }
            ConstrType::ConstrDefault => {
                let (c, new_context) =
                    DefaultColumnConstraint::analyze_constraint(context, table_name, syn, tokens)?;
                context = new_context;
                Self::Default(c)
            }
            ConstrType::ConstrIdentity => todo!(),
            ConstrType::ConstrGenerated => todo!(),
            ConstrType::ConstrCheck => {
                let (c, new_context) =
                    CheckColumnConstraint::analyze_constraint(context, table_name, syn, tokens)?;
                context = new_context;
                Self::Check(c)
            }
            ConstrType::ConstrPrimary => {
                let (pk_constraint, new_context) =
                    PrimaryKeyColumnConstraint::analyze_constraint(context, table_name, syn)?;
                context = new_context;
                Self::PrimaryKey(pk_constraint)
            }
            ConstrType::ConstrUnique => {
                let (uniq_constraint, new_context) =
                    UniqueColumnConstraint::analyze_constraint(context, table_name, syn)?;
                context = new_context;
                Self::Unique(uniq_constraint)
            }
            ConstrType::ConstrExclusion => todo!(),
            ConstrType::ConstrForeign => todo!(),
            ConstrType::ConstrAttrDeferrable => todo!(),
            ConstrType::ConstrAttrNotDeferrable => todo!(),
            ConstrType::ConstrAttrDeferred => todo!(),
            ConstrType::ConstrAttrImmediate => todo!(),
            ConstrType::OptionNone => todo!(),
        };
        Ok((constraint, context))
    }
}

impl ColumnConstraint {
    /// analyze [`crate::syn::sqllite::ColumnConstraint`]
    #[cfg(feature = "sqlite")]
    pub fn analyze_sqlite_column_constraint<TParseContext>(
        mut context: TParseContext,
        table_name: &TableName,
        syn: &crate::syn::sqlite::ColumnConstraint,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        use crate::syn::sqlite::ColumnConstraint;

        let result = match syn {
            ColumnConstraint::PrimaryKey(primary_key_column_constraint) => Self::PrimaryKey(
                PrimaryKeyColumnConstraint::from(primary_key_column_constraint),
            ),
            ColumnConstraint::NotNull(not_null_column_constraint) => {
                Self::NotNull(ColumnNullableConstraint::from(not_null_column_constraint))
            }
            ColumnConstraint::Unique(unique_column_constraint) => {
                Self::Unique(UniqueColumnConstraint::from(unique_column_constraint))
            }
            ColumnConstraint::Check(check_column_constraint) => {
                let (c, new_conext) = CheckColumnConstraint::analyze_constraint_sqlite(
                    context,
                    table_name,
                    check_column_constraint,
                    tokens,
                )?;
                context = new_conext;
                Self::Check(c)
            }
            ColumnConstraint::Default(default_column_constraint) => {
                let (c, new_context) = DefaultColumnConstraint::analyze_constraint_sqlite(
                    context,
                    table_name,
                    default_column_constraint,
                    tokens,
                )?;
                context = new_context;
                Self::Default(c)
            }
            ColumnConstraint::Collate(collate_column_constraint) => {
                let (c, new_context) = CollateColumnConstraint::analyze_constraint_sqlite(
                    context,
                    table_name,
                    collate_column_constraint,
                    tokens,
                )?;
                context = new_context;
                Self::Collate(c)
            }
            ColumnConstraint::ForeignKey(_foreign_key_column_constraint) => {
                AnalysisError::raise_unexpected_input(
                    "sqlite foreign key column constraints are not supported yet",
                )?
            }
            ColumnConstraint::Generated(_generated_column_constraint) => {
                AnalysisError::raise_unexpected_input(
                    "sqlite generated column constraints are not supported yet",
                )?
            }
        };
        Ok((result, context))
    }
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct CollateColumnConstraint {
    collation_name: String,
}

impl CollateColumnConstraint {
    pub fn new(collation_name: &str) -> Self {
        Self {
            collation_name: String::from(collation_name),
        }
    }

    pub fn collation_name(&self) -> &str {
        &self.collation_name
    }
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct DefaultColumnConstraint {
    expr: SemScalarExpr,
}

impl From<SemScalarExpr> for DefaultColumnConstraint {
    fn from(value: SemScalarExpr) -> Self {
        Self { expr: value }
    }
}

impl DefaultColumnConstraint {
    pub fn expr(&self) -> &SemScalarExpr {
        &self.expr
    }
}

impl DefaultColumnConstraint {
    pub fn analyze_constraint<TParseContext>(
        mut context: TParseContext,
        _table_name: &TableName,
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
        Ok((Self { expr }, context))
    }

    #[cfg(feature = "sqlite")]
    pub fn analyze_constraint_sqlite<TParseContext>(
        mut context: TParseContext,
        _table_name: &TableName,
        syn: &crate::syn::sqlite::DefaultColumnConstraint,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (expr, new_context) = SemScalarExpr::analyze_sqlite_scaler_expr(
            context,
            &WithClause::default(),
            &FromClause::default(),
            syn.expr(),
            tokens,
        )?;
        context = new_context;
        Ok((Self { expr }, context))
    }
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct CheckColumnConstraint {
    expr: SemScalarExpr,
}

impl CheckColumnConstraint {
    pub fn expr(&self) -> &SemScalarExpr {
        &self.expr
    }
}

impl CheckColumnConstraint {
    pub fn analyze_constraint<TParseContext>(
        context: TParseContext,
        _table_name: &TableName,
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
        Ok((Self { expr }, new_context))
    }

    #[cfg(feature = "sqlite")]
    pub fn analyze_constraint_sqlite<TParseContext>(
        mut context: TParseContext,
        _table_name: &TableName,
        syn: &crate::syn::sqlite::CheckColumnConstraint,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (expr, new_context) = SemScalarExpr::analyze_sqlite_scaler_expr(
            context,
            &WithClause::default(),
            &FromClause::default(),
            syn.expr(),
            tokens,
        )?;
        context = new_context;
        Ok((Self { expr }, context))
    }
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct UniqueColumnConstraint {
    conflict_clause: Option<ConflictClause>,
}

#[cfg(feature = "sqlite")]
impl From<&crate::syn::sqlite::UniqueColumnConstraint> for UniqueColumnConstraint {
    fn from(value: &crate::syn::sqlite::UniqueColumnConstraint) -> Self {
        Self {
            conflict_clause: value.conflict_clause().map(ConflictClause::from),
        }
    }
}

impl UniqueColumnConstraint {
    pub fn analyze_constraint<TParseContext>(
        context: TParseContext,
        _table_name: &TableName,
        syn: &crate::syn::Constraint,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(contype) = syn.get_contype().as_inner() else {
            AnalysisError::raise_unexpected_none("contraint.contype")?;
            AnalysisError::never()
        };
        if !matches!(contype, ConstrType::ConstrUnique) {
            AnalysisError::raise_unexpected_input("constraint.contype")?;
            AnalysisError::never()
        }

        Ok((
            Self {
                conflict_clause: None,
            },
            context,
        ))
    }
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct ColumnNullableConstraint {
    is_not_null: bool,
    conflict_clause: Option<ConflictClause>,
    validity: Option<Validity>,
}

#[cfg(feature = "sqlite")]
impl From<&crate::syn::sqlite::NotNullColumnConstraint> for ColumnNullableConstraint {
    fn from(value: &crate::syn::sqlite::NotNullColumnConstraint) -> Self {
        Self {
            is_not_null: !value.nullable(),
            conflict_clause: value.conflict_clause().map(ConflictClause::from),
            validity: None,
        }
    }
}

impl ColumnNullableConstraint {
    pub fn is_not_null(&self) -> bool {
        self.is_not_null
    }
}

impl ColumnNullableConstraint {
    pub fn analyze_constraint<TParseContext>(
        context: TParseContext,
        _table_name: &TableName,
        syn: &crate::syn::Constraint,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(contype) = syn.get_contype().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.contype")?;
            AnalysisError::never()
        };

        let not_null = if matches!(contype, ConstrType::ConstrNotnull) {
            true
        } else if matches!(contype, ConstrType::ConstrNull) {
            false
        } else {
            AnalysisError::raise_unexpected_input("constraint.contype")?;
            AnalysisError::never()
        };
        Ok((
            Self {
                is_not_null: not_null,
                conflict_clause: None,
                validity: Some(Validity::from_constraint(syn)),
            },
            context,
        ))
    }
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct PrimaryKeyColumnConstraint {
    conflict_clause: Option<ConflictClause>,
    deferrable: Option<Deferrable>,
}

#[cfg(feature = "sqlite")]
impl From<&crate::syn::sqlite::PrimaryKeyColumnConstraint> for PrimaryKeyColumnConstraint {
    fn from(value: &crate::syn::sqlite::PrimaryKeyColumnConstraint) -> Self {
        Self {
            conflict_clause: value.conflict_clause().map(ConflictClause::from),
            deferrable: None,
        }
    }
}

impl PrimaryKeyColumnConstraint {
    pub fn analyze_constraint<TParseContext>(
        context: TParseContext,
        _table_name: &TableName,
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
            Self {
                conflict_clause: None,
                deferrable: Deferrable::try_from_constraint(syn)?,
            },
            context,
        ))
    }
}

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

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct Validity {
    skip_validation: bool,
    initially_valid: bool,
}

impl Validity {
    pub fn from_constraint(syn: &crate::syn::Constraint) -> Self {
        Self {
            skip_validation: syn.get_skip_validation(),
            initially_valid: syn.get_initially_valid(),
        }
    }
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct Deferrable {
    init_deferred: bool,
}

impl Deferrable {
    pub fn try_from_constraint(
        syn: &crate::syn::Constraint,
    ) -> Result<Option<Self>, AnalysisError> {
        let r = if syn.get_deferrable() {
            Some(Self {
                init_deferred: syn.get_initdeferred(),
            })
        } else {
            if syn.get_initdeferred() {
                AnalysisError::raise_unexpected_input("defferable: false, but initdeferred")?;
            }
            None
        };
        Ok(r)
    }
}

#[cfg(feature = "sqlite")]
impl CollateColumnConstraint {
    const COLLATION_NAMES: [&'static str; 3] = ["BINARY", "NOCASE", "RTRIM"];

    pub fn analyze_constraint_sqlite<TParseContext>(
        mut context: TParseContext,
        _table_name: &TableName,
        syn: &crate::syn::sqlite::CollateColumnConstraint,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let collation_name = syn.collation_name().value();
        if !Self::COLLATION_NAMES.contains(&collation_name.to_uppercase().as_str()) {
            context.report_problem(AnalysisProblem::collation_name_not_found(collation_name))?;
        }
        Ok((
            Self {
                collation_name: String::from(collation_name),
            },
            context,
        ))
    }
}
