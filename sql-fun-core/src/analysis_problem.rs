//! Presentation-neutral SQL analysis diagnostics.
mod alter_column;
mod array;
mod call_argument;
mod creation;
mod delete;
mod index;
mod insert;
mod message;
mod min_max;
mod name_resolution;
mod overload_resolution;
mod ownership;
mod parameter_constraint;
mod query;
mod rule;
mod table_function;
mod trigger;
mod update;
mod view;
pub use self::alter_column::{AlterColumnProblem, ColumnAttributeProblem};
pub use self::array::ArrayProblem;
pub use self::call_argument::CallArgumentProblem;
pub use self::creation::CreationProblem;
pub use self::delete::{
    CursorNameProblem, CursorTargetMismatchProblem, DeleteProblem, DeleteProblemKind,
};
pub use self::index::{IndexProblem, IndexProblemKind};
pub use self::insert::{InsertProblem, InsertProblemKind};
pub use self::min_max::MinMaxProblem;
pub use self::name_resolution::{DomainBaseTypeNameNotFound, FunctionNotFound, NameResolution};
pub use self::overload_resolution::OverloadResolution;
pub use self::ownership::OwnershipProblem;
pub use self::parameter_constraint::ParameterConstraintProblem;
pub use self::query::QueryProblem;
pub use self::rule::{RuleDefinitionProblem, RuleProblem};
pub use self::table_function::TableFunctionProblem;
pub use self::trigger::TriggerProblem;
pub use self::update::{UpdateProblem, UpdateProblemKind};
pub use self::view::ViewProblem;
mod values;
pub use self::values::{
    AExprKind, AliasName, ArgumentBindingCollection, ColumnName, CteName, DebugName, DefElemKey,
    DiagnosticLabel, DiagnosticMetadata, DiagnosticSeverity, ExpressionList, FullName, Kind,
    OperatorDefinition, OperatorName, OptionalExpression, OwnershipIdentity, OwnershipLookup,
    RuleIdentity, SemScalarExpr, StringSpan, TableName, TriggerIdentity, TypeDefinition, TypeList,
    TypeName, TypeReference, ViewName,
};
/// Explanatory text available for every semantic diagnostic, without source labels.
pub trait DiagnosticMessage {
    /// Describe the problem using the facts retained by its semantic payload.
    fn diagnostic_message(&self) -> String;
}
/// Semantic analysis problems
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum AnalysisProblem {
    /// SQL analysis could not establish a finalized parameter type.
    ParameterTypeUnresolved(u32),
    /// Input could not be parsed, realized, hinted, analyzed, or deparsed.
    QueryAnalysis(QueryAnalysisProblem),
    /// Invalid ARRAY constructor typing.
    Array(ArrayProblem),
    /// Invalid index alteration or a statistics warning.
    Index(IndexProblem),
    /// An ALTER COLUMN binding or attribute is invalid.
    AlterColumn(ColumnAttributeProblem),
    /// Invalid PostgreSQL GREATEST/LEAST common type or comparison capability.
    MinMax(MinMaxProblem),
    /// A function invocation has an invalid argument order or repeated name.
    CallArgument(CallArgumentProblem),
    /// A creation conflicts with an existing catalog identity.
    Creation(CreationProblem),
    /// Invalid rule definition.
    Rule(RuleProblem),
    /// ALTER OWNER target or role lookup failed.
    Ownership(OwnershipProblem),
    /// Invalid trigger semantics.
    Trigger(TriggerProblem),
    /// A view declaration or catalog capability violation.
    View(ViewProblem),
    /// A violation of INSERT semantic rules.
    Insert(InsertProblem),
    /// A violation of DELETE semantic rules.
    Delete(DeleteProblem),
    /// A violation of UPDATE semantic rules.
    Update(UpdateProblem),
    /// A structured SELECT clause violation.
    Query(QueryProblem),
    /// Invalid table-function output declarations.
    TableFunction(TableFunctionProblem),
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

    /// A COALESCE argument follows a proven non-null operand.
    UnreachableCoalesceArgument(SemScalarExpr, SemScalarExpr),

    /// Coalesce expr type unknown
    CoalesceExprTypeUnknown(ExpressionList),

    /// Expression type not known
    ExpressionTypeNotKnown(SemScalarExpr),

    /// Operator Not Defined
    UndefinedOperator(
        AExprKind,
        OperatorName,
        OptionalExpression,
        OptionalExpression,
    ),

    /// Array Required
    RequireArray(SemScalarExpr, TypeReference),

    /// sub query not have a derived column
    SubQueryNotHaveColumnForDerivedColumnName(),

    /// incompatible case values
    IncompatibleCaseValues(TypeList),

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
    /// Invalid table-function output declaration.
    #[must_use]
    pub const fn table_function(problem: TableFunctionProblem) -> Self {
        Self::TableFunction(problem)
    }
    /// Retained table-function diagnostic identity.
    #[must_use]
    pub const fn table_function_problem(&self) -> Option<&TableFunctionProblem> {
        if let Self::TableFunction(problem) = self {
            Some(problem)
        } else {
            None
        }
    }
    /// Diagnostic index.
    #[must_use]
    pub fn index(problem: &IndexProblem) -> Self {
        Self::Index(problem.clone())
    }
    /// Structured index diagnostic independent of rendered text.
    #[must_use]
    pub const fn index_problem(&self) -> Option<&IndexProblem> {
        if let Self::Index(problem) = self {
            Some(problem)
        } else {
            None
        }
    }
    /// Diagnostic severity, independent of rendering and catalog applicability.
    #[must_use]
    pub const fn severity(&self) -> DiagnosticSeverity {
        match self {
            Self::AlterColumn(problem) => problem.reason().severity(),
            Self::Index(problem) => problem.severity(),
            Self::UnreachableCoalesceArgument(..) => DiagnosticSeverity::Warning,
            _ => DiagnosticSeverity::Error,
        }
    }

    /// Diagnostic alter column.
    #[must_use]
    pub fn alter_column(
        table: impl Into<TableName>,
        column: impl Into<ColumnName>,
        problem: &AlterColumnProblem,
    ) -> Self {
        Self::AlterColumn(ColumnAttributeProblem::new(table, column, problem))
    }
    /// Structured ALTER COLUMN validation outcome.
    #[must_use]
    pub const fn alter_column_problem(&self) -> Option<&ColumnAttributeProblem> {
        if let Self::AlterColumn(problem) = self {
            Some(problem)
        } else {
            None
        }
    }

    /// Diagnostic creation.
    #[must_use]
    pub const fn creation(problem: CreationProblem) -> Self {
        Self::Creation(problem)
    }
    /// Structured creation failure, independently of rendered diagnostic text.
    #[must_use]
    pub const fn creation_problem(&self) -> Option<&CreationProblem> {
        if let Self::Creation(problem) = self {
            Some(problem)
        } else {
            None
        }
    }

    /// Diagnostic rule.
    #[must_use]
    pub fn rule(problem: &RuleProblem) -> Self {
        Self::Rule(problem.clone())
    }
    /// Diagnostic ownership target.
    #[must_use]
    pub fn ownership_target(target: impl Into<OwnershipLookup>) -> Self {
        Self::Ownership(OwnershipProblem::target(target))
    }
    /// Diagnostic ownership role not found.
    #[must_use]
    pub fn ownership_role_not_found(name: &str) -> Self {
        Self::Ownership(OwnershipProblem::missing_role(name))
    }
    /// Ownership failure, independent of diagnostic formatting.
    #[must_use]
    pub const fn ownership_problem(&self) -> Option<&OwnershipProblem> {
        if let Self::Ownership(problem) = self {
            Some(problem)
        } else {
            None
        }
    }

    /// Diagnostic trigger.
    #[must_use]
    pub fn trigger(problem: &TriggerProblem) -> Self {
        Self::Trigger(problem.clone())
    }

    /// Diagnostic delete.
    #[must_use]
    pub const fn delete(kind: DeleteProblemKind) -> Self {
        Self::Delete(DeleteProblem::new(kind))
    }
    /// Structured DELETE violation, independently of diagnostic wording.
    #[must_use]
    pub const fn delete_problem(&self) -> Option<&DeleteProblem> {
        match self {
            Self::Delete(problem) => Some(problem),
            _ => None,
        }
    }

    /// Diagnostic view.
    #[must_use]
    pub fn view(problem: &ViewProblem) -> Self {
        Self::View(problem.clone())
    }

    /// Structured CREATE RULE failure, when this diagnostic describes a rule.
    #[must_use]
    pub const fn rule_problem(&self) -> Option<&RuleProblem> {
        if let Self::Rule(problem) = self {
            Some(problem)
        } else {
            None
        }
    }
    /// Structured view declaration or capability violation.
    #[must_use]
    pub const fn view_problem(&self) -> Option<&ViewProblem> {
        if let Self::View(problem) = self {
            Some(problem)
        } else {
            None
        }
    }
    /// Diagnostic update.
    #[must_use]
    pub fn update(kind: UpdateProblemKind, message: &str) -> Self {
        Self::Update(UpdateProblem::new(kind, message))
    }
    /// Structured UPDATE violation.
    #[must_use]
    pub const fn update_problem(&self) -> Option<&UpdateProblem> {
        match self {
            Self::Update(problem) => Some(problem),
            _ => None,
        }
    }
    /// Diagnostic insert.
    #[must_use]
    pub fn insert(kind: InsertProblemKind, message: &str) -> Self {
        Self::Insert(InsertProblem::new(kind, message))
    }

    /// Structured INSERT violation, independently of diagnostic wording.
    #[must_use]
    pub const fn insert_problem(&self) -> Option<&InsertProblem> {
        if let Self::Insert(problem) = self {
            Some(problem)
        } else {
            None
        }
    }

    /// Report a signature-independent invocation violation.
    #[must_use]
    pub const fn call_argument(problem: CallArgumentProblem) -> Self {
        Self::CallArgument(problem)
    }

    /// Report a structured SELECT clause violation without inventing a source location.
    #[must_use]
    pub const fn query(problem: QueryProblem) -> Self {
        Self::Query(problem)
    }
    /// Structured SELECT violation, independently of its diagnostic rendering.
    #[must_use]
    pub const fn query_problem(&self) -> Option<&QueryProblem> {
        match self {
            Self::Query(problem) => Some(problem),
            _ => None,
        }
    }
    /// An ARRAY constructor typing failure.
    #[must_use]
    pub const fn array(problem: ArrayProblem) -> Self {
        Self::Array(problem)
    }

    /// Report a PostgreSQL GREATEST/LEAST semantic failure.
    #[must_use]
    pub const fn min_max(problem: MinMaxProblem) -> Self {
        Self::MinMax(problem)
    }
    /// Report conflicting or invalid SQL-derived parameter requirements.
    #[must_use]
    pub fn parameter_constraint(message: &str) -> Self {
        Self::ParameterConstraint(ParameterConstraintProblem::new(message))
    }

    /// Stable diagnostic classification, independent of formatted debug output.
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::ParameterTypeUnresolved(_) => "parameter_type_unresolved",
            Self::QueryAnalysis(problem) => problem.code(),
            Self::Index(problem) => problem.code(),
            Self::AlterColumn(problem) => problem.code(),
            Self::Creation(_) => "conflicting_creation",
            Self::Ownership(problem) => problem.code(),
            Self::Rule(_) => "rule_invalid_definition",
            Self::Trigger(_) => "trigger_invalid_definition",
            Self::View(problem) => problem.code(),
            Self::Insert(_) => "insert_semantics",
            Self::Update(_) => "update_semantics",
            Self::Delete(_) => "delete_semantics",
            Self::Query(problem) => problem.code(),
            Self::TableFunction(problem) => problem.code(),
            Self::CallArgument(problem) => problem.code(),
            Self::MinMax(problem) => problem.code(),
            Self::Array(problem) => problem.code(),
            Self::ParameterConstraint(_) => "parameter_constraint",
            Self::NameResolution(problem) => problem.code(),
            Self::OverloadResolution(problem) => problem.code(),
            Self::ExtensionNotFound(..) => "extension_not_found",
            Self::NullabilityUnknown(..) => "nullability_unknown",
            Self::ArgumentTypeMismatch(..) => "argument_type_mismatch",
            Self::DuplicateDefElemKey(..) => "duplicate_def_elem_key",
            Self::ExplicitCastNotDefined(..) => "explicit_cast_not_defined",
            Self::CoalesceExprTypeUnknown(..) => "coalesce_expr_type_unknown",
            Self::UnreachableCoalesceArgument(..) => "unreachable_coalesce_argument",
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
            Self::CallArgument(problem) => problem.statement_relative_span(),
            _ => None,
        }
    }

    /// Presentation data for implemented diagnostics. Unimplemented renderers remain absent.
    #[must_use]
    pub fn render_metadata(&self) -> Option<DiagnosticMetadata> {
        Some(match self {
            Self::ParameterTypeUnresolved(_) => DiagnosticMetadata::from(self.diagnostic_message()),
            Self::QueryAnalysis(problem) => DiagnosticMetadata::from(problem.message()),
            Self::Index(problem) => problem.render_metadata(),
            Self::UnreachableCoalesceArgument(..) => DiagnosticMetadata::from(
                "COALESCE argument is unreachable after a non-null operand",
            ),
            Self::CoalesceExprTypeUnknown(..) => DiagnosticMetadata::from(
                "COALESCE operands do not have a common implicit result type",
            ),
            Self::AlterColumn(problem) => problem.render_metadata(),
            Self::Creation(problem) => problem.render_metadata(),
            Self::Ownership(problem) => problem.render_metadata(),
            Self::Rule(problem) => problem.render_metadata(),
            Self::Trigger(problem) => problem.render_metadata(),
            Self::View(problem) => problem.render_metadata(),
            Self::Insert(problem) => problem.render_metadata(),
            Self::Update(problem) => problem.render_metadata(),
            Self::Delete(problem) => problem.render_metadata(),
            Self::Query(problem) => problem.render_metadata(),
            Self::TableFunction(problem) => problem.render_metadata(),
            Self::CallArgument(problem) => problem.render_metadata(),
            Self::MinMax(problem) => problem.render_metadata(),
            Self::Array(problem) => problem.render_metadata(),
            Self::ParameterConstraint(problem) => problem.render_metadata(),
            Self::NameResolution(problem) => return problem.render_metadata(),
            _ => return None,
        })
    }
}

