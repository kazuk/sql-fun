mod name_resolution;
mod overload_resolution;

use std::collections::HashSet;

use crate::sem::{
    DefElemKey, FullName, OperatorName, SemScalarExpr, TypeDefinition, TypeReference,
    create_table::ViewName, type_system::ArgumentBindingCollection,
};

pub use self::{name_resolution::NameResolution, overload_resolution::OverloadResolution};

/// Semantic analysis problems
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum AnalysisProblem {
    /// Name resolution problems
    NameResolution(NameResolution),
    /// Overload resolution problems
    OverloadResolution(OverloadResolution),

    /// extension not found
    ExtensionNotFound(String),

    /// can not to guess nullablity
    NullabilityUnknown(String),

    /// Argument type missmatch
    ArgumentTypeMissmatch(FullName, usize),

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

    /// array type missmatch
    ArrayIndexTypeMissmatch(SemScalarExpr, TypeReference),

    /// member access requires composite type
    MemberAccessRequiresCompositeType(FullName, String),
}

impl AnalysisProblem {
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

    /// function argument type missmatch
    #[tracing::instrument()]
    pub fn argument_type_missmatch(func_name: &FullName, index: usize) -> Self {
        Self::ArgumentTypeMissmatch(func_name.clone(), index)
    }

    /// duplicate name in definition element
    #[tracing::instrument()]
    pub fn duplicate_def_elem_key(key: &DefElemKey) -> Self {
        Self::DuplicateDefElemKey(key.clone())
    }

    /// doing explicit cast coversion , but not allowed
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
    pub fn array_index_type_missmatch(
        expr: &SemScalarExpr,
        type_inspected: &TypeReference,
    ) -> Self {
        Self::ArrayIndexTypeMissmatch(expr.clone(), type_inspected.clone())
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
    pub fn parameter_type_missmatch_for_table_function(
        call: &crate::syn::FuncCall,
        arg_types: ArgumentBindingCollection,
    ) -> Self {
        todo!()
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
        candiates: &HashSet<TypeReference>,
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
        todo!()
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
        todo!()
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
}
