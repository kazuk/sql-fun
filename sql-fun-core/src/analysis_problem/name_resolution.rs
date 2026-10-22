use super::values::Problem;
use super::{
    AliasName, AnalysisProblem, ColumnName, CteName, DiagnosticMetadata, FullName, StringSpan,
    TableName, TypeName, TypeReference, ViewName,
};

/// Domain base type name not found
#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct DomainBaseTypeNameNotFound {
    domain_type_name: FullName,
    base_type_name: String,
    base_type_name_span: StringSpan,
}

impl DomainBaseTypeNameNotFound {
    /// create instance
    #[must_use]
    pub fn new(
        domain_type_name: impl Into<FullName>,
        base_type_name: &str,
        base_type_name_span: impl Into<StringSpan>,
    ) -> Self {
        Self {
            domain_type_name: domain_type_name.into(),
            base_type_name: base_type_name.to_string(),
            base_type_name_span: base_type_name_span.into(),
        }
    }

    /// domain type name
    #[must_use]
    pub const fn domain_type_name(&self) -> &FullName {
        &self.domain_type_name
    }

    /// base type name
    #[must_use]
    pub fn base_type_name(&self) -> &str {
        &self.base_type_name
    }
}

impl Problem for DomainBaseTypeNameNotFound {
    const CODE: &'static str = "SF000001";

    const MESSAGE: &'static str = "Domain base type not found";
    const SPAN_LABEL: &'static str = "base type name";

    fn get_problem_span(&self) -> StringSpan {
        self.base_type_name_span
    }
}

#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
/// A function name that could not be resolved.
pub struct FunctionNotFound {
    function_name: FullName,
    function_span: StringSpan,
}

impl FunctionNotFound {
    /// create instance
    #[must_use]
    pub fn new(function_name: impl Into<FullName>, function_span: impl Into<StringSpan>) -> Self {
        Self {
            function_name: function_name.into(),
            function_span: function_span.into(),
        }
    }

    /// specified function name
    #[must_use]
    pub const fn function_name(&self) -> &FullName {
        &self.function_name
    }

    /// span of function name
    #[must_use]
    pub const fn function_span(&self) -> &StringSpan {
        &self.function_span
    }
}

impl Problem for FunctionNotFound {
    const CODE: &'static str = "SF00002";
    const MESSAGE: &'static str = "Function not found";

    const SPAN_LABEL: &'static str = "function name";

    fn get_problem_span(&self) -> StringSpan {
        self.function_span
    }
}