impl DiagnosticMessage for AnalysisProblem {
    fn diagnostic_message(&self) -> String {
        match self {
            Self::ParameterTypeUnresolved(index) => {
                format!(
                    "application input binding for parameter ${index} is unresolved; supply a supported explicit input type hint; an SQL result cast may be insufficient"
                )
            }
            Self::QueryAnalysis(problem) => problem.diagnostic_message(),
            Self::Array(problem) => problem.diagnostic_message(),
            Self::Index(problem) => problem.diagnostic_message(),
            Self::AlterColumn(problem) => problem.diagnostic_message(),
            Self::MinMax(problem) => problem.diagnostic_message(),
            Self::CallArgument(problem) => problem.diagnostic_message(),
            Self::Creation(problem) => problem.diagnostic_message(),
            Self::Rule(problem) => problem.diagnostic_message(),
            Self::Ownership(problem) => problem.diagnostic_message(),
            Self::Trigger(problem) => problem.diagnostic_message(),
            Self::View(problem) => problem.diagnostic_message(),
            Self::Insert(problem) => problem.diagnostic_message(),
            Self::Delete(problem) => problem.diagnostic_message(),
            Self::Update(problem) => problem.diagnostic_message(),
            Self::Query(problem) => problem.diagnostic_message(),
            Self::TableFunction(problem) => problem.diagnostic_message(),
            Self::ParameterConstraint(problem) => problem.diagnostic_message(),
            Self::NameResolution(problem) => problem.diagnostic_message(),
            Self::OverloadResolution(problem) => problem.diagnostic_message(),
            Self::ExtensionNotFound(name) => format!("extension not found: {name}"),
            Self::NullabilityUnknown(name) => {
                format!("nullability could not be determined for {name}")
            }
            Self::ArgumentTypeMismatch(name, index) => {
                format!("argument {index} has an incompatible type for {name}")
            }
            Self::DuplicateDefElemKey(key) => format!("duplicate definition option: {key}"),
            Self::ExplicitCastNotDefined(source, target) => {
                format!("explicit cast from {source} to {target} is not defined")
            }
            Self::ExpressionTypeNotKnown(_) => {
                "SQL type of an expression could not be determined".to_owned()
            }
            Self::UndefinedOperator(_, operator, _, _) => {
                format!("operator {operator} has no definition for the supplied operands")
            }
            Self::RequireArray(_, ty) => {
                format!("array operation requires an array value, found {ty}")
            }
            Self::SubQueryNotHaveColumnForDerivedColumnName() => {
                "subquery has no column for the derived column name".to_owned()
            }
            Self::IncompatibleCaseValues(_) => {
                "CASE result expressions have no common type".to_owned()
            }
            Self::CaseWhenExprTypeUnknown(_) => {
                "SQL type of a CASE WHEN condition could not be determined".to_owned()
            }
            Self::ArrayIndexTypeMismatch(_, ty) => {
                format!("array subscript has an incompatible type: {ty}")
            }
            Self::MemberAccessRequiresCompositeType(name, member) => {
                format!("member {member} of {name} requires a composite type")
            }
            Self::FunctionArgImplicitCastNotFound(name, index, ty, _) => {
                format!("argument {index} of {name} cannot be implicitly cast to {ty}")
            }
            Self::ImplicitCastNotFound(source, target) => {
                format!("implicit cast from {source} to {target} is not defined")
            }
            Self::ParameterTypeMismatchForTableFunction(name, arguments) => {
                format!("no matching table-function signature for {name} with {arguments}")
            }
            Self::UnreachableCoalesceArgument(..) => {
                "COALESCE argument is unreachable after a non-null operand".to_owned()
            }
            Self::CoalesceExprTypeUnknown(..) => {
                "COALESCE operands do not have a common implicit result type".to_owned()
            }
        }
    }
}

