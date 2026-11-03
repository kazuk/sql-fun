use sql_fun_core::IVec;

#[cfg(feature = "sqlite")]
use crate::SqliteToken;
use crate::{
    names::{ColumnName, TableName},
    sem::{
        AnalysisError, ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ParseContext,
        ScalarAnalysisContext, SemScalarExpr,
    },
    syn::{ConstrType, Opt, ScanToken},
};

/// default constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct DefaultColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    expr: SemScalarExpr,
}

impl DefaultColumnConstraint {
    pub(crate) fn rename_column(&mut self, rename: &crate::sem::ColumnRename, owned: bool) {
        if owned {
            rename.rename_name(&mut self.column_name);
        }
    }

    pub(super) fn rename_expression(
        &mut self,
        visitor: &mut impl crate::sem::traversal::SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        self.expr.walk(visitor)
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.table_name.rename_reference(old, new);
    }

    /// get default value expression
    pub const fn expr(&self) -> &SemScalarExpr {
        &self.expr
    }
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
            &ScalarAnalysisContext::default(),
            default_expr,
            tokens,
        )?;
        context = new_context;
        Ok((Self::new(table_name, column_name, expr), context))
    }
}

#[cfg(feature = "sqlite")]
impl DefaultColumnConstraint {
    /// analyze [`crate::syn::sqlite::DefaultColumnConstraint`]
    pub fn analyze_constraint_sqlite<TParseContext>(
        mut context: TParseContext,
        table_name: &TableName,
        column_name: &ColumnName,
        syn: &crate::syn::sqlite::DefaultColumnConstraint,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(ColumnConstraint, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (expr, new_context) = SemScalarExpr::analyze_sqlite_scalar_expr(
            context,
            &ScalarAnalysisContext::default(),
            syn.expr(),
            tokens,
        )?;
        context = new_context;
        Ok((
            ColumnConstraint::default(Self::new(table_name, column_name, expr)),
            context,
        ))
    }
}

impl ColumnConstraint {
    /// create Default variant
    pub const fn default(value: DefaultColumnConstraint) -> Self {
        Self::Default(value)
    }

    /// get Default variant
    pub const fn as_default(&self) -> Option<&DefaultColumnConstraint> {
        if let Self::Default(c) = self {
            Some(c)
        } else {
            None
        }
    }
}
