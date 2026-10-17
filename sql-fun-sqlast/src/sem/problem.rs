#![doc= include_str!("problem.md")]

mod insert;
mod name_resolution;
mod overload_resolution;
mod parameter_constraint;
mod query;
mod sql_highlighter;

use std::collections::HashSet;

use miette::{GraphicalReportHandler, LabeledSpan, NamedSource, Report};
use sql_fun_core::HighlighterTheme;

use crate::{
    StringSpan,
    sem::{
        AnalysisError, ColumnName, DefElemKey, FullName, OperatorName, SemScalarExpr, TableName,
        TypeDefinition, TypeReference, create_table::ViewName,
        type_system::ArgumentBindingCollection,
    },
};

pub use self::insert::{InsertProblem, InsertProblemKind};
pub use self::name_resolution::{DomainBaseTypeNameNotFound, NameResolution};
pub use self::parameter_constraint::ParameterConstraintProblem;
pub use self::query::QueryProblem;

pub(crate) use self::{overload_resolution::OverloadResolution, sql_highlighter::SqlHighlighter};

trait Problem {
    const CODE: &'static str;
    const MESSAGE: &'static str;
    const SEVERITY: miette::Severity;
    const SPAN_LABEL: &'static str;

    fn get_problem_span(&self) -> StringSpan;

    fn labeled_problem_span(&self) -> LabeledSpan {
        self.get_problem_span().new_labeled_span(Self::SPAN_LABEL)
    }
}

trait RenderProblemReport: Problem {
    fn generate_report(
        &self,
        handler: &GraphicalReportHandler,
        source: String,
    ) -> Result<String, AnalysisError> {
        let diag = miette::MietteDiagnostic::new(Self::MESSAGE)
            .with_code(Self::CODE)
            .with_severity(Self::SEVERITY)
            .with_label(self.labeled_problem_span());

        let report = Report::new(diag).with_source_code(NamedSource::new("SQL", source));

        let mut out = String::new();
        handler.render_report(&mut out, &*report)?;
        Ok(out)
    }
}

/// Semantic analysis problems
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum AnalysisProblem {
    /// A violation of INSERT semantic rules.
    Insert(InsertProblem),
    /// A structured SELECT clause violation.
    Query(QueryProblem),
    /// SQL-derived parameter requirements cannot be reconciled.
    ParameterConstraint(ParameterConstraintProblem),
    /// Name resolution problems
    NameResolution(NameResolution),
    /// Overload resolution problems
    OverloadResolution(OverloadResolution),

    /// extension not found
    ExtensionNotFound(String),

    /// can not to guess nullablity
    NullabilityUnknown(String),

    /// Argument type mismatch
    ArgumentTypeMismatch(FullName, usize),

    /// duplicate def elem key
    DuplicateDefElemKey(DefElemKey),

    /// explicit cast not defined
    ExplicitCastNotDefined(TypeReference, TypeReference),

    /// Coalesce expr type unknown
    CoalesceExprTypeUnknown(Vec<SemScalarExpr>),

    /// Expression type not known
    ExpressionTypeNotKnown(SemScalarExpr),

    /// Operator Not Defined
    UndefinedOperator(
        crate::syn::AExprKind,
        OperatorName,
        Option<SemScalarExpr>,
        Option<SemScalarExpr>,
    ),

    /// Array Required
    RequireArray(SemScalarExpr, TypeReference),

    /// sub query not have a derived column
    SubQueryNotHaveColumnForDerivedColumnName(),

    /// incompatible case values
    IncompatibleCaseValues(HashSet<TypeReference>),

    /// case when expr type unknown
    CaseWhenExprTypeUnknown(SemScalarExpr),

    /// array type mismatch
    ArrayIndexTypeMismatch(SemScalarExpr, TypeReference),

    /// member access requires composite type
    MemberAccessRequiresCompositeType(FullName, String),

    /// function argument implicit cast not found
    FunctionArgImplicitCastNotFound(FullName, usize, TypeDefinition, SemScalarExpr),

    /// implicit cast not found
    ImplicitCastNotFound(TypeReference, TypeReference),

    /// no compatible table-function overload
    ParameterTypeMismatchForTableFunction(FullName, ArgumentBindingCollection),
}

