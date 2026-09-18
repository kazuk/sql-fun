mod column_not_found;
use miette::GraphicalReportHandler;

use super::{AnalysisProblem, Problem, RenderProblemReport};

use crate::{
    StringSpan,
    sem::{
        AnalysisError, CteName, FullName, TableName, TypeReference,
        create_table::{ColumnName, ViewName},
        data_source::AliasName,
    },
};

/// Domain base type name not found
#[derive(Debug, Clone)]
pub struct DomainBaseTypeNameNotFound {
    domain_type_name: FullName,
    base_type_name: String,
    base_type_name_span: StringSpan,
}

impl DomainBaseTypeNameNotFound {
    /// create instance
    pub fn new(
        domain_type_name: &FullName,
        base_type_name: &str,
        base_type_name_span: &StringSpan,
    ) -> Self {
        Self {
            domain_type_name: domain_type_name.clone(),
            base_type_name: base_type_name.to_string(),
            base_type_name_span: *base_type_name_span,
        }
    }

    /// domain type name
    pub const fn domain_type_name(&self) -> &FullName {
        &self.domain_type_name
    }

    /// base type name
    pub fn base_type_name(&self) -> &str {
        &self.base_type_name
    }
}

impl Problem for DomainBaseTypeNameNotFound {
    const CODE: &'static str = "SF000001";
    const SEVERITY: miette::Severity = miette::Severity::Error;
    const MESSAGE: &'static str = "Domain base type not found";
    const SPAN_LABEL: &'static str = "base type name";

    fn get_problem_span(&self) -> StringSpan {
        self.base_type_name_span
    }
}

impl RenderProblemReport for DomainBaseTypeNameNotFound {}

#[derive(Debug, Clone)]
pub struct FunctionNotFound {
    function_name: FullName,
    function_span: StringSpan,
}

impl FunctionNotFound {
    /// create instance
    pub fn new(function_name: &FullName, function_span: &StringSpan) -> Self {
        Self {
            function_name: function_name.clone(),
            function_span: *function_span,
        }
    }

    /// specified function name
    pub const fn function_name(&self) -> &FullName {
        &self.function_name
    }

    /// span of function name
    pub const fn function_span(&self) -> &StringSpan {
        &self.function_span
    }
}

impl Problem for FunctionNotFound {
    const CODE: &'static str = "SF00002";
    const MESSAGE: &'static str = "Function not found";
    const SEVERITY: miette::Severity = miette::Severity::Error;
    const SPAN_LABEL: &'static str = "function name";

    fn get_problem_span(&self) -> StringSpan {
        self.function_span
    }
}

impl RenderProblemReport for FunctionNotFound {}

/// Name resolution problems
#[derive(Debug, Clone)]
pub enum NameResolution {
    #[doc = include_str!("domain_type_name_not_found.md")]
    DomainTypeNameNotFound(DomainBaseTypeNameNotFound),

    #[doc = include_str!("function_not_found.md")]
    FunctionNotFound(FunctionNotFound),

    /// table function not found
    TableFunctionNotFound(FullName),

    /// column reference not resolved
    UndefinedColumn(String),

    /// type not found in column definition
    ColumnTypeNotFound(TableName, ColumnName, FullName),

    /// column not found in table function
    ColumnNotFoundInTableFunction(AliasName, ColumnName),

    /// column not found in table
    ColumnNotFoundInTable(TableName, ColumnName),

    /// column already exists in table
    ColumnAlreadyExists(TableName, ColumnName),

    /// column not found in CTE
    ColumnNotFoundInCte(CteName, ColumnName),

    /// column not found in subquery
    ColumnNotFoundInSubQuery(AliasName, ColumnName),

    /// column not found in source
    ColumnNotFoundInSource(ColumnName),

    /// column not found in view
    ColumnNotFoundInView(ViewName, ColumnName),

    /// column reference ambiguous
    AmbiguousColumn(ColumnName),

    /// same alias defined
    DuplicateAlias(AliasName),

