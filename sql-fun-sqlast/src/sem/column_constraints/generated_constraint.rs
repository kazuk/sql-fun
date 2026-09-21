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

#[cfg(feature = "sqlite")]
impl From<&crate::syn::sqlite::GeneratedColumnType> for GeneratedColumnMode {
    fn from(value: &crate::syn::sqlite::GeneratedColumnType) -> Self {
        match value {
            crate::syn::sqlite::GeneratedColumnType::Stored => Self::Stored,
            crate::syn::sqlite::GeneratedColumnType::Virtual => Self::Virtual,
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

/// generated column / computed column constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct GeneratedColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    expr: SemScalarExpr,
    generated_when: Option<GeneratedColumnMode>,
}

impl GeneratedColumnConstraint {
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
        syn: &crate::syn::sqlite::GeneratedColumnConstraint,
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