#[cfg(test)]
mod diagnostic_message_tests {
    use super::{AExprKind, AnalysisProblem, DiagnosticMessage, RuleProblem, TriggerProblem};

    #[test]
    fn unresolved_binding_messages_agree_and_remain_errors() {
        let problem = AnalysisProblem::parameter_type_unresolved(2);
        let expected = "application input binding for parameter $2 is unresolved; supply a supported explicit input type hint; an SQL result cast may be insufficient";
        assert_eq!(problem.code(), "parameter_type_unresolved");
        assert_eq!(problem.severity(), super::DiagnosticSeverity::Error);
        assert_eq!(problem.diagnostic_message(), expected);
        assert_eq!(problem.render_metadata().unwrap().message(), expected);
    }

    #[test]
    fn rule_and_trigger_messages_explain_the_problem_without_debug_output() {
        let rule = AnalysisProblem::rule(&RuleProblem::target("public.orders"));
        let trigger = AnalysisProblem::trigger(&TriggerProblem::function("public.audit"));

        assert_eq!(
            rule.diagnostic_message(),
            "rule target public.orders does not resolve to a supported relation"
        );
        assert_eq!(
            trigger.diagnostic_message(),
            "trigger function public.audit is missing or cannot be called without arguments"
        );
    }

    #[test]
    fn undefined_operator_message_does_not_expose_rust_expression_debug_text() {
        let problem = AnalysisProblem::undefined_operator(
            AExprKind::new("AexprOp"),
            "+",
            &Some("left expression"),
            &Some("right expression"),
        );

        assert_eq!(
            problem.diagnostic_message(),
            "operator + has no definition for the supplied operands"
        );
    }
}