    /// data source not defined in from clause
    UndefinedAlias(String),

    /// table .0 not found in current context
    RelationNotFound(String),

    /// relation already exists in current context
    RelationAlreadyExists(String),

    /// view not found
    ViewNotFound(ViewName),

    /// index not found
    IndexNotFound(String),

    /// sequence not found
    SequenceNotFound(FullName),

    /// table not found
    TableNotFound(TableName),

    /// composite type field not found
    CompositeTypeFieldNotFound(TypeReference, String),

    /// collation not found
    CollationNotFound(String),
}

impl NameResolution {
    pub(crate) fn render_report(
        &self,
        report_handler: &GraphicalReportHandler,
        source: String,
    ) -> Result<String, AnalysisError> {
        match self {
            Self::DomainTypeNameNotFound(d) => d.generate_report(report_handler, source),
            Self::FunctionNotFound(d) => d.generate_report(report_handler, source),
            _ => todo!(),
        }
    }

    pub(crate) fn duplicate_alias(alias_name: &AliasName) -> Self {
        Self::DuplicateAlias(alias_name.clone())
    }

    pub(crate) fn composite_type_field_not_found(
        type_ref: &TypeReference,
        field_name: &str,
    ) -> Self {
        Self::CompositeTypeFieldNotFound(type_ref.clone(), field_name.to_string())
    }

    pub(crate) fn ambiguous_column(column_name: &ColumnName) -> Self {
        Self::AmbiguousColumn(column_name.clone())
    }

    pub(crate) fn table_not_found(table_name: &TableName) -> Self {
        Self::TableNotFound(table_name.clone())
    }

    pub(crate) fn sequence_not_found(sequence_name: &FullName) -> Self {
        Self::SequenceNotFound(sequence_name.clone())
    }

    pub(crate) fn index_not_found(index_name: &str) -> Self {
        Self::IndexNotFound(index_name.to_string())
    }

    pub(crate) fn view_not_found(view_name: &ViewName) -> Self {
        Self::ViewNotFound(view_name.clone())
    }

    /// domain not found
    #[tracing::instrument()]
    pub fn domain_type_name_not_found(
        domain_name: &FullName,
        type_name: &crate::syn::TypeName,
        type_name_span: &StringSpan,
    ) -> Self {
        let problem = DomainBaseTypeNameNotFound::new(
            domain_name,
            type_name.full_name().as_str(),
            type_name_span,
        );

        Self::DomainTypeNameNotFound(problem)
    }

    /// specified table not found
    #[tracing::instrument()]
    pub fn relation_not_found(table_name: &TableName) -> Self {
        Self::RelationNotFound(table_name.to_string())
    }

    /// specified relation already exists
    #[tracing::instrument()]
    pub fn relation_already_exists(table_name: &TableName) -> Self {
        Self::RelationAlreadyExists(table_name.to_string())
    }

    /// calling function not found
    pub fn function_not_found(func_name: &FullName, func_span: &StringSpan) -> Self {
        Self::FunctionNotFound(FunctionNotFound::new(func_name, func_span))
    }

    /// specified table function not found
    pub fn table_function_not_found(call: &FullName) -> Self {
        Self::TableFunctionNotFound(call.clone())
    }

    /// table column type
    #[tracing::instrument()]
    pub fn column_type_not_found(
        table_name: &TableName,
        colname: &ColumnName,
        type_name: &FullName,
    ) -> Self {
        Self::ColumnTypeNotFound(table_name.clone(), colname.clone(), type_name.clone())
    }

    /// alias name undefined
    pub fn undefined_alias(alias_name: &str) -> Self {
        Self::UndefinedAlias(alias_name.to_owned())
    }

    /// column already exists in table
    pub fn column_already_exists(table_name: &TableName, col_name: &ColumnName) -> Self {
        Self::ColumnAlreadyExists(table_name.clone(), col_name.clone())
    }

    /// collation name not found
    pub fn collation_not_found(collation_name: &str) -> Self {
        Self::CollationNotFound(String::from(collation_name))
    }
}

