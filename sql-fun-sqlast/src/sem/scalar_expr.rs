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

use sql_fun_core::IVec;

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

use crate::{
    sem::{
        AnalysisError, AnalysisProblem, FromClause, ParseContext, PgBuiltInType, TypeReference,
        WithClause, create_table::ColumnDefinition,
    },
    syn::ScanToken,
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
        tokens: &IVec<ScanToken>,
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
    /// `ImplicitCast` is not in Syntax level AST.
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
    #[must_use]
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
    #[must_use]
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
}

#[cfg(test)]
mod test_get_type_and_is_not_null {
    use super::{ScalarConstExpr, SemScalarExpr};
    use crate::sem::PgBuiltInType;

    #[test]
    fn get_type_and_is_not_null_for_const() {
        let int_expr = SemScalarExpr::Const(ScalarConstExpr::new_integer(10));
        let null_expr = SemScalarExpr::Const(ScalarConstExpr::Null);

        assert_eq!(int_expr.get_type(), Some(PgBuiltInType::int4()));
        assert_eq!(int_expr.is_not_null(), Some(true));
        assert_eq!(null_expr.get_type(), None);
        assert_eq!(null_expr.is_not_null(), Some(false));
    }
}

impl SemScalarExpr {
    /// get column definition
    #[must_use]
    pub fn get_column_def(&self) -> Option<ColumnDefinition> {
        match self {
            Self::ColumnRef(c) => c.get_column_def().clone(),
            Self::TypeCast(_) | SemScalarExpr::FuncCall(_) => None,
            _ => todo!("not implemented {self:?}"),
        }
    }