impl AnalysisProblem {
    /// extension not found
    #[must_use]
    pub fn extension_not_found(name: &str) -> Self {
        Self::ExtensionNotFound(String::from(name))
    }

    /// expression nullability not known
    #[must_use]
    pub fn nullability_unknown(item_name: &str) -> Self {
        Self::NullabilityUnknown(String::from(item_name))
    }

    /// function argument type mismatch
    #[must_use]
    pub fn argument_type_mismatch(func_name: impl Into<FullName>, index: usize) -> Self {
        Self::ArgumentTypeMismatch(func_name.into(), index)
    }

    /// duplicate name in definition element
    #[must_use]
    pub fn duplicate_def_elem_key(key: impl Into<DefElemKey>) -> Self {
        Self::DuplicateDefElemKey(key.into())
    }

    /// doing explicit cast conversion , but not allowed
    #[must_use]
    pub fn explicit_cast_not_defined(
        source: impl Into<TypeReference>,
        target: impl Into<TypeReference>,
    ) -> Self {
        Self::ExplicitCastNotDefined(source.into(), target.into())
    }

    /// expression requires array but not
    #[must_use]
    pub fn array_required(
        expr: impl Into<SemScalarExpr>,
        type_inspected: impl Into<TypeReference>,
    ) -> Self {
        Self::RequireArray(expr.into(), type_inspected.into())
    }

