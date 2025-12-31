use sql_fun_core::IVec;

use crate::sem::{
    AnalysisError, AnalysisProblem, ColumnDefinition, ColumnName, Nullable, OverloadVariant,
    ParseContext, SemScalarExpr, TypeReference, data_source::AliasName,
    scalar_expr::ColumnReferenceExpr,
};

use super::ColumnReference;

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TableFunctionColumnRef {
    alias: AliasName,
    column: ColumnName,
    column_def: Option<ColumnDefinition>,
    table_function: OverloadVariant,
    args: IVec<SemScalarExpr>,
    datasource_nullable: Nullable,
}

impl ColumnReference for TableFunctionColumnRef {
    fn get_type(&self) -> Option<TypeReference> {
        if let Some(col_def) = &self.column_def {
            col_def.get_type()
        } else {
            None
        }
    }

    fn is_not_null(&self) -> Option<bool> {
        if self.datasource_nullable {
            return Some(false);
        }
        let Some(column_def) = &self.column_def else {
            return None;
        };
        column_def.is_not_null()
    }

    fn get_column_def(&self) -> &Option<ColumnDefinition> {
        &self.column_def
    }

    fn get_column_name(&self) -> &ColumnName {
        &self.column
    }
}

impl TableFunctionColumnRef {
    fn new(
        alias: &AliasName,
        column: &ColumnName,
        column_def: Option<&ColumnDefinition>,
        table_function: &OverloadVariant,
        args: &IVec<SemScalarExpr>,
        datasource_nullable: Nullable,
    ) -> Self {
        Self {
            alias: alias.clone(),
            column: column.clone(),
            column_def: column_def.cloned(),
            table_function: table_function.clone(),
            args: args.clone(),
            datasource_nullable,
        }
    }

    pub(crate) fn function_source_ref<TParseContext>(
        mut context: TParseContext,
        alias: &AliasName,
        column: &ColumnName,
        calling_func: &OverloadVariant,
        args: &IVec<SemScalarExpr>,
        nullability: Nullable,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let column_def = calling_func.get_result_column_def(column);
        if column_def.is_none() {
            context.report_problem(AnalysisProblem::column_not_found_in_table_function(
                alias, column,
            ));
        }
        let col_ref = Self::new(
            alias,
            column,
            column_def.as_ref(),
            calling_func,
            args,
            nullability,
        );
        Ok((
            SemScalarExpr::ColumnRef(ColumnReferenceExpr::TableFuncColumn(col_ref)),
            context,
        ))
    }
}

impl ColumnReferenceExpr {
    pub(crate) fn function_source_ref<TParseContext>(
        context: TParseContext,
        alias: &AliasName,
        column: &ColumnName,
        calling_func: &OverloadVariant,
        args: &IVec<SemScalarExpr>,
        nullability: Nullable,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        TableFunctionColumnRef::function_source_ref(
            context,
            alias,
            column,
            calling_func,
            args,
            nullability,
        )
    }
}