impl AnalysisProblem {
    /// alias name duplicate in `FROM` clause
    #[tracing::instrument()]
    pub fn duplicate_alias(alias_name: &AliasName) -> Self {
        Self::NameResolution(NameResolution::duplicate_alias(alias_name))
    }

    /// accessing member not found in complex type
    #[track_caller]
    #[tracing::instrument()]
    pub fn composite_type_field_not_found(
        composite_type: &TypeReference,
        field_name: &str,
    ) -> Self {
        tracing::warn!(
            "complex type member not found {composite_type:?} member_name: {field_name}"
        );
        Self::NameResolution(NameResolution::composite_type_field_not_found(
            composite_type,
            field_name,
        ))
    }

    /// same column name existing from clause
    #[tracing::instrument()]
    pub fn ambiguous_column(column: &ColumnName) -> Self {
        Self::NameResolution(NameResolution::ambiguous_column(column))
    }

    /// alter table not found
    #[tracing::instrument()]
    pub fn table_not_found(table_name: &TableName) -> Self {
        Self::NameResolution(NameResolution::table_not_found(table_name))
    }

    /// sequence not found
    #[tracing::instrument()]
    pub fn sequence_not_found(seq_name: &FullName) -> Self {
        Self::NameResolution(NameResolution::sequence_not_found(seq_name))
    }

    /// specified index not found
    #[tracing::instrument()]
    pub fn index_not_found(index_name: &str) -> Self {
        Self::NameResolution(NameResolution::index_not_found(index_name))
    }

    /// specified view not found
    #[track_caller]
    #[tracing::instrument()]
    pub fn view_not_found(view_name: &ViewName) -> Self {
        Self::NameResolution(NameResolution::view_not_found(view_name))
    }

    /// domain not found
    #[tracing::instrument()]
    pub fn domain_type_name_not_found(
        domain_name: &FullName,
        type_name: &crate::syn::TypeName,
        type_name_span: &StringSpan,
    ) -> Self {
        Self::NameResolution(NameResolution::domain_type_name_not_found(
            domain_name,
            type_name,
            type_name_span,
        ))
    }

    /// specified table not found
    #[tracing::instrument()]
    pub fn relation_not_found(table_name: &TableName) -> Self {
        Self::NameResolution(NameResolution::relation_not_found(table_name))
    }

    /// specified relation already exists
    #[tracing::instrument()]
    pub fn relation_already_exists(table_name: &TableName) -> Self {
        Self::NameResolution(NameResolution::relation_already_exists(table_name))
    }

    /// calling function not found
    #[track_caller]
    #[tracing::instrument()]
    pub fn function_not_found(func_name: &FullName, func_span: &StringSpan) -> Self {
        Self::NameResolution(NameResolution::function_not_found(func_name, func_span))
    }

    /// specified table function not found
    #[track_caller]
    #[tracing::instrument()]
    pub fn table_function_not_found(func: &FullName) -> Self {
        Self::NameResolution(NameResolution::table_function_not_found(func))
    }

    /// table column type
    #[tracing::instrument()]
    pub fn column_type_not_found(
        table_name: &TableName,
        colname: &ColumnName,
        type_name: &FullName,
    ) -> Self {
        Self::NameResolution(NameResolution::column_type_not_found(
            table_name, colname, type_name,
        ))
    }

    /// column already exists in table
    #[tracing::instrument()]
    pub fn column_already_exists(table_name: &TableName, col_name: &ColumnName) -> Self {
        Self::NameResolution(NameResolution::column_already_exists(table_name, col_name))
    }

    /// undefined alias
    #[must_use]
    pub fn undefined_alias(alias_name: String) -> Self {
        Self::NameResolution(NameResolution::undefined_alias(&alias_name))
    }

    /// collation name not found
    pub fn collation_name_not_found(collation_name: &str) -> Self {
        Self::NameResolution(NameResolution::collation_not_found(collation_name))
    }
}
