#[cfg(feature = "sqlite")]
use crate::syn::sqlite::GeneratedColumnConstraint as SqliteGeneratedColumnConstraint;
pub use crate::syntax_options::GeneratedColumnMode;
use crate::{
    sem::{ColumnRename, traversal::SemanticVisitorMut},
    syn::Constraint,
};
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

/// generated column / computed column constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct GeneratedColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    expr: SemScalarExpr,
    generated_when: Option<GeneratedColumnMode>,
}

impl GeneratedColumnConstraint {
    pub(crate) fn rename_column(&mut self, rename: &ColumnRename, owned: bool) {
        if owned {
            rename.rename_name(&mut self.column_name);
        }
    }

    pub(super) fn rename_expression(
        &mut self,
        visitor: &mut impl SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        self.expr.walk(visitor)
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.table_name.rename_reference(old, new);
    }

    pub(crate) fn is_stored(&self) -> bool {
        self.generated_when == Some(GeneratedColumnMode::stored())
    }
    pub(crate) fn is_virtual(&self) -> bool {
        self.generated_when == Some(GeneratedColumnMode::r#virtual())
    }
    /// create instance
    pub fn new(
        table_name: &TableName,
        column_name: &ColumnName,
        expr: SemScalarExpr,
        generated_when: Option<GeneratedColumnMode>,
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
        syn: &Constraint,
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
            GeneratedColumnMode::from_postgres_generated_column_mode(&syn.get_generated_kind())?;
        let (expr, new_context) = SemScalarExpr::analyze_scaler_expr(
            context,
            &ScalarAnalysisContext::default(),
            raw_expr,
            tokens,
        )?;
        context = new_context;
        Ok((
            Self::new(table_name, column_name, expr, Some(generated_when)),
            context,
        ))
    }
}

#[cfg(feature = "sqlite")]
impl GeneratedColumnConstraint {
    /// analyze [`crate::syn::sqlite::GeneratedColumnConstraint`]
    pub fn analyze_constraint_sqlite<TParseContext>(
        mut context: TParseContext,
        table_name: &TableName,
        column_name: &ColumnName,
        syn: &SqliteGeneratedColumnConstraint,
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
        let generated_when = syn.typ().as_ref().map(GeneratedColumnMode::from);
        Ok((
            ColumnConstraint::generated(Self::new(table_name, column_name, expr, generated_when)),
            context,
        ))
    }
}

impl ColumnConstraint {
    /// create Generated variant
    pub const fn generated(value: GeneratedColumnConstraint) -> Self {
        Self::Generated(value)
    }

    /// get Generated variant
    pub const fn as_generated(&self) -> Option<&GeneratedColumnConstraint> {
        if let Self::Generated(c) = self {
            Some(c)
        } else {
            None
        }
    }
}