    /// array index type not integer
    #[must_use]
    pub fn array_index_type_mismatch(
        expr: impl Into<SemScalarExpr>,
        type_inspected: impl Into<TypeReference>,
    ) -> Self {
        Self::ArrayIndexTypeMismatch(expr.into(), type_inspected.into())
    }

    /// member accessing to non-composite type
    #[must_use]
    pub fn member_access_requires_composite_type(
        type_name: impl Into<FullName>,
        member_name: &str,
    ) -> Self {
        Self::MemberAccessRequiresCompositeType(type_name.into(), member_name.to_string())
    }

    /// operator not defined
    #[track_caller]
    #[must_use]
    pub fn undefined_operator(
        aexpr_op: impl Into<AExprKind>,
        name: impl Into<OperatorName>,
        left_arg: &impl std::fmt::Debug,
        right_arg: &impl std::fmt::Debug,
    ) -> Self {
        Self::UndefinedOperator(
            aexpr_op.into(),
            name.into(),
            OptionalExpression::new(&format!("{left_arg:?}")),
            OptionalExpression::new(&format!("{right_arg:?}")),
        )
    }

    /// parameter type can not cast to function argument
    #[must_use]
    pub fn parameter_type_mismatch_for_table_function(
        name: impl Into<FullName>,
        arg_types: impl Into<ArgumentBindingCollection>,
    ) -> Self {
        Self::ParameterTypeMismatchForTableFunction(name.into(), arg_types.into())
    }