/// Name resolution problems
#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
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
    /// Minimum description when a detailed source-label renderer is unavailable.
    pub(super) fn minimum_message(&self) -> String {
        match self {
            Self::DomainTypeNameNotFound(problem) => {
                format!("domain base type not found: {}", problem.base_type_name())
            }
            Self::FunctionNotFound(problem) => {
                format!("function not found: {}", problem.function_name())
            }
            Self::TableFunctionNotFound(name) => format!("table function not found: {name}"),
            Self::UndefinedColumn(name) => format!("column not found: {name}"),
            Self::ColumnTypeNotFound(table, column, ty) => {
                format!("type {ty} of column {table}.{column} not found")
            }
            Self::ColumnNotFoundInTableFunction(alias, column) => {
                format!("column {column} not found in table function {alias}")
            }
            Self::ColumnNotFoundInTable(table, column) => {
                format!("column {column} not found in table {table}")
            }
            Self::ColumnAlreadyExists(table, column) => {
                format!("column {column} already exists in table {table}")
            }
            Self::ColumnNotFoundInCte(cte, column) => {
                format!("column {column} not found in CTE {cte}")
            }
            Self::ColumnNotFoundInSubQuery(alias, column) => {
                format!("column {column} not found in subquery {alias}")
            }
            Self::ColumnNotFoundInSource(column) => {
                format!("column {column} not found in query source")
            }
            Self::ColumnNotFoundInView(view, column) => {
                format!("column {column} not found in view {view}")
            }
            Self::AmbiguousColumn(column) => format!("column reference {column} is ambiguous"),
            Self::DuplicateAlias(alias) => format!("alias {alias} is defined more than once"),
            Self::UndefinedAlias(alias) => format!("alias {alias} is not defined in FROM"),
            Self::RelationNotFound(name) => format!("relation not found: {name}"),
            Self::RelationAlreadyExists(name) => format!("relation already exists: {name}"),
            Self::ViewNotFound(name) => format!("view not found: {name}"),
            Self::IndexNotFound(name) => format!("index not found: {name}"),
            Self::SequenceNotFound(name) => format!("sequence not found: {name}"),
            Self::TableNotFound(name) => format!("table not found: {name}"),
            Self::CompositeTypeFieldNotFound(ty, field) => {
                format!("field {field} not found in composite type {ty}")
            }
            Self::CollationNotFound(name) => format!("collation not found: {name}"),
        }
    }

    /// Stable diagnostic classification, independent of formatted debug output.
    #[must_use]
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::DomainTypeNameNotFound(_) => DomainBaseTypeNameNotFound::CODE,
            Self::FunctionNotFound(_) => FunctionNotFound::CODE,
            Self::TableFunctionNotFound(..) => "table_function_not_found",
            Self::UndefinedColumn(..) => "undefined_column",
            Self::ColumnTypeNotFound(..) => "column_type_not_found",
            Self::ColumnNotFoundInTableFunction(..) => "column_not_found_in_table_function",
            Self::ColumnNotFoundInTable(..) => "column_not_found_in_table",
            Self::ColumnAlreadyExists(..) => "column_already_exists",
            Self::ColumnNotFoundInCte(..) => "column_not_found_in_cte",
            Self::ColumnNotFoundInSubQuery(..) => "column_not_found_in_sub_query",
            Self::ColumnNotFoundInSource(..) => "column_not_found_in_source",
            Self::ColumnNotFoundInView(..) => "column_not_found_in_view",
            Self::AmbiguousColumn(..) => "ambiguous_column",
            Self::DuplicateAlias(..) => "duplicate_alias",
            Self::UndefinedAlias(..) => "undefined_alias",
            Self::RelationNotFound(..) => "relation_not_found",
            Self::RelationAlreadyExists(..) => "relation_already_exists",
            Self::ViewNotFound(..) => "view_not_found",
            Self::IndexNotFound(..) => "index_not_found",
            Self::SequenceNotFound(..) => "sequence_not_found",
            Self::TableNotFound(..) => "table_not_found",
            Self::CompositeTypeFieldNotFound(..) => "composite_type_field_not_found",
            Self::CollationNotFound(..) => "collation_not_found",
        }
    }

    /// Diagnostic statement relative span.
    pub(super) fn statement_relative_span(&self) -> Option<StringSpan> {
        match self {
            Self::DomainTypeNameNotFound(problem) => Some(problem.get_problem_span()),
            Self::FunctionNotFound(problem) => Some(problem.get_problem_span()),
            _ => None,
        }
    }

    /// Diagnostic presentation data.
    pub(super) fn render_metadata(&self) -> Option<DiagnosticMetadata> {
        match self {
            Self::DomainTypeNameNotFound(p) => Some(Problem::render_metadata(p)),
            Self::FunctionNotFound(p) => Some(Problem::render_metadata(p)),
            _ => None,
        }
    }

    /// Diagnostic duplicate alias.
    #[must_use]
    pub fn duplicate_alias(alias_name: impl Into<AliasName>) -> Self {
        Self::DuplicateAlias(alias_name.into())
    }

    /// Diagnostic composite type field not found.
    #[must_use]
    pub fn composite_type_field_not_found(
        type_ref: impl Into<TypeReference>,
        field_name: &str,
    ) -> Self {
        Self::CompositeTypeFieldNotFound(type_ref.into(), field_name.to_string())
    }

    /// Diagnostic ambiguous column.
    #[must_use]
    pub fn ambiguous_column(column_name: impl Into<ColumnName>) -> Self {
        Self::AmbiguousColumn(column_name.into())
    }

    /// Diagnostic table not found.
    #[must_use]
    pub fn table_not_found(table_name: impl Into<TableName>) -> Self {
        Self::TableNotFound(table_name.into())
    }

    /// Diagnostic sequence not found.
    #[must_use]
    pub fn sequence_not_found(sequence_name: impl Into<FullName>) -> Self {
        Self::SequenceNotFound(sequence_name.into())
    }

    /// Diagnostic index not found.
    #[must_use]
    pub fn index_not_found(index_name: &str) -> Self {
        Self::IndexNotFound(index_name.to_string())
    }

    /// Diagnostic view not found.
    #[must_use]
    pub fn view_not_found(view_name: impl Into<ViewName>) -> Self {
        Self::ViewNotFound(view_name.into())
    }

    /// domain not found
    #[must_use]
    pub fn domain_type_name_not_found(
        domain_name: impl Into<FullName>,
        type_name: impl Into<TypeName>,
        type_name_span: impl Into<StringSpan>,
    ) -> Self {
        let problem =
            DomainBaseTypeNameNotFound::new(domain_name, type_name.into().as_str(), type_name_span);

        Self::DomainTypeNameNotFound(problem)
    }

    /// specified table not found
    #[must_use]
    pub fn relation_not_found(table_name: impl Into<TableName>) -> Self {
        Self::RelationNotFound(table_name.into().to_string())
    }

    /// specified relation already exists
    #[must_use]
    pub fn relation_already_exists(table_name: impl Into<TableName>) -> Self {
        Self::RelationAlreadyExists(table_name.into().to_string())
    }

    /// calling function not found
    #[must_use]
    pub fn function_not_found(
        func_name: impl Into<FullName>,
        func_span: impl Into<StringSpan>,
    ) -> Self {
        Self::FunctionNotFound(FunctionNotFound::new(func_name, func_span))
    }

    /// specified table function not found
    #[must_use]
    pub fn table_function_not_found(call: impl Into<FullName>) -> Self {
        Self::TableFunctionNotFound(call.into())
    }

    /// table column type
    #[must_use]
    pub fn column_type_not_found(
        table_name: impl Into<TableName>,
        colname: impl Into<ColumnName>,
        type_name: impl Into<FullName>,
    ) -> Self {
        Self::ColumnTypeNotFound(table_name.into(), colname.into(), type_name.into())
    }

    /// alias name undefined
    #[must_use]
    pub fn undefined_alias(alias_name: &str) -> Self {
        Self::UndefinedAlias(alias_name.to_owned())
    }

    /// column already exists in table
    #[must_use]
    pub fn column_already_exists(
        table_name: impl Into<TableName>,
        col_name: impl Into<ColumnName>,
    ) -> Self {
        Self::ColumnAlreadyExists(table_name.into(), col_name.into())
    }

    /// collation name not found
    #[must_use]
    pub fn collation_not_found(collation_name: &str) -> Self {
        Self::CollationNotFound(String::from(collation_name))
    }
}

