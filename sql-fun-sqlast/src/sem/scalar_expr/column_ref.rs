mod cte_column_ref;
mod invalid_column_ref;
mod output_column_ref;
mod subquery_column_ref;
mod table_column_ref;
mod table_func_col_ref;
mod view_column_ref;

pub use self::{
    cte_column_ref::CteColumnRef,
    invalid_column_ref::{
        UndefinedAliasColumnRef, UnknownSourceColumnRef, UnknownTableFuncColumnRef,
    },
    output_column_ref::OutputColumnRef,
    subquery_column_ref::SubQueryColumnRef,
    table_column_ref::TableColumnRef,
    table_func_col_ref::TableFunctionColumnRef,
    view_column_ref::ViewColumnRef,
};

use crate::sem::{
    AnalysisError, AnalysisProblem, ColumnDefinition, ColumnName, ParseContext, SemScalarExpr,
    TypeReference, scalar_expr::SemScalarExprNode,
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
    /// Reference to a resolved projection position (alias or ordinal).
    OutputColumn(OutputColumnRef),
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
    pub(in crate::sem) fn index_column(name: &ColumnName) -> Self {
        Self::UndefinedColumn(name.clone())
    }

    pub(in crate::sem) fn canonicalize_index_binding(&mut self) {
        if let Self::TableColumn(value) = self {
            *self = Self::index_column(value.get_column_name());
        }
    }

    pub(crate) fn output_column(index: usize, definition: &ColumnDefinition) -> Self {
        Self::OutputColumn(OutputColumnRef::new(index, definition))
    }
    pub(crate) const fn output_index(&self) -> Option<usize> {
        match self {
            Self::OutputColumn(value) => Some(value.index()),
            _ => None,
        }
    }
    pub(super) fn constraint_binding(&self) -> Option<(&str, &ColumnName)> {
        match self {
            Self::CteColumn(value) => {
                Some((value.binding_alias().as_str(), value.get_column_name()))
            }
            Self::SubQueryColumn(value) => {
                Some((value.binding_alias().as_str(), value.get_column_name()))
            }
            _ => None,
        }
    }

    pub(crate) fn cte_column(
        alias: &crate::sem::data_source::AliasName,
        cte_name: &crate::sem::CteName,
        column_name: &ColumnName,
        column_def: Option<&ColumnDefinition>,
        source_nullable: bool,
    ) -> Self {
        Self::CteColumn(CteColumnRef::new(
            alias,
            cte_name,
            column_name,
            column_def,
            source_nullable,
        ))
    }

    /// get column type
    #[must_use]
    pub fn get_type(&self) -> Option<TypeReference> {
        match self {
            Self::OutputColumn(r) => r.get_type(),
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
            Self::OutputColumn(r) => r.is_not_null(),
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
            Self::OutputColumn(r) => r.get_column_def(),
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
            Self::OutputColumn(r) => r.get_column_name(),
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
        Self::undefined_column_resolved(context, column)
    }

    pub(crate) fn undefined_column_resolved<TContext: crate::sem::ParseContext>(
        context: &mut TContext,
        column: &ColumnName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        context.report_analysis_problem(AnalysisProblem::column_not_found_in_source(column))?;
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
        Self::ambiguous_column_resolved(context, column)
    }

    pub(crate) fn ambiguous_column_resolved<TContext: crate::sem::ParseContext>(
        context: &mut TContext,
        column: &ColumnName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        context.report_analysis_problem(AnalysisProblem::ambiguous_column(column))?;
        Ok(SemScalarExpr::ColumnRef(Self::AmbiguousColumnRef(
            column.clone(),
        )))
    }
}
