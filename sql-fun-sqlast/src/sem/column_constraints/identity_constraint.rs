use crate::{
    sem::{
        AnalysisError, ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ColumnName,
        ParseContext, TableName,
    },
    syn::{ConstrType, Opt},
};

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

    use super::IdentityColumnMode;

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
    /// Whether explicit values require an OVERRIDING clause.
    pub const fn is_always(&self) -> bool {
        matches!(self.generated_when, IdentityColumnMode::Always)
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
    pub const fn identity(value: IdentityColumnConstraint) -> Self {
        Self::Identity(value)
    }

    /// get identity variant
    pub const fn as_identity(&self) -> Option<&IdentityColumnConstraint> {
        if let Self::Identity(c) = self {
            Some(c)
        } else {
            None
        }
    }
}