impl AnalysisProblem {
    /// alias name duplicate in `FROM` clause
    #[must_use]
    pub fn duplicate_alias(alias_name: impl Into<AliasName>) -> Self {
        Self::NameResolution(NameResolution::duplicate_alias(alias_name))
    }

    /// accessing member not found in complex type
    #[track_caller]
    /// Diagnostic composite type field not found.
    #[must_use]
    pub fn composite_type_field_not_found(
        composite_type: impl Into<TypeReference>,
        field_name: &str,
    ) -> Self {
        let composite_type = composite_type.into();
        tracing::warn!(
            "complex type member not found {composite_type:?} member_name: {field_name}"
        );
        Self::NameResolution(NameResolution::composite_type_field_not_found(
            composite_type,
            field_name,
        ))
    }

    /// same column name existing from clause
    #[must_use]
    pub fn ambiguous_column(column: impl Into<ColumnName>) -> Self {
        Self::NameResolution(NameResolution::ambiguous_column(column))
    }

    /// alter table not found
    #[must_use]
    pub fn table_not_found(table_name: impl Into<TableName>) -> Self {
        Self::NameResolution(NameResolution::table_not_found(table_name))
    }

    /// sequence not found
    #[must_use]
    pub fn sequence_not_found(seq_name: impl Into<FullName>) -> Self {
        Self::NameResolution(NameResolution::sequence_not_found(seq_name))
    }

    /// specified index not found
    #[must_use]
    pub fn index_not_found(index_name: &str) -> Self {
        Self::NameResolution(NameResolution::index_not_found(index_name))
    }

    /// specified view not found
    #[track_caller]
    /// Diagnostic view not found.
    #[must_use]
    pub fn view_not_found(view_name: impl Into<ViewName>) -> Self {
        Self::NameResolution(NameResolution::view_not_found(view_name))
    }

    /// domain not found
    #[must_use]
    pub fn domain_type_name_not_found(
        domain_name: impl Into<FullName>,
        type_name: impl Into<TypeName>,
        type_name_span: impl Into<StringSpan>,
    ) -> Self {
        Self::NameResolution(NameResolution::domain_type_name_not_found(
            domain_name,
            type_name,
            type_name_span,
        ))
    }

    /// Diagnostic domain base type not found.
    #[must_use]
    pub fn domain_base_type_not_found(
        domain_name: impl Into<FullName>,
        base_name: &str,
        span: impl Into<StringSpan>,
    ) -> Self {
        Self::NameResolution(NameResolution::DomainTypeNameNotFound(
            DomainBaseTypeNameNotFound::new(domain_name, base_name, span),
        ))
    }

    /// specified table not found
    #[must_use]
    pub fn relation_not_found(table_name: impl Into<TableName>) -> Self {
        Self::NameResolution(NameResolution::relation_not_found(table_name))
    }

    /// specified relation already exists
    #[must_use]
    pub fn relation_already_exists(table_name: impl Into<TableName>) -> Self {
        Self::NameResolution(NameResolution::relation_already_exists(table_name))
    }

    /// calling function not found
    #[track_caller]
    /// Diagnostic function not found.
    #[must_use]
    pub fn function_not_found(
        func_name: impl Into<FullName>,
        func_span: impl Into<StringSpan>,
    ) -> Self {
        Self::NameResolution(NameResolution::function_not_found(func_name, func_span))
    }