    /// specified function result set type not found
    #[track_caller]
    #[must_use]
    pub fn function_result_set_type_not_found(ret_type: impl Into<FullName>) -> Self {
        todo!("{}", ret_type.into())
    }

    /// A COALESCE argument follows a proven non-null operand.
    #[must_use]
    pub fn unreachable_coalesce_argument(
        arg_expr: impl Into<SemScalarExpr>,
        first_non_null: impl Into<SemScalarExpr>,
    ) -> Self {
        Self::UnreachableCoalesceArgument(arg_expr.into(), first_non_null.into())
    }

    /// unknown type used in coalesce
    #[must_use]
    pub fn unknown_type_in_coalesce_expr(_arg_expr: impl Into<SemScalarExpr>) -> Self {
        todo!()
    }

    /// colesce expression result type ambiguous
    #[must_use]
    pub fn coalesce_expr_type_ambiguous(
        _arg_exprs: &impl std::fmt::Debug,
        _candidates: &impl std::fmt::Debug,
        _choose: Option<&impl std::fmt::Debug>,
    ) -> Self {
        todo!()
    }

    /// all of coalesce type is not known
    #[must_use]
    pub fn coalesce_expr_type_unknown(arg_exprs: &impl std::fmt::Debug) -> Self {
        let arg_exprs = ExpressionList::new(&format!("{arg_exprs:#?}"));
        tracing::warn!("{arg_exprs:#?}");
        Self::CoalesceExprTypeUnknown(arg_exprs)
    }