impl AnalysisProblem {
    pub(crate) fn insert(kind: InsertProblemKind, message: &str) -> Self {
        Self::Insert(InsertProblem::new(kind, message))
    }

    /// Structured INSERT violation, independently of diagnostic wording.
    pub const fn insert_problem(&self) -> Option<&InsertProblem> {
        if let Self::Insert(problem) = self {
            Some(problem)
        } else {
            None
        }
    }

    /// Report a structured SELECT clause violation without inventing a source location.
    pub const fn query(problem: QueryProblem) -> Self {
        Self::Query(problem)
    }
    /// Structured SELECT violation, independently of its diagnostic rendering.
    pub const fn query_problem(&self) -> Option<&QueryProblem> {
        match self {
            Self::Query(problem) => Some(problem),
            _ => None,
        }
    }
    /// Report conflicting or invalid SQL-derived parameter requirements.
    pub fn parameter_constraint(message: &str) -> Self {
        Self::ParameterConstraint(ParameterConstraintProblem::new(message))
    }

    /// Stable diagnostic classification, independent of formatted debug output.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Insert(problem) => problem.code(),
            Self::Query(problem) => problem.code(),
            Self::ParameterConstraint(problem) => problem.code(),
            Self::NameResolution(problem) => problem.code(),
            Self::OverloadResolution(problem) => problem.code(),
            Self::ExtensionNotFound(..) => "extension_not_found",
            Self::NullabilityUnknown(..) => "nullability_unknown",
            Self::ArgumentTypeMismatch(..) => "argument_type_mismatch",
            Self::DuplicateDefElemKey(..) => "duplicate_def_elem_key",
            Self::ExplicitCastNotDefined(..) => "explicit_cast_not_defined",
            Self::CoalesceExprTypeUnknown(..) => "coalesce_expr_type_unknown",
            Self::ExpressionTypeNotKnown(..) => "expression_type_not_known",
            Self::UndefinedOperator(..) => "undefined_operator",
            Self::RequireArray(..) => "require_array",
            Self::SubQueryNotHaveColumnForDerivedColumnName(..) => {
                "sub_query_not_have_column_for_derived_column_name"
            }
            Self::IncompatibleCaseValues(..) => "incompatible_case_values",
            Self::CaseWhenExprTypeUnknown(..) => "case_when_expr_type_unknown",
            Self::ArrayIndexTypeMismatch(..) => "array_index_type_mismatch",
            Self::MemberAccessRequiresCompositeType(..) => "member_access_requires_composite_type",
            Self::FunctionArgImplicitCastNotFound(..) => "function_arg_implicit_cast_not_found",
            Self::ImplicitCastNotFound(..) => "implicit_cast_not_found",
            Self::ParameterTypeMismatchForTableFunction(..) => {
                "parameter_type_mismatch_for_table_function"
            }
        }
    }

    /// Retained statement-relative byte range, when recorded during analysis.
    #[must_use]
    pub fn statement_relative_span(&self) -> Option<StringSpan> {
        match self {
            Self::NameResolution(problem) => problem.statement_relative_span(),
            _ => None,
        }
    }

    /// covert problem to report
    pub fn render_report(
        &self,
        theme: &HighlighterTheme,
        source: String,
    ) -> Result<String, AnalysisError> {
        let report_handler = GraphicalReportHandler::new()
            .with_syntax_highlighting(SqlHighlighter::from_theme(theme)?);

        let report = match self {
            Self::Insert(problem) => problem.render_report(&report_handler, source)?,
            Self::Query(problem) => problem.render_report(&report_handler, source)?,
            Self::NameResolution(nr) => nr.render_report(&report_handler, source)?,
            Self::ParameterConstraint(problem) => problem.render_report(&report_handler, source)?,
            _ => todo!(),
        };
        Ok(report)
    }

    /// extension not found
    #[tracing::instrument()]
    pub fn extension_not_found(name: &str) -> Self {
        Self::ExtensionNotFound(String::from(name))
    }

    /// expression nullability not known
    #[tracing::instrument()]
    pub fn nullability_unknown(item_name: &str) -> Self {
        Self::NullabilityUnknown(String::from(item_name))
    }

    /// function argument type mismatch
    #[tracing::instrument()]
    pub fn argument_type_mismatch(func_name: &FullName, index: usize) -> Self {
        Self::ArgumentTypeMismatch(func_name.clone(), index)
    }

    /// duplicate name in definition element
    #[tracing::instrument()]
    pub fn duplicate_def_elem_key(key: &DefElemKey) -> Self {
        Self::DuplicateDefElemKey(key.clone())
    }

    /// doing explicit cast conversion , but not allowed
    #[tracing::instrument()]
    pub fn explicit_cast_not_defined(source: &TypeReference, target: &TypeReference) -> Self {
        Self::ExplicitCastNotDefined(source.clone(), target.clone())
    }

    /// expression requires array but not
    #[tracing::instrument()]
    pub fn array_required(expr: &SemScalarExpr, type_inspected: &TypeReference) -> Self {
        Self::RequireArray(expr.clone(), type_inspected.clone())
    }

    /// array index type not integer
    #[tracing::instrument()]
    pub fn array_index_type_mismatch(expr: &SemScalarExpr, type_inspected: &TypeReference) -> Self {
        Self::ArrayIndexTypeMismatch(expr.clone(), type_inspected.clone())
    }

    /// member accessing to non-composite type
    #[tracing::instrument()]
    pub fn member_access_requires_composite_type(type_name: &FullName, member_name: &str) -> Self {
        Self::MemberAccessRequiresCompositeType(type_name.clone(), member_name.to_string())
    }

    /// operator not defined
    #[track_caller]
    #[tracing::instrument()]
    pub fn undefined_operator(
        aexpr_op: crate::syn::AExprKind,
        name: &OperatorName,
        left_arg: &Option<SemScalarExpr>,
        right_arg: &Option<SemScalarExpr>,
    ) -> Self {
        Self::UndefinedOperator(aexpr_op, name.clone(), left_arg.clone(), right_arg.clone())
    }

    /// parameter type can not cast to function argument
    #[tracing::instrument()]
    pub fn parameter_type_mismatch_for_table_function(
        name: &FullName,
        arg_types: &ArgumentBindingCollection,
    ) -> Self {
        Self::ParameterTypeMismatchForTableFunction(name.clone(), arg_types.clone())
    }

    /// specified function result set type not found
    #[track_caller]
    #[tracing::instrument()]
    pub fn function_result_set_type_not_found(ret_type: &super::FullName) -> Self {
        todo!("{ret_type:?}")
    }

    /// coalece argument unreachable
    #[tracing::instrument()]
    pub fn unreachable_coalesce_argument(
        arg_expr: &SemScalarExpr,
        first_non_null: &SemScalarExpr,
    ) -> Self {
        todo!()
    }

    /// unknown type used in coalesce
    #[tracing::instrument()]
    pub fn unknown_type_in_coalesce_expr(arg_expr: &SemScalarExpr) -> Self {
        todo!()
    }

    /// colesce expression result type ambiguous
    #[tracing::instrument()]
    pub fn coalesce_expr_type_ambiguous(
        arg_exprs: &Vec<SemScalarExpr>,
        candidates: &HashSet<TypeReference>,
        choose: Option<&TypeReference>,
    ) -> Self {
        todo!()
    }

    /// all of coalesce type is not known
    #[tracing::instrument()]
    pub fn coalesce_expr_type_unknown(arg_exprs: &[SemScalarExpr]) -> Self {
        tracing::warn!("{arg_exprs:#?}");
        Self::CoalesceExprTypeUnknown(arg_exprs.to_vec())
    }

    /// implicit cast not allowed
    #[tracing::instrument()]
    pub fn function_arg_implicit_cast_not_found(
        func_name: &FullName,
        index: usize,
        t: &TypeDefinition,
        arg: &SemScalarExpr,
    ) -> Self {
        Self::FunctionArgImplicitCastNotFound(func_name.clone(), index, t.clone(), arg.clone())
    }

    /// view used as materialized view, but not a materialized view
    #[tracing::instrument()]
    pub fn view_not_matview(view_name: &ViewName) -> Self {
        todo!()
    }

    /// operator name syntax is invalid
    #[tracing::instrument()]
    pub fn operator_name_syntax(v: &[String]) -> Self {
        todo!()
    }

    /// expression type not known
    #[tracing::instrument()]
    pub fn expression_type_not_known(expr: &SemScalarExpr) -> Self {
        Self::ExpressionTypeNotKnown(expr.clone())
    }

    /// case when type unknown
    #[tracing::instrument()]
    pub fn case_when_type_unknown(case_when_expr: SemScalarExpr) -> Self {
        Self::CaseWhenExprTypeUnknown(case_when_expr)
    }

    /// implicit cast not found
    #[tracing::instrument()]
    pub fn implicit_cast_not_found(ty: &TypeReference, result_type: &TypeReference) -> Self {
        Self::ImplicitCastNotFound(ty.clone(), result_type.clone())
    }

    /// incompatible cast values
    #[tracing::instrument()]
    pub fn incompatible_case_values(result_types: HashSet<TypeReference>) -> Self {
        Self::IncompatibleCaseValues(result_types)
    }

    /// unexpected dynamic type
    #[tracing::instrument()]
    pub fn unexpected_dynamic_type(sem_type: &TypeDefinition) -> Self {
        todo!()
    }

    /// table not have column
    #[tracing::instrument()]
    pub fn table_not_have_column_for_derived_column_name() -> Self {
        todo!()
    }

    /// cte not have column
    #[tracing::instrument()]
    pub fn cte_not_have_column_for_derived_column_name() -> Self {
        todo!()
    }

    /// subquery not have column
    #[tracing::instrument()]
    pub fn subquery_not_have_column_for_derived_column_name() -> Self {
        Self::SubQueryNotHaveColumnForDerivedColumnName()
    }

    pub(crate) fn foreign_key_column_type_mismatch(
        _fk_table_name: &TableName,
        _fk_column_name: &ColumnName,
        _fk_column_type: &TypeReference,
        _pk_table_name: &TableName,
        _pk_column_name: &ColumnName,
        _pk_column_type: &TypeReference,
    ) -> Self {
        todo!()
    }

    #[cfg(feature = "sqlite")]
    pub(crate) fn sqlite_ignores_match_option_in_fk_constraint(
        _table_name: &TableName,
        _column_name: &ColumnName,
        _name: &str,
    ) -> Self {
        todo!()
    }
}

#[cfg(test)]
mod test_rpc_diagnostic_metadata {
    use super::AnalysisProblem;
    use crate::{
        StringSpan,
        sem::{FullName, TableName},
    };

    #[test]
    fn preserves_existing_code_and_recorded_byte_range() {
        let span = StringSpan::new(12, 7);
        let problem = AnalysisProblem::domain_base_type_not_found(
            &FullName::with_schema("public", "domain"),
            "missing",
            &span,
        );
        assert_eq!(problem.code(), "SF000001");
        assert_eq!(problem.statement_relative_span().unwrap().offset(), 12);
        assert_eq!(problem.statement_relative_span().unwrap().len(), 7);
    }

    #[test]
    fn classifies_unlocated_problems_without_fabricating_a_span() {
        let problem = AnalysisProblem::table_not_found(&TableName::from(FullName::with_schema(
            "public", "missing",
        )));
        assert_eq!(problem.code(), "table_not_found");
        assert!(problem.statement_relative_span().is_none());
    }
}
