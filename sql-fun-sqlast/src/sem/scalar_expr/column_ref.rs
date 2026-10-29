mod cte_column_ref;
mod invalid_column_ref;
mod merge_record_column_ref;
mod output_column_ref;
mod qualified_column_failure;
mod subquery_column_ref;
mod system_column_ref;
mod table_column_ref;
mod table_func_col_ref;
mod view_column_ref;

pub use self::{
    cte_column_ref::CteColumnRef,
    invalid_column_ref::{
        UndefinedAliasColumnRef, UnknownSourceColumnRef, UnknownTableFuncColumnRef,
    },
    merge_record_column_ref::MergeRecordColumnRef,
    output_column_ref::OutputColumnRef,
    qualified_column_failure::QualifiedColumnFailure,
    subquery_column_ref::SubQueryColumnRef,
    system_column_ref::SystemColumnRef,
    table_column_ref::TableColumnRef,
    table_func_col_ref::TableFunctionColumnRef,
    view_column_ref::ViewColumnRef,
};

use crate::{
    context::ParseContext,
    sem::{
        AnalysisError, AnalysisProblem, ColumnDefinition, ColumnName, SemScalarExpr, TypeReference,
        scalar_expr::SemScalarExprNode,
    },
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
    /// Failed qualified lookup with its requested identity and reason.
    QualifiedFailure(QualifiedColumnFailure),
    /// PostgreSQL DELETE RETURNING OLD/NEW target-column binding.
    DeleteRow(crate::sem::DeleteRowColumn),
    /// MERGE target, source, or row-image column binding.
    MergeRecord(MergeRecordColumnRef),
    /// Reference to a resolved projection position (alias or ordinal).
    OutputColumn(OutputColumnRef),
    /// CTE column
    CteColumn(CteColumnRef),
    /// Table column
    TableColumn(TableColumnRef),
    /// PostgreSQL system attribute, separate from declared table columns.
    SystemColumn(SystemColumnRef),
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
    pub(crate) fn matches_column(&self, rename: &crate::sem::ColumnRename) -> bool {
        if let Self::TableColumn(value) = self {
            value.matches_column(rename)
        } else {
            false
        }
    }

    pub(crate) const fn namespace_reference(&self) -> Option<(&str, &ColumnName, bool)> {
        match self {
            Self::TableColumn(value) => Some(value.namespace_reference()),
            Self::ViewColumn(value) => Some(value.namespace_reference()),
            Self::SubQueryColumn(value) => Some(value.namespace_reference()),
            Self::CteColumn(value) => Some(value.namespace_reference()),
            _ => None,
        }
    }

    pub(super) const fn mark_unqualified(&mut self) {
        match self {
            Self::TableColumn(value) => value.mark_unqualified(),
            Self::ViewColumn(value) => value.mark_unqualified(),
            Self::SubQueryColumn(value) => value.mark_unqualified(),
            Self::CteColumn(value) => value.mark_unqualified(),
            _ => {}
        }
    }
    pub(crate) fn renamed_output(&self, rename: &crate::sem::ColumnRename) -> Option<&ColumnName> {
        if let Self::TableColumn(value) = self {
            value.renamed_output(rename)
        } else {
            None
        }
    }

    pub(crate) fn with_derived_column_name(self, derived: bool) -> Self {
        if let Self::TableColumn(value) = self {
            Self::TableColumn(value.with_derived_name(derived))
        } else {
            self
        }
    }

    pub(in crate::sem) fn set_projection_name(&mut self, name: &ColumnName) {
        match self {
            Self::TableColumn(value) => value.set_projection_name(name),
            Self::ViewColumn(value) => value.set_projection_name(name),
            Self::SubQueryColumn(value) => value.set_projection_name(name),
            Self::CteColumn(value) => value.set_projection_name(name),
            _ => {}
        }
    }
    pub(in crate::sem) fn rename_column(&mut self, rename: &crate::sem::ColumnRename) {
        if let Self::TableColumn(value) = self {
            value.rename_column(rename);
        }
    }

    pub(in crate::sem) fn rename_relation(
        &mut self,
        old: &crate::sem::TableName,
        new: &crate::sem::TableName,
    ) {
        match self {
            Self::TableColumn(value) => value.rename_relation(old, new),
            Self::SystemColumn(value) => value.rename_relation(old, new),
            _ => {}
        }
    }

    /// SQL source spelling retained by a completed projection binding.
    pub(crate) const fn projection_reference(&self) -> Option<(&str, &ColumnName)> {
        match self {
            Self::TableColumn(value) => value.projection_reference(),
            Self::SystemColumn(value) => value.projection_reference(),
            Self::ViewColumn(value) => value.projection_reference(),
            Self::TableFuncColumn(value) => value.projection_reference(),
            Self::CteColumn(value) => value.projection_reference(),
            Self::SubQueryColumn(value) => value.projection_reference(),
            _ => None,
        }
    }

    pub(super) fn with_reference_name(
        self,
        alias: &crate::sem::AliasName,
        column: &ColumnName,
    ) -> Self {
        match self {
            Self::MergeRecord(value) => Self::MergeRecord(value.with_reference_name(alias, column)),
            Self::TableColumn(value) => Self::TableColumn(value.with_reference_name(alias, column)),
            Self::SystemColumn(value) => {
                Self::SystemColumn(value.with_reference_name(alias, column))
            }
            Self::ViewColumn(value) => Self::ViewColumn(value.with_reference_name(alias, column)),
            Self::CteColumn(value) => Self::CteColumn(value.with_reference_name(alias, column)),
            Self::SubQueryColumn(value) => {
                Self::SubQueryColumn(value.with_reference_name(alias, column))
            }
            Self::TableFuncColumn(value) => {
                Self::TableFuncColumn(value.with_reference_name(alias, column))
            }
            _ => self,
        }
    }

    pub(crate) fn qualified_failure(failure: &QualifiedColumnFailure) -> Self {
        Self::QualifiedFailure(failure.clone())
    }
    /// Completed qualification failure, when this column did not bind a source.
    pub const fn qualification_failure(&self) -> Option<&QualifiedColumnFailure> {
        if let Self::QualifiedFailure(failure) = self {
            Some(failure)
        } else {
            None
        }
    }
    pub(super) const fn grouping_reference_resolved(&self) -> bool {
        !matches!(
            self,
            Self::QualifiedFailure(_)
                | Self::UndefinedAlias(_)
                | Self::UndefinedTableOrView(_)
                | Self::UndefinedFunction(_)
                | Self::UndefinedColumn(_)
                | Self::AmbiguousColumnRef(_)
        )
    }
    pub(super) fn canonicalize_grouping_key(&mut self) {
        match self {
            Self::CteColumn(value) => value.canonicalize_grouping_key(),
            Self::TableColumn(value) => value.canonicalize_grouping_key(),
            Self::ViewColumn(value) => value.canonicalize_grouping_key(),
            Self::SubQueryColumn(value) => value.canonicalize_grouping_key(),
            Self::TableFuncColumn(value) => value.canonicalize_grouping_key(),
            _ => {}
        }
    }

    /// Resolved table-column binding.
    pub const fn table_column_reference(&self) -> Option<&TableColumnRef> {
        if let Self::TableColumn(reference) = self {
            Some(reference)
        } else {
            None
        }
    }
    /// Resolved PostgreSQL system-attribute binding.
    pub const fn system_column_reference(&self) -> Option<&SystemColumnRef> {
        if let Self::SystemColumn(reference) = self {
            Some(reference)
        } else {
            None
        }
    }

    pub(crate) fn system_column(
        alias: &crate::sem::AliasName,
        column: &ColumnName,
        table: &crate::sem::TableName,
        column_type: &TypeReference,
        nullable: bool,
    ) -> Self {
        Self::SystemColumn(SystemColumnRef::new(
            alias,
            column,
            table,
            column_type,
            nullable,
        ))
    }

    /// MERGE record binding, when this reference belongs to a MERGE record.
    pub const fn merge_record_reference(&self) -> Option<&MergeRecordColumnRef> {
        if let Self::MergeRecord(reference) = self {
            Some(reference)
        } else {
            None
        }
    }

    pub(crate) fn undefined_alias_resolved<C: ParseContext>(
        context: &mut C,
        alias: &crate::sem::data_source::AliasName,
        column: &ColumnName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        UndefinedAliasColumnRef::new_with_report(context, alias, column)
    }

    pub(crate) fn merge_record(
        role: crate::sem::data_source::MergeRecordRole,
        source: &Self,
        nullable: bool,
    ) -> Self {
        let source = match source {
            Self::MergeRecord(reference) => reference.source(),
            _ => source,
        };
        Self::MergeRecord(MergeRecordColumnRef::new(role, source, nullable))
    }

    pub(crate) const fn delete_row_reference(&self) -> Option<&crate::sem::DeleteRowColumn> {
        match self {
            Self::DeleteRow(column) => Some(column),
            _ => None,
        }
    }

    pub(crate) fn bound_view_column(&self, view: &crate::sem::FullName) -> Option<&ColumnName> {
        match self {
            Self::TableColumn(column) if column.belongs_to(view) => Some(column.get_column_name()),
            Self::ViewColumn(column) if column.belongs_to(view) => Some(column.get_column_name()),
            _ => None,
        }
    }

    pub(crate) fn delete_row(version: crate::sem::DeleteRowVersion, source: &Self) -> Self {
        Self::DeleteRow(crate::sem::DeleteRowColumn::new(version, source))
    }

    pub(crate) fn view_source_column(&self) -> Option<&ColumnName> {
        match self {
            Self::TableColumn(value) => value.get_column_def().as_ref()?.name().as_ref(),
            Self::ViewColumn(value) => value.get_column_def().as_ref()?.name().as_ref(),
            _ => None,
        }
    }

    pub(crate) fn table_column(
        alias: &crate::sem::AliasName,
        column: &ColumnName,
        table: &crate::sem::TableName,
        definition: Option<&ColumnDefinition>,
        nullable: bool,
    ) -> Self {
        Self::TableColumn(TableColumnRef::new(
            alias, column, table, definition, nullable,
        ))
    }

    pub(in crate::sem) fn index_column_name(&self) -> Option<&ColumnName> {
        if let Self::TableColumn(value) = self {
            Some(value.get_column_name())
        } else {
            None
        }
    }

    pub(in crate::sem) fn canonicalize_index_binding(&mut self) {
        if let Self::TableColumn(value) = self {
            value.canonicalize_alias();
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
            Self::MergeRecord(value) => value.source().constraint_binding(),
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
            Self::DeleteRow(value) => value.column_type(),
            Self::MergeRecord(value) => value.column_type(),
            Self::OutputColumn(r) => r.get_type(),
            Self::CteColumn(r) => r.get_type(),
            Self::TableColumn(r) => r.get_type(),
            Self::SystemColumn(r) => r.get_type(),
            Self::ViewColumn(r) => r.get_type(),
            Self::TableFuncColumn(r) => r.get_type(),
            Self::SubQueryColumn(r) => r.get_type(),
            Self::UndefinedAlias(r) => r.get_type(),
            Self::UndefinedTableOrView(r) => r.get_type(),
            Self::UndefinedFunction(r) => r.get_type(),
            Self::UndefinedColumn(_) => None,
            Self::AmbiguousColumnRef(_) | Self::QualifiedFailure(_) => None,
        }
    }

    /// get column is not null
    #[must_use]
    pub fn is_not_null(&self) -> Option<bool> {
        match self {
            Self::DeleteRow(value) => value.is_not_null(),
            Self::MergeRecord(value) => value.is_not_null(),
            Self::OutputColumn(r) => r.is_not_null(),
            Self::CteColumn(r) => r.is_not_null(),
            Self::TableColumn(r) => r.is_not_null(),
            Self::SystemColumn(r) => r.is_not_null(),
            Self::ViewColumn(r) => r.is_not_null(),
            Self::TableFuncColumn(r) => r.is_not_null(),
            Self::SubQueryColumn(r) => r.is_not_null(),
            Self::UndefinedAlias(r) => r.is_not_null(),
            Self::UndefinedTableOrView(r) => r.is_not_null(),
            Self::UndefinedFunction(r) => r.is_not_null(),
            Self::UndefinedColumn(_) => None,
            Self::AmbiguousColumnRef(_) | Self::QualifiedFailure(_) => None,
        }
    }

    /// get column definition
    #[must_use]
    pub fn get_column_def(&self) -> &Option<ColumnDefinition> {
        match self {
            Self::DeleteRow(value) => value.column_definition(),
            Self::MergeRecord(value) => value.column_definition(),
            Self::OutputColumn(r) => r.get_column_def(),
            Self::CteColumn(r) => r.get_column_def(),
            Self::TableColumn(r) => r.get_column_def(),
            Self::SystemColumn(r) => r.get_column_def(),
            Self::ViewColumn(r) => r.get_column_def(),
            Self::TableFuncColumn(r) => r.get_column_def(),
            Self::SubQueryColumn(r) => r.get_column_def(),
            Self::UndefinedAlias(r) => r.get_column_def(),
            Self::UndefinedTableOrView(r) => r.get_column_def(),
            Self::UndefinedFunction(r) => r.get_column_def(),
            Self::UndefinedColumn(_) => &None,
            Self::AmbiguousColumnRef(_) | Self::QualifiedFailure(_) => &None,
        }
    }

    /// get column name
    #[must_use]
    pub fn get_column_name(&self) -> &ColumnName {
        match self {
            Self::DeleteRow(value) => value.column_name(),
            Self::MergeRecord(value) => value.column_name(),
            Self::OutputColumn(r) => r.get_column_name(),
            Self::CteColumn(r) => r.get_column_name(),
            Self::TableColumn(r) => r.get_column_name(),
            Self::SystemColumn(r) => r.get_column_name(),
            Self::ViewColumn(r) => r.get_column_name(),
            Self::TableFuncColumn(r) => r.get_column_name(),
            Self::SubQueryColumn(r) => r.get_column_name(),
            Self::UndefinedAlias(r) => r.get_column_name(),
            Self::UndefinedTableOrView(r) => r.get_column_name(),
            Self::UndefinedFunction(r) => r.get_column_name(),
            Self::UndefinedColumn(c) => c,
            Self::AmbiguousColumnRef(c) => c,
            Self::QualifiedFailure(failure) => failure.column(),
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

    pub(crate) fn undefined_column_resolved<TContext: ParseContext>(
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

    pub(crate) fn ambiguous_column_resolved<TContext: ParseContext>(
        context: &mut TContext,
        column: &ColumnName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        context.report_analysis_problem(AnalysisProblem::ambiguous_column(column))?;
        Ok(SemScalarExpr::ColumnRef(Self::AmbiguousColumnRef(
            column.clone(),
        )))
    }
}