    /// implicit cast not allowed
    #[must_use]
    pub fn function_arg_implicit_cast_not_found(
        func_name: impl Into<FullName>,
        index: usize,
        t: impl Into<TypeDefinition>,
        arg: impl Into<SemScalarExpr>,
    ) -> Self {
        Self::FunctionArgImplicitCastNotFound(func_name.into(), index, t.into(), arg.into())
    }

    /// view used as materialized view, but not a materialized view
    #[must_use]
    pub fn view_not_matview(_view_name: impl Into<ViewName>) -> Self {
        todo!()
    }

    /// operator name syntax is invalid
    #[must_use]
    pub fn operator_name_syntax(_v: &[String]) -> Self {
        todo!()
    }

    /// expression type not known
    #[must_use]
    pub fn expression_type_not_known(expr: impl Into<SemScalarExpr>) -> Self {
        Self::ExpressionTypeNotKnown(expr.into())
    }

    /// case when type unknown
    #[must_use]
    pub fn case_when_type_unknown(case_when_expr: impl Into<SemScalarExpr>) -> Self {
        Self::CaseWhenExprTypeUnknown(case_when_expr.into())
    }

    /// implicit cast not found
    #[must_use]
    pub fn implicit_cast_not_found(
        ty: impl Into<TypeReference>,
        result_type: impl Into<TypeReference>,
    ) -> Self {
        Self::ImplicitCastNotFound(ty.into(), result_type.into())
    }

    /// incompatible cast values
    #[must_use]
    pub fn incompatible_case_values(result_types: impl std::fmt::Debug) -> Self {
        Self::IncompatibleCaseValues(TypeList::new(&format!("{result_types:?}")))
    }

    /// unexpected dynamic type
    #[must_use]
    pub fn unexpected_dynamic_type(_sem_type: impl Into<TypeDefinition>) -> Self {
        todo!()
    }

    /// table not have column
    #[must_use]
    pub fn table_not_have_column_for_derived_column_name() -> Self {
        todo!()
    }

    /// cte not have column
    #[must_use]
    pub fn cte_not_have_column_for_derived_column_name() -> Self {
        todo!()
    }

    /// subquery not have column
    #[must_use]
    pub const fn subquery_not_have_column_for_derived_column_name() -> Self {
        Self::SubQueryNotHaveColumnForDerivedColumnName()
    }

    /// Diagnostic foreign key column type mismatch.
    #[must_use]
    pub fn foreign_key_column_type_mismatch(
        _fk_table_name: impl Into<TableName>,
        _fk_column_name: impl Into<ColumnName>,
        _fk_column_type: impl Into<TypeReference>,
        _pk_table_name: impl Into<TableName>,
        _pk_column_name: impl Into<ColumnName>,
        _pk_column_type: impl Into<TypeReference>,
    ) -> Self {
        todo!()
    }

    #[cfg(feature = "sqlite")]
    /// Diagnostic sqlite ignores match option in fk constraint.
    #[must_use]
    pub fn sqlite_ignores_match_option_in_fk_constraint(
        _table_name: impl Into<TableName>,
        _column_name: impl Into<ColumnName>,
        _name: &str,
    ) -> Self {
        todo!()
    }
}

#[cfg(test)]
mod coalesce_diagnostic_tests {
    use super::{AnalysisProblem, DiagnosticSeverity};

    #[test]
    fn unreachable_argument_is_a_serializable_warning() {
        let problem = AnalysisProblem::unreachable_coalesce_argument("later", "first");
        assert_eq!(problem.code(), "unreachable_coalesce_argument");
        assert_eq!(problem.severity(), DiagnosticSeverity::Warning);
        assert!(problem.render_metadata().is_some());
        let encoded = ::serde_json::to_string(&problem).unwrap();
        let restored: AnalysisProblem = ::serde_json::from_str(&encoded).unwrap();
        assert_eq!(restored.code(), problem.code());
        assert_eq!(restored.severity(), problem.severity());
    }
}

#[cfg(test)]
mod test_transport {
    use super::{AnalysisProblem, DiagnosticMessage, DiagnosticSeverity, StringSpan};

