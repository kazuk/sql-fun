mod analyze;
mod arith_expr;
mod array;
mod boolean;
mod case_expr;
mod cast_expr;
mod coalesce;
mod collate;
mod column_ref;
mod const_expr;
mod func_call;
mod indirect;
mod min_max;
mod named_arg;
mod param_ref;
mod sql_value;
mod sub_link;

pub use self::{
    analyze::analyze_scaler_expr,
    arith_expr::ArithExpr,
    array::ArrayExpr,
    boolean::{BooleanExpr, NullTestExpr},
    case_expr::CaseExpr,
    cast_expr::{ImplicitCastExpr, TypeCastExpr},
    coalesce::CoalesceExpr,
    collate::CollateExpr,
    column_ref::{ColumnReferenceExpr, CteColumnRef, SubQueryColumnRef, TableColumnRef},
    const_expr::ScalarConstExpr,
    func_call::FuncCallExpr,
    indirect::IndirectionExpr,
    min_max::MinMaxExpr,
    named_arg::NamedArgExpr,
    param_ref::ParamRef,
    sql_value::SqlValueExpr,
    sub_link::SubLinkExpr,
};

use crate::sem::{
    AnalysisError, AnalysisProblem, FromClause, ParseContext, PgBuiltInType, TypeReference,
    WithClause, create_table::ColumnDefinition,
};