    /// get column name
    #[must_use]
    pub fn get_column_name(&self) -> String {
        match self {
            Self::Const(_scalar_const_expr) => String::new(),
            Self::ColumnRef(cr) => cr.get_column_name().to_string(),
            Self::FuncCall(_func_call_expr) => String::new(),
            Self::Arith(_arith_expr) => todo!(),
            Self::TypeCast(_type_cast_expr) => String::new(),
            Self::Case(_case_expr) => todo!(),
            Self::Boolean(_boolean_expr) => String::new(),
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
}

#[cfg(test)]
mod test_get_column_def_and_name {
    use super::{ColumnReferenceExpr, SemScalarExpr, TableColumnRef, TypeReference};
    use crate::sem::{
        FullName, PgBuiltInType,
        create_table::{ColumnDefinition, ColumnName, TableName},
        data_source::AliasName,
    };

    fn make_table_column_expr(
        column_name: &str,
        column_type: &TypeReference,
        is_not_null: Option<bool>,
    ) -> SemScalarExpr {
        let alias = AliasName::from("t");
        let column = ColumnName::from(column_name);
        let table_name = TableName::from(FullName::with_schema("public", "tbl"));
        let col_def = ColumnDefinition::new(&Some(column.clone()), Some(column_type), is_not_null);
        let table_column = TableColumnRef::new(&alias, &column, &table_name, Some(&col_def), false);
        SemScalarExpr::ColumnRef(ColumnReferenceExpr::TableColumn(table_column))
    }

    #[test]
    fn get_column_def_and_name_from_column_ref() {
        let col_type = PgBuiltInType::int4();
        let expr = make_table_column_expr("col", &col_type, Some(true));

        let col_def = expr.get_column_def().expect("column definition");
        assert_eq!(col_def.get_type(), Some(col_type));
        assert_eq!(expr.get_column_name(), "col");
    }
}

impl SemScalarExpr {
    /// convert `value_expr` with implicit casting
    #[must_use]
    pub fn wrap_implicit_cast(taget_type: &TypeReference, value_expr: &SemScalarExpr) -> Self {
        Self::ImplicitCast(ImplicitCastExpr::new(value_expr, taget_type))
    }
}

#[cfg(test)]
mod test_wrap_implicit_cast {
    use super::{ScalarConstExpr, SemScalarExpr};
    use crate::sem::PgBuiltInType;

    #[test]
    fn wrap_implicit_cast_sets_type() {
        let base_expr = SemScalarExpr::Const(ScalarConstExpr::new_integer(1));
        let target_type = PgBuiltInType::int8();
        let wrapped = SemScalarExpr::wrap_implicit_cast(&target_type, &base_expr);

        match wrapped {
            SemScalarExpr::ImplicitCast(_) => {
                assert_eq!(wrapped.get_type(), Some(target_type));
            }
            _ => panic!("expected implicit cast expression"),
        }
    }
}

impl SemScalarExpr {
    /// convert expression type with implicit cast
    pub fn implicit_cast_if_require<TParseContext>(
        context: &mut TParseContext,
        result_type: &TypeReference,
        expr: &mut SemScalarExpr,
    ) -> Result<(), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(ty) = expr.get_type() else {
            return Ok(());
        };
        if &ty == result_type {
            return Ok(());
        }

        let Some(cast) = context.get_implicit_cast(&ty, result_type) else {
            context.report_problem(AnalysisProblem::implicit_cast_not_found(&ty, result_type))?;
            return Ok(());
        };
        if !cast.is_no_coversion() {
            *expr = SemScalarExpr::wrap_implicit_cast(result_type, expr);
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_implicit_cast_if_require {
    use super::{ScalarConstExpr, SemScalarExpr};
    use crate::sem::{CastContext, CastDefinition, PgBuiltInType};
    use crate::test_helpers::TestParseContext;

    #[test]
    fn implicit_cast_if_require_no_change_when_same_type() {
        let mut context = TestParseContext::default();
        let target_type = PgBuiltInType::int4();
        let mut expr = SemScalarExpr::Const(ScalarConstExpr::new_integer(3));

        SemScalarExpr::implicit_cast_if_require(&mut context, &target_type, &mut expr)
            .expect("implicit cast check");

        assert!(matches!(expr, SemScalarExpr::Const(_)));
        assert_eq!(context.reported_problem_count(), 0);
    }

    #[test]
    fn implicit_cast_if_require_wraps_on_available_cast() {
        let mut context = TestParseContext::default();
        let source_type = PgBuiltInType::int4();
        let target_type = PgBuiltInType::int8();
        context.set_get_implicit_cast_result(
            &source_type,
            &target_type,
            Some(CastDefinition::new(CastContext::Implicit)),
        );

        let mut expr = SemScalarExpr::Const(ScalarConstExpr::new_integer(3));
        SemScalarExpr::implicit_cast_if_require(&mut context, &target_type, &mut expr)
            .expect("implicit cast check");

        assert!(matches!(expr, SemScalarExpr::ImplicitCast(_)));
        assert_eq!(expr.get_type(), Some(target_type));
        assert_eq!(context.reported_problem_count(), 0);
    }

    #[test]
    fn implicit_cast_if_require_reports_when_missing_cast() {
        let mut context = TestParseContext::default();
        let target_type = PgBuiltInType::int8();
        let mut expr = SemScalarExpr::Const(ScalarConstExpr::new_integer(3));

        SemScalarExpr::implicit_cast_if_require(&mut context, &target_type, &mut expr)
            .expect("implicit cast check");

        assert!(matches!(expr, SemScalarExpr::Const(_)));
        assert_eq!(context.reported_problem_count(), 1);
    }
}

impl SemScalarExpr {
    /// test expression is array and report
    pub fn require_array<TParseContext>(
        &self,
        context: &mut TParseContext,
    ) -> Result<(), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        if let Some(ty) = self.get_type()
            && let Some(is_array) = ty.is_array()
            && !is_array
        {
            context.report_problem(AnalysisProblem::array_required(self, &ty))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_require_array {
    use super::{ImplicitCastExpr, ScalarConstExpr, SemScalarExpr, TypeReference};
    use crate::sem::PgBuiltInType;
    use crate::test_helpers::TestParseContext;

    #[test]
    fn require_array_reports_for_non_array() {
        let mut context = TestParseContext::default();
        let expr = SemScalarExpr::Const(ScalarConstExpr::new_integer(5));

        expr.require_array(&mut context).expect("require array");
        assert_eq!(context.reported_problem_count(), 1);
    }

    #[test]
    fn require_array_accepts_array_type() {
        let mut context = TestParseContext::default();
        let base_expr = SemScalarExpr::Const(ScalarConstExpr::new_integer(5));
        let array_type = TypeReference::concrete_type_ref(PgBuiltInType::int4().full_name(), true);
        let expr = SemScalarExpr::ImplicitCast(ImplicitCastExpr::new(&base_expr, &array_type));

        expr.require_array(&mut context).expect("require array");
        assert_eq!(context.reported_problem_count(), 0);
    }
}

impl SemScalarExpr {
    /// test fits array index
    pub fn fits_array_index<TParseContext>(
        &self,
        context: &mut TParseContext,
    ) -> Result<(), AnalysisError>
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
            context.report_problem(AnalysisProblem::array_index_type_missmatch(self, &ty))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_fits_array_index {
    use super::{ScalarConstExpr, SemScalarExpr};
    use crate::sem::{CastContext, CastDefinition, PgBuiltInType};
    use crate::test_helpers::TestParseContext;

    #[test]
    fn fits_array_index_reports_when_not_castable() {
        let mut context = TestParseContext::default();
        let expr = SemScalarExpr::Const(ScalarConstExpr::String("value".to_string()));

        expr.fits_array_index(&mut context)
            .expect("array index check");
        assert_eq!(context.reported_problem_count(), 1);
    }

    #[test]
    fn fits_array_index_accepts_castable_type() {
        let mut context = TestParseContext::default();
        let int4 = PgBuiltInType::int4();
        context.set_get_implicit_cast_result(
            &int4,
            &int4,
            Some(CastDefinition::new(CastContext::NoConversion)),
        );
        let expr = SemScalarExpr::Const(ScalarConstExpr::new_integer(7));

        expr.fits_array_index(&mut context)
            .expect("array index check");
        assert_eq!(context.reported_problem_count(), 0);
    }
}
