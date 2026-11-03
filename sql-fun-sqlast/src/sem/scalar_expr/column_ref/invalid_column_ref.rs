use crate::{
    names::{AliasName, ColumnName},
    sem::{
        AnalysisError, AnalysisProblem, ColumnDefinition, ParseContext, SemScalarExpr,
        TypeReference,
        scalar_expr::{ColumnReferenceExpr, column_ref::ColumnReference},
    },
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UndefinedAliasColumnRef {
    alias: AliasName,
    column: ColumnName,
}

impl ColumnReference for UndefinedAliasColumnRef {
    fn get_type(&self) -> Option<TypeReference> {
        None
    }

    fn is_not_null(&self) -> Option<bool> {
        None
    }

    fn get_column_def(&self) -> &Option<ColumnDefinition> {
        &None
    }

    fn get_column_name(&self) -> &ColumnName {
        &self.column
    }
}

impl UndefinedAliasColumnRef {
    pub(crate) fn unreported(alias: &AliasName, column: &ColumnName) -> SemScalarExpr {
        SemScalarExpr::ColumnRef(ColumnReferenceExpr::UndefinedAlias(Self {
            alias: alias.clone(),
            column: column.clone(),
        }))
    }

    pub(crate) fn new_with_report<TParseContext>(
        context: &mut TParseContext,
        alias: &AliasName,
        column: &ColumnName,
    ) -> Result<SemScalarExpr, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        context.report_analysis_problem(AnalysisProblem::undefined_alias(alias.as_str()))?;

        Ok(Self::unreported(alias, column))
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UnknownSourceColumnRef {
    alias: AliasName,
    column: ColumnName,
}

impl ColumnReference for UnknownSourceColumnRef {
    fn get_type(&self) -> Option<TypeReference> {
        None
    }

    fn is_not_null(&self) -> Option<bool> {
        None
    }

    fn get_column_def(&self) -> &Option<ColumnDefinition> {
        &None
    }

    fn get_column_name(&self) -> &ColumnName {
        &self.column
    }
}

impl UnknownSourceColumnRef {
    #[expect(clippy::new_ret_no_self)]
    pub fn new(alias: &AliasName, column: &ColumnName) -> SemScalarExpr {
        SemScalarExpr::ColumnRef(ColumnReferenceExpr::UndefinedTableOrView(Self {
            alias: alias.clone(),
            column: column.clone(),
        }))
    }
}

impl ColumnReferenceExpr {
    pub(crate) fn unknown_source_ref<'context, TParseContext>(
        context: &'context mut TParseContext,
        alias: &AliasName,
        column: &ColumnName,
    ) -> Result<(SemScalarExpr, &'context mut TParseContext), AnalysisError> {
        Ok((UnknownSourceColumnRef::new(alias, column), context))
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UnknownTableFuncColumnRef {
    alias: AliasName,
    column: ColumnName,
}

impl ColumnReference for UnknownTableFuncColumnRef {
    fn get_type(&self) -> Option<TypeReference> {
        None
    }

    fn is_not_null(&self) -> Option<bool> {
        None
    }

    fn get_column_def(&self) -> &Option<ColumnDefinition> {
        &None
    }

    fn get_column_name(&self) -> &ColumnName {
        &self.column
    }
}

impl UnknownTableFuncColumnRef {
    #[expect(clippy::new_ret_no_self)]
    pub fn new(alias: &AliasName, column: &ColumnName) -> SemScalarExpr {
        SemScalarExpr::ColumnRef(ColumnReferenceExpr::UndefinedFunction(Self {
            alias: alias.clone(),
            column: column.clone(),
        }))
    }
}

impl ColumnReferenceExpr {
    pub(crate) fn unknown_function_source_ref<'context, TParseContext>(
        context: &'context mut TParseContext,
        alias: &AliasName,
        column: &ColumnName,
    ) -> Result<(SemScalarExpr, &'context mut TParseContext), AnalysisError> {
        Ok((UnknownTableFuncColumnRef::new(alias, column), context))
    }
}