    #[test]
    fn transports_typed_diagnostic_without_source_or_rendered_output() {
        let problem = AnalysisProblem::domain_base_type_not_found(
            "public.domain",
            "missing",
            StringSpan::new(12, 7),
        );
        let encoded = ::serde_json::to_string(&problem).unwrap();
        let restored: AnalysisProblem = ::serde_json::from_str(&encoded).unwrap();
        assert_eq!(restored.code(), "SF000001");
        assert_eq!(restored.severity(), DiagnosticSeverity::Error);
        assert_eq!(
            restored.statement_relative_span(),
            Some(StringSpan::new(12, 7))
        );
        let metadata = restored.render_metadata().unwrap();
        assert_eq!(metadata.message(), "Domain base type not found");
        assert_eq!(metadata.labels()[0].message(), "base type name");
    }

    #[test]
    fn unsupported_rendering_remains_explicit_after_transport() {
        let problem = AnalysisProblem::extension_not_found("missing");
        let encoded = ::serde_json::to_string(&problem).unwrap();
        let restored: AnalysisProblem = ::serde_json::from_str(&encoded).unwrap();
        assert_eq!(restored.code(), "extension_not_found");
        assert!(restored.render_metadata().is_none());
        assert!(restored.statement_relative_span().is_none());
        assert_eq!(
            restored.diagnostic_message(),
            "extension not found: missing"
        );
    }
}

/// A query-input or analysis-capability failure, distinct from semantic inference.
#[derive(Clone, Debug, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct QueryAnalysisProblem {
    stage: QueryAnalysisStage,
    message: String,
}
impl QueryAnalysisProblem {
    /// Preserve a stage-specific failure without reclassifying it as a type-hint problem.
    #[must_use]
    pub fn new(stage: QueryAnalysisStage, message: &str) -> Self {
        Self {
            stage,
            message: message.to_owned(),
        }
    }
    /// Stable failure stage code.
    #[must_use]
    pub const fn code(&self) -> &str {
        self.stage.code()
    }
    /// Original failure explanation.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}
impl DiagnosticMessage for QueryAnalysisProblem {
    fn diagnostic_message(&self) -> String {
        self.message.clone()
    }
}
impl AnalysisProblem {
    /// Report an input or analysis capability failure.
    #[must_use]
    pub fn query_analysis(stage: QueryAnalysisStage, message: &str) -> Self {
        Self::QueryAnalysis(QueryAnalysisProblem::new(stage, message))
    }
    /// Report an application input binding whose inference did not establish one type.
    #[must_use]
    pub const fn parameter_type_unresolved(index: u32) -> Self {
        Self::ParameterTypeUnresolved(index)
    }
}

/// Structured failure stage; kept distinct from semantic name/type diagnostics.
#[derive(Clone, Copy, Debug, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum QueryAnalysisStage {
    /// Parse capability or input failure.
    Parse,
    /// Realization capability or input failure.
    Realization,
    /// Hint capability or input failure.
    Hint,
    /// Statement capability or input failure.
    Statement,
    /// Analysis capability or input failure.
    Analysis,
    /// Deparse capability or input failure.
    Deparse,
}
impl QueryAnalysisStage {
    /// Identify the parse stage.
    #[must_use]
    pub const fn parse() -> Self {
        Self::Parse
    }
    /// Identify the realization stage.
    #[must_use]
    pub const fn realization() -> Self {
        Self::Realization
    }
    /// Identify the hint stage.
    #[must_use]
    pub const fn hint() -> Self {
        Self::Hint
    }
    /// Identify the statement stage.
    #[must_use]
    pub const fn statement() -> Self {
        Self::Statement
    }
    /// Identify the analysis stage.
    #[must_use]
    pub const fn analysis() -> Self {
        Self::Analysis
    }
    /// Identify the deparse stage.
    #[must_use]
    pub const fn deparse() -> Self {
        Self::Deparse
    }
    const fn code(self) -> &'static str {
        match self {
            Self::Parse => "query_parse",
            Self::Realization => "query_realization",
            Self::Hint => "invalid_parameter_hint",
            Self::Statement => "unsupported_statement",
            Self::Analysis => "query_analysis",
            Self::Deparse => "query_deparse",
        }
    }
}
