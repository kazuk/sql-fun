use sql_fun_core::IVec;

#[cfg(feature = "sqlite")]
use crate::SqliteToken;
use crate::{
    sem::{
        AnalysisError, ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ColumnName,
        ParseContext, ScalarAnalysisContext, SemScalarExpr, TableName,
    },
    syn::{ConstrType, Opt, ScanToken},
};

/// column value constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct CheckColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    expr: SemScalarExpr,
}

impl CheckColumnConstraint {
    pub(super) fn rename_expression(
        &mut self,
        visitor: &mut crate::sem::traversal::RelationRenamer<'_>,
    ) -> Result<(), AnalysisError> {
        self.expr.walk(visitor)
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.table_name.rename_reference(old, new);
    }

    /// create instance
    pub fn new(table_name: &TableName, column_name: &ColumnName, expr: SemScalarExpr) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            expr,
        }
    }

    /// get constraint expression
    pub const fn expr(&self) -> &SemScalarExpr {
        &self.expr
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
            &ScalarAnalysisContext::default(),
            check_expr,
            tokens,
        )?;
        Ok((Self::new(table_name, column_name, expr), new_context))
    }

    /// analyze [`crate::syn::sqlite::CheckColumnConstraint`]
    #[cfg(feature = "sqlite")]
    pub fn analyze_constraint_sqlite<TParseContext>(
        mut context: TParseContext,
        table_name: &TableName,
        column_name: &ColumnName,
        syn: &crate::syn::sqlite::CheckColumnConstraint,
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
            ColumnConstraint::check(Self::new(table_name, column_name, expr)),
            context,
        ))
    }
}

impl ColumnConstraint {
    /// create Check variant
    pub const fn check(value: CheckColumnConstraint) -> Self {
        Self::Check(value)
    }

    /// get check variant
    pub const fn as_check(&self) -> Option<&CheckColumnConstraint> {
        if let Self::Check(c) = self {
            Some(c)
        } else {
            None
        }
    }
}
