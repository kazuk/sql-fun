mod cte_column_ref;
mod invalid_column_ref;
mod subquery_column_ref;
mod table_column_ref;
mod table_func_col_ref;
mod view_column_ref;

use sql_fun_core::IVec;

pub use self::{
    cte_column_ref::CteColumnRef,
    invalid_column_ref::{
        UndefinedAliasColumnRef, UnknownSourceColumnRef, UnknownTableFuncColumnRef,
    },
    subquery_column_ref::SubQueryColumnRef,
    table_column_ref::TableColumnRef,
    table_func_col_ref::TableFunctionColumnRef,
    view_column_ref::ViewColumnRef,
};

use crate::{
    sem::{
        AnalysisError, AnalysisProblem, ColumnDefinition, ColumnName, ParseContext,
        ScalarAnalysisContext, SemScalarExpr, TypeReference,
        data_source::AliasName,
        scalar_expr::{AnalyzeScalarExpr, SemScalarExprNode},
    },
    syn::{ListOpt, ScanToken},
};

trait ColumnReference {
    fn get_type(&self) -> Option<TypeReference>;
    fn is_not_null(&self) -> Option<bool>;
    fn get_column_def(&self) -> &Option<ColumnDefinition>;
    fn get_column_name(&self) -> &ColumnName;
}

/// Column reference
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ColumnReferenceExpr {
    /// CTE column
    CteColumn(CteColumnRef),
    /// Table column
    TableColumn(TableColumnRef),
    /// View column
    ViewColumn(ViewColumnRef),
    /// table function result column
    TableFuncColumn(TableFunctionColumnRef),
    /// Sub-query column
    SubQueryColumn(SubQueryColumnRef),

    /// referenced alias not defined in from clause
    UndefinedAlias(UndefinedAliasColumnRef),
    /// table or view not found in source definition
    UndefinedTableOrView(UnknownSourceColumnRef),
    /// table function not found in source definition
    UndefinedFunction(UnknownTableFuncColumnRef),
    /// column not found
    UndefinedColumn(ColumnName),
    /// column name ambiguous
    AmbiguousColumnRef(ColumnName),
}

impl SemScalarExprNode for ColumnReferenceExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.get_type()
    }

    fn is_not_null(&self) -> Option<bool> {
        self.is_not_null()
    }
}

impl ColumnReferenceExpr {
    /// get column type
    #[must_use]
    pub fn get_type(&self) -> Option<TypeReference> {
        match self {
            Self::CteColumn(r) => r.get_type(),
            Self::TableColumn(r) => r.get_type(),
            Self::ViewColumn(r) => r.get_type(),
            Self::TableFuncColumn(r) => r.get_type(),
            Self::SubQueryColumn(r) => r.get_type(),
            Self::UndefinedAlias(r) => r.get_type(),
            Self::UndefinedTableOrView(r) => r.get_type(),
            Self::UndefinedFunction(r) => r.get_type(),
            Self::UndefinedColumn(_) => None,
            Self::AmbiguousColumnRef(_) => None,
        }
    }

    /// get column is not null
    #[must_use]
    pub fn is_not_null(&self) -> Option<bool> {
        match self {
            Self::CteColumn(r) => r.is_not_null(),
            Self::TableColumn(r) => r.is_not_null(),
            Self::ViewColumn(r) => r.is_not_null(),
            Self::TableFuncColumn(r) => r.is_not_null(),
            Self::SubQueryColumn(r) => r.is_not_null(),
            Self::UndefinedAlias(r) => r.is_not_null(),
            Self::UndefinedTableOrView(r) => r.is_not_null(),
            Self::UndefinedFunction(r) => r.is_not_null(),
            Self::UndefinedColumn(_) => None,
            Self::AmbiguousColumnRef(_) => None,
        }
    }

    /// get column definition
    #[must_use]
    pub fn get_column_def(&self) -> &Option<ColumnDefinition> {
        match self {
            Self::CteColumn(r) => r.get_column_def(),
            Self::TableColumn(r) => r.get_column_def(),
            Self::ViewColumn(r) => r.get_column_def(),
            Self::TableFuncColumn(r) => r.get_column_def(),
            Self::SubQueryColumn(r) => r.get_column_def(),
            Self::UndefinedAlias(r) => r.get_column_def(),
            Self::UndefinedTableOrView(r) => r.get_column_def(),
            Self::UndefinedFunction(r) => r.get_column_def(),
            Self::UndefinedColumn(_) => &None,
            Self::AmbiguousColumnRef(_) => &None,
        }
    }

    /// get column name
    #[must_use]
    pub fn get_column_name(&self) -> &ColumnName {
        match self {
            Self::CteColumn(r) => r.get_column_name(),
            Self::TableColumn(r) => r.get_column_name(),
            Self::ViewColumn(r) => r.get_column_name(),
            Self::TableFuncColumn(r) => r.get_column_name(),
            Self::SubQueryColumn(r) => r.get_column_name(),
            Self::UndefinedAlias(r) => r.get_column_name(),
            Self::UndefinedTableOrView(r) => r.get_column_name(),
            Self::UndefinedFunction(r) => r.get_column_name(),
            Self::UndefinedColumn(c) => c,
            Self::AmbiguousColumnRef(c) => c,
        }
    }

    /// create undefined column reference instance
    pub fn undefined_column_with_report<TParseContext>(
        context: &mut TParseContext,
        column: &ColumnName,
    ) -> Result<SemScalarExpr, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        context.report_problem(AnalysisProblem::column_not_found_in_source(column))?;
        Ok(SemScalarExpr::ColumnRef(Self::UndefinedColumn(
            column.clone(),
        )))
    }

    /// create ambiguous column reference instance
    pub fn ambiguous_column_with_report<TParseContext>(
        context: &mut TParseContext,
        column: &ColumnName,
    ) -> Result<SemScalarExpr, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        context.report_problem(AnalysisProblem::ambiguous_column(column))?;
        Ok(SemScalarExpr::ColumnRef(Self::AmbiguousColumnRef(
            column.clone(),
        )))
    }
}

impl<TParseContext> AnalyzeScalarExpr<TParseContext, crate::syn::ColumnRef> for ColumnReferenceExpr
where
    TParseContext: ParseContext,
{
    fn analyze_scalar_expr(
        context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        syn: crate::syn::ColumnRef,
        _tokens: &IVec<ScanToken>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        let Some(fields) = syn.get_fields().map(|v| v.as_string()) else {
            AnalysisError::raise_unexpected_none("column_ref.fields")?
        };

        let fields: Vec<String> = fields.iter().map(crate::syn::String::get_sval).collect();
        if fields.len() > 2 {
            AnalysisError::raise_unexpected_input(&format!(
                "unexpected len column_ref.fields {}",
                fields.len()
            ))?;
        }

        if fields.len() == 1 {
            let column = ColumnName::from(fields[0].clone());

            scalar_context.resolve_column_ref(context, None, &column)
        } else if fields.len() == 2 {
            let alias = &fields[0];
            let column = ColumnName::from(fields[1].clone());
            scalar_context.resolve_column_ref(
                context,
                Some(&AliasName::from(alias.as_str())),
                &column,
            )
        } else {
            unreachable!()
        }
    }
}