    /// specified table function not found
    #[track_caller]
    /// Diagnostic table function not found.
    #[must_use]
    pub fn table_function_not_found(func: impl Into<FullName>) -> Self {
        Self::NameResolution(NameResolution::table_function_not_found(func))
    }

    /// table column type
    #[must_use]
    pub fn column_type_not_found(
        table_name: impl Into<TableName>,
        colname: impl Into<ColumnName>,
        type_name: impl Into<FullName>,
    ) -> Self {
        Self::NameResolution(NameResolution::column_type_not_found(
            table_name, colname, type_name,
        ))
    }

    /// column already exists in table
    #[must_use]
    pub fn column_already_exists(
        table_name: impl Into<TableName>,
        col_name: impl Into<ColumnName>,
    ) -> Self {
        Self::NameResolution(NameResolution::column_already_exists(table_name, col_name))
    }

    /// undefined alias
    #[must_use]
    pub fn undefined_alias(alias_name: &str) -> Self {
        Self::NameResolution(NameResolution::undefined_alias(alias_name))
    }

    /// collation name not found
    #[must_use]
    pub fn collation_name_not_found(collation_name: &str) -> Self {
        Self::NameResolution(NameResolution::collation_not_found(collation_name))
    }
}

impl NameResolution {
    /// Diagnostic column not found in view.
    #[must_use]
    pub fn column_not_found_in_view(
        view_name: impl Into<ViewName>,
        column: impl Into<ColumnName>,
    ) -> Self {
        Self::ColumnNotFoundInView(view_name.into(), column.into())
    }

    /// Diagnostic column not found in cte.
    #[must_use]
    pub fn column_not_found_in_cte(
        cte_name: impl Into<CteName>,
        column: impl Into<ColumnName>,
    ) -> Self {
        Self::ColumnNotFoundInCte(cte_name.into(), column.into())
    }

    /// Diagnostic column not found in table.
    #[must_use]
    pub fn column_not_found_in_table(
        table_name: impl Into<TableName>,
        column: impl Into<ColumnName>,
    ) -> Self {
        Self::ColumnNotFoundInTable(table_name.into(), column.into())
    }

    /// Diagnostic column not found in subquery.
    #[must_use]
    pub fn column_not_found_in_subquery(
        alias_name: impl Into<AliasName>,
        column: impl Into<ColumnName>,
    ) -> Self {
        Self::ColumnNotFoundInSubQuery(alias_name.into(), column.into())
    }

    /// Diagnostic column not found in source.
    #[must_use]
    pub fn column_not_found_in_source(column: impl Into<ColumnName>) -> Self {
        Self::ColumnNotFoundInSource(column.into())
    }

    /// column not found in table function
    #[must_use]
    pub fn column_not_found_in_table_function(
        alias: impl Into<AliasName>,
        column: impl Into<ColumnName>,
    ) -> Self {
        Self::ColumnNotFoundInTableFunction(alias.into(), column.into())
    }
}

impl AnalysisProblem {
    /// column not found in view
    #[must_use]
    pub fn column_not_found_in_view(
        view_name: impl Into<ViewName>,
        column: impl Into<ColumnName>,
    ) -> Self {
        Self::NameResolution(NameResolution::column_not_found_in_view(view_name, column))
    }

    /// specified column not found in sub query
    #[must_use]
    pub fn column_not_found_in_subquery(
        alias: impl Into<AliasName>,
        column: impl Into<ColumnName>,
    ) -> Self {
        Self::NameResolution(NameResolution::column_not_found_in_subquery(alias, column))
    }

    /// specified column not found in table
    #[must_use]
    pub fn column_not_found_in_table(
        table_name: impl Into<TableName>,
        col_name: impl Into<ColumnName>,
    ) -> Self {
        Self::NameResolution(NameResolution::column_not_found_in_table(
            table_name, col_name,
        ))
    }

    /// specified column not found in CTE
    #[must_use]
    pub fn column_not_found_in_cte(
        cte_name: impl Into<CteName>,
        column: impl Into<ColumnName>,
    ) -> Self {
        Self::NameResolution(NameResolution::column_not_found_in_cte(cte_name, column))
    }

    /// column not found
    #[must_use]
    pub fn column_not_found_in_source(column: impl Into<ColumnName>) -> Self {
        Self::NameResolution(NameResolution::column_not_found_in_source(column))
    }

    /// column not found in table function
    #[must_use]
    pub fn column_not_found_in_table_function(
        alias: impl Into<AliasName>,
        column: impl Into<ColumnName>,
    ) -> Self {
        Self::NameResolution(NameResolution::column_not_found_in_table_function(
            alias, column,
        ))
    }
}