trait AnalyzeScalarExpr<TParseContext, TNode>
where
    TParseContext: ParseContext,
{
    fn analyze_scalar_expr(
        context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        syn: TNode,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>;
}

trait SemScalarExprNode {
    fn get_type(&self) -> Option<TypeReference>;
    fn is_not_null(&self) -> Option<bool>;
}

/// scalar explation in semantic ast
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SemScalarExpr {
    /// array expression
    Array(ArrayExpr),
    /// const expression
    Const(ScalarConstExpr),
    /// column reference expression
    ColumnRef(ColumnReferenceExpr),
    /// function call expression
    FuncCall(FuncCallExpr),
    /// arith expression
    Arith(ArithExpr),
    /// type casting expression
    TypeCast(TypeCastExpr),
    /// cast expression
    Case(CaseExpr),
    /// boolean expression
    Boolean(BooleanExpr),
    /// null test expression
    NullTest(NullTestExpr),
    /// sub-query expression
    SubLink(SubLinkExpr),
    /// calesce expression
    Coalesce(CoalesceExpr),
    /// min-max expressions
    MinMax(MinMaxExpr),
    /// collate explressions
    Collate(CollateExpr),
    /// SQL standard value expression
    SqlValue(SqlValueExpr),
    /// parameter reference expression
    Param(ParamRef),
    /// named argment expression
    NamedArg(NamedArgExpr),
    /// indirectrion expression
    Indirection(IndirectionExpr),
    /// implicit cast expression
    ///
    /// # Note
    ///
    /// ImplicitCast is not in Syntax level AST.
    ///
    /// `sql-fun` inserts implicit cast at a implicit coversion as needed.
    ///
    ImplicitCast(ImplicitCastExpr),

    /// Unexpected syntax node in expression
    Unexpected(String),
}

impl SemScalarExpr {
    /// create new const expr
    #[cfg(test)]
    pub fn new_const(value: ScalarConstExpr) -> Self {
        Self::Const(value)
    }

    fn builtin_boolean_type() -> Option<TypeReference> {
        Some(TypeReference::concrete_type_ref(
            PgBuiltInType::bool().full_name(),
            false,
        ))
    }

    /// determine expresion value type
    pub fn get_type(&self) -> Option<TypeReference> {
        match self {
            Self::Const(scalar_const_expr) => scalar_const_expr.get_type(),
            Self::FuncCall(func_call_expr) => func_call_expr.get_type(),
            Self::Arith(arith_expr) => arith_expr.get_type(),
            Self::TypeCast(type_cast_expr) => type_cast_expr.get_type(),
            Self::Case(case_expr) => case_expr.get_type(),
            Self::Boolean(_boolean_expr) => Self::builtin_boolean_type(),
            Self::NullTest(_null_test_expr) => Self::builtin_boolean_type(),
            Self::SubLink(sub_link_expr) => sub_link_expr.get_type(),
            Self::Coalesce(coalesce_expr) => coalesce_expr.get_type(),
            Self::MinMax(min_max_expr) => min_max_expr.get_type(),
            Self::Collate(collate_expr) => collate_expr.get_type(),
            Self::SqlValue(sql_value_expr) => sql_value_expr.get_type(),
            Self::Param(param_ref) => param_ref.get_type(),
            Self::NamedArg(named_arg_expr) => named_arg_expr.get_type(),
            Self::Indirection(ind) => ind.get_type(),
            Self::ImplicitCast(ice) => ice.get_type(),
            Self::Array(arr) => arr.get_type(),
            Self::Unexpected(_node) => None,
            Self::ColumnRef(cr) => cr.get_type(),
        }
    }

    /// determine expression value is nullable
    pub fn is_not_null(&self) -> Option<bool> {
        match self {
            Self::Const(scalar_const_expr) => scalar_const_expr.is_not_null(),
            Self::FuncCall(func_call_expr) => func_call_expr.is_not_null(),
            Self::Arith(arith_expr) => arith_expr.is_not_null(),
            Self::TypeCast(type_cast_expr) => type_cast_expr.is_not_null(),
            Self::Case(case_expr) => case_expr.is_not_null(),
            Self::Boolean(boolean_expr) => boolean_expr.is_not_null(),
            Self::NullTest(null_test_expr) => null_test_expr.is_not_null(),
            Self::SubLink(sub_link_expr) => sub_link_expr.is_not_null(),
            Self::Coalesce(coalesce_expr) => coalesce_expr.is_not_null(),
            Self::MinMax(min_max_expr) => min_max_expr.is_not_null(),
            Self::Collate(collate_expr) => collate_expr.is_not_null(),
            Self::SqlValue(sql_value_expr) => sql_value_expr.is_not_null(),
            Self::Param(param_ref) => param_ref.is_not_null(),
            Self::NamedArg(named_arg_expr) => named_arg_expr.is_not_null(),
            Self::Indirection(ind) => ind.is_not_null(),
            Self::ImplicitCast(ice) => ice.is_not_null(),
            Self::Array(arr) => arr.is_not_null(),
            Self::Unexpected(_node) => None,
            Self::ColumnRef(cr) => cr.is_not_null(),
        }
    }

    /// get column definition
    pub fn get_column_def(&self) -> Option<ColumnDefinition> {
        match self {
            Self::ColumnRef(c) => c.get_column_def().clone(),
            Self::TypeCast(_) | SemScalarExpr::FuncCall(_) => None,
            _ => todo!("not implemented {self:?}"),
        }
    }

    /// convert value_expr with implicit casting
    pub fn wrap_implicit_cast(taget_type: &TypeReference, value_expr: &SemScalarExpr) -> Self {
        Self::ImplicitCast(ImplicitCastExpr::new(value_expr, taget_type))
    }

    /// convert expression type with implicit cast
    pub fn implicit_cast_if_require<TParseContext>(
        context: &mut TParseContext,
        result_type: &TypeReference,
        expr: &mut SemScalarExpr,
    ) where
        TParseContext: ParseContext,
    {
        let Some(ty) = expr.get_type() else {
            return;
        };
        if &ty == result_type {
            return;
        };

        let Some(cast) = context.get_implicit_cast(&ty, result_type) else {
            context.report_problem(AnalysisProblem::implicit_cast_not_found(&ty, result_type));
            return;
        };
        if !cast.is_no_coversion() {
            *expr = SemScalarExpr::wrap_implicit_cast(result_type, expr);
        }
    }

    /// get column name
    pub fn get_column_name(&self) -> String {
        match self {
            Self::Const(_scalar_const_expr) => String::from(""),
            Self::ColumnRef(cr) => cr.get_column_name().to_string(),
            Self::FuncCall(_func_call_expr) => String::from(""),
            Self::Arith(_arith_expr) => todo!(),
            Self::TypeCast(_type_cast_expr) => String::from(""),
            Self::Case(_case_expr) => todo!(),
            Self::Boolean(_boolean_expr) => String::from(""),
            Self::NullTest(_null_test_expr) => todo!(),
            Self::SubLink(_sub_link_expr) => todo!(),
            Self::Coalesce(_coalesce_expr) => todo!(),
            Self::MinMax(_min_max_expr) => todo!(),
            Self::Collate(_collate_expr) => todo!(),
            Self::SqlValue(_sql_value_expr) => todo!(),
            Self::Param(_param_ref) => todo!(),
            Self::NamedArg(_named_arg_expr) => todo!(),
            Self::Indirection(_indirection_expr) => todo!(),
            Self::ImplicitCast(_implicit_cast_expr) => todo!(),
            Self::Array(_arr) => todo!(),
            Self::Unexpected(_node) => todo!(),
        }
    }

    /// test expression is array and report
    pub fn require_array<TParseContext>(&self, context: &mut TParseContext)
    where
        TParseContext: ParseContext,
    {
        if let Some(ty) = self.get_type()
            && let Some(is_array) = ty.is_array()
            && !is_array
        {
            context.report_problem(AnalysisProblem::array_required(self, &ty));
        }
    }

    /// test fits array index
    pub fn fits_array_index<TParseContext>(&self, context: &mut TParseContext)
    where
        TParseContext: ParseContext,
    {
        if let Some(ty) = self.get_type()
            && context
                .get_implicit_cast(&ty, &PgBuiltInType::int2())
                .is_none()
            && context
                .get_implicit_cast(&ty, &PgBuiltInType::int4())
                .is_none()
            && context
                .get_implicit_cast(&ty, &PgBuiltInType::int8())
                .is_none()
        {
            context.report_problem(AnalysisProblem::array_index_type_missmatch(&self, &ty));
        }
    }
}
