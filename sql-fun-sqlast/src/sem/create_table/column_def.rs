use sql_fun_core::IVec;

use crate::{
    sem::{
        AnalysisError, AnalysisProblem, ColumnName, FromClause, FullName, ParseContext,
        SemScalarExpr, TableName, TypeReference, WithClause,
    },
    syn::{ConstrType, ListOpt, Opt, ScanToken},
};

/// column definitions
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColumnDefinition {
    name: Option<ColumnName>,
    type_ref: Option<TypeReference>,
    #[deprecated = "query from constraints"]
    is_not_null: Option<bool>,
    #[deprecated = "query from constraints"]
    default: Option<Box<SemScalarExpr>>,
    constraints: Vec<ColumnConstraint>,
}

impl Clone for ColumnDefinition {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            type_ref: self.type_ref.clone(),
            #[expect(deprecated)]
            is_not_null: self.is_not_null,
            #[expect(deprecated)]
            default: self.default.clone(),
            constraints: self.constraints.clone(),
        }
    }
}

impl ColumnDefinition {
    /// create instance
    #[must_use]
    pub fn new(
        name: &Option<ColumnName>,
        type_ref: Option<&TypeReference>,
        is_not_null: Option<bool>,
        constraints: &[ColumnConstraint],
    ) -> Self {
        Self {
            name: name.clone(),
            type_ref: type_ref.cloned(),
            #[expect(deprecated)]
            is_not_null,
            #[expect(deprecated)]
            default: Default::default(),
            constraints: constraints.to_vec(),
        }
    }

    /// get name of column
    #[must_use]
    pub fn name(&self) -> &Option<ColumnName> {
        &self.name
    }

    /// get [`TypeReference`]
    #[must_use]
    pub fn get_type(&self) -> Option<TypeReference> {
        self.type_ref.clone()
    }

    /// column nullability
    #[must_use]
    pub fn is_not_null(&self) -> Option<bool> {
        #[expect(deprecated)]
        self.is_not_null
    }

    /// apply default expression to column
    #[expect(deprecated)]
    pub fn set_default(&mut self, scalar_expr: &SemScalarExpr) {
        self.default = Some(Box::new(scalar_expr.clone()));
    }

    /// apply not null constrant to column
    pub fn set_not_null(&mut self, not_null: Option<bool>) {
        self.is_not_null = not_null;
    }

    /// get column constraints
    #[must_use]
    pub fn constraints(&self) -> &[ColumnConstraint] {
        self.constraints.as_slice()
    }

    /// add column constraint
    pub fn push_constraint(&mut self, constraint: ColumnConstraint) {
        self.constraints.push(constraint)
    }

    /// analyze [`crate::syn::ColumnDef`]
    pub fn analyze_column_def<TParseContext>(
        mut context: TParseContext,
        table_name: &TableName,
        column_def: &crate::syn::ColumnDef,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let colname = column_def.get_colname();
        let colname = ColumnName::new(&colname);
        let Some(type_name) = column_def.get_type_name().as_inner() else {
            AnalysisError::raise_unexpected_none("column_def.get_type_name")?
        };
        let is_not_null = column_def.get_is_not_null();

        let key = FullName::try_from(type_name)?;
        let type_ref = if let Some(sem_type) = context.get_type(&key).cloned() {
            if let Some(type_ref) = sem_type.type_reference().cloned() {
                Some(type_ref)
            } else {
                context.report_problem(AnalysisProblem::unexpected_dynamic_type(&sem_type))?;
                None
            }
        } else {
            context.report_problem(AnalysisProblem::column_type_not_found(
                table_name, &colname, &key,
            ))?;
            None
        };

        let constraints = if let Some(constraint_list) =
            column_def.get_constraints().map(|v| v.as_constraint())
        {
            let mut constraints = Vec::new();
            for constraint in constraint_list {
                let (c, new_context) =
                    ColumnConstraint::analyze_constraint(context, table_name, &constraint, tokens)?;
                context = new_context;
                constraints.push(c)
            }
            constraints
        } else {
            Vec::default()
        };
        let column = Self::new(
            &Some(colname),
            type_ref.as_ref(),
            Some(is_not_null),
            &constraints,
        );
        Ok((column, context))
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
pub struct PrimaryKeyColumnConstraint {
    conflict_clause: Option<ConflictClause>,
    deferrable: Option<Deferrable>,
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
pub struct ColumnNullableConstraint {
    is_not_null: bool,
    conflict_clause: Option<ConflictClause>,
    validity: Option<Validity>,
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
pub struct UniqueColumnConstraint {
    conflict_clause: Option<ConflictClause>,
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
pub struct CheckColumnConstraint {
    expr: SemScalarExpr,
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
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct DefaultColumnConstraint {
    expr: SemScalarExpr,
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
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct CollateColumnConstraint {
    collation_name: String,
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub enum ColumnConstraint {
    PrimaryKey(PrimaryKeyColumnConstraint),
    NotNull(ColumnNullableConstraint),
    Unique(UniqueColumnConstraint),
    Check(CheckColumnConstraint),
    Default(DefaultColumnConstraint),
    Collate(CollateColumnConstraint),
}

impl ColumnConstraint {
    pub fn analyze_constraint<TParseContext>(
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
