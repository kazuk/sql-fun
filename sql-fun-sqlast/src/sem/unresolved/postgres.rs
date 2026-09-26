mod query;

use crate::sem::unresolved::{
    Access, BooleanExpr, BooleanOperator, Call, CallKind, Case, CaseArm, Cast, Collate, Expr,
    Indirection, Literal, Name, NamedArgument, NullTest, OperatorExpr, OperatorKind, Order,
    Parameter, Query, Slice, SqlValue, Subquery, SubqueryKind, TruthTest, TypeName, Window,
};
use crate::syn::{ListOpt, Opt};
use crate::{StringSpan, sem::AnalysisError, syn};

use ::sql_fun_core::IVec;

pub(in crate::sem::unresolved) fn required<T>(
    value: Option<T>,
    field: &str,
) -> Result<T, AnalysisError> {
    match value {
        Some(value) => Ok(value),
        None => AnalysisError::raise_unexpected_none(field),
    }
}

fn nonempty(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}

impl Name {
    pub(in crate::sem::unresolved) fn postgres(
        nodes: syn::NodeListOpt,
    ) -> Result<Self, AnalysisError> {
        let mut parts = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            parts.push(required(node.as_string().get_sval(), "name component")?);
        }
        Ok(Self::from_parts(&parts))
    }
}

impl Expr {
    pub(crate) fn postgres_function(
        value: &syn::FuncCall,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self::Call(Box::new(Call::postgres(value, tokens)?)))
    }
    pub(crate) fn postgres(
        node: &syn::Node,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        if node.is_a_const() {
            return Ok(Self::Literal(Literal::postgres(&node.as_a_const())));
        }
        if let Some(column) = node.as_column_ref().as_inner() {
            let mut parts = Vec::new();
            let mut star = false;
            for field in column.get_fields().as_inner().unwrap_or_default() {
                if field.is_a_star() {
                    star = true;
                } else {
                    parts.push(required(field.as_string().get_sval(), "column name")?);
                }
            }
            let name = Name::from_parts(&parts);
            return Ok(if star {
                Self::Star(name)
            } else {
                Self::Column(name)
            });
        }
        if let Some(call) = node.as_func_call().as_inner() {
            return Ok(Self::Call(Box::new(Call::postgres(&call, tokens)?)));
        }
        if let Some(operator) = node.as_a_expr().as_inner() {
            return Ok(Self::Operator(Box::new(OperatorExpr {
                name: Name::postgres(operator.get_name())?,
                kind: OperatorKind::postgres(required(
                    operator.get_kind().as_inner(),
                    "operator kind",
                )?)?,
                left: Self::postgres_optional(operator.get_lexpr(), tokens)?,
                right: Self::postgres_optional(operator.get_rexpr(), tokens)?,
                escape: None,
            })));
        }
        if let Some(boolean) = node.as_bool_expr().as_inner() {
            return Ok(Self::Boolean(Box::new(BooleanExpr {
                operator: BooleanOperator::postgres(required(
                    boolean.get_boolop().as_inner(),
                    "boolean operator",
                )?)?,
                arguments: Self::postgres_list(boolean.get_args(), tokens)?,
            })));
        }
        if let Some(test) = node.as_null_test().as_inner() {
            return Ok(Self::NullTest(Box::new(NullTest {
                argument: Self::postgres(&test.get_arg(), tokens)?,
                negated: matches!(
                    test.get_nulltesttype().as_inner(),
                    Some(syn::NullTestType::IsNotNull)
                ),
                row: test.get_argisrow(),
            })));
        }
        if let Some(test) = node.as_boolean_test().as_inner() {
            let (value, negated) = match required(test.get_booltesttype().as_inner(), "truth test")?
            {
                syn::BoolTestType::IsTrue => (Some(true), false),
                syn::BoolTestType::IsNotTrue => (Some(true), true),
                syn::BoolTestType::IsFalse => (Some(false), false),
                syn::BoolTestType::IsNotFalse => (Some(false), true),
                syn::BoolTestType::IsUnknown => (None, false),
                syn::BoolTestType::IsNotUnknown => (None, true),
                _ => return AnalysisError::raise_unexpected_input("truth test"),
            };
            return Ok(Self::TruthTest(Box::new(TruthTest {
                argument: Self::postgres(&test.get_arg(), tokens)?,
                value,
                negated,
            })));
        }
        if let Some(cast) = node.as_type_cast().as_inner() {
            return Ok(Self::Cast(Box::new(Cast {
                argument: Self::postgres(&cast.get_arg(), tokens)?,
                target: TypeName::postgres(
                    &required(cast.get_type_name().as_inner(), "cast type")?,
                    tokens,
                )?,
            })));
        }
        if let Some(collate) = node.as_collate_clause().as_inner() {
            return Ok(Self::Collate(Box::new(Collate {
                argument: Self::postgres(&collate.get_arg(), tokens)?,
                name: Name::postgres(collate.get_collname())?,
            })));
        }
        if let Some(case) = node.as_case_expr().as_inner() {
            return Self::postgres_case(&case, tokens);
        }
        if let Some(link) = node.as_sub_link().as_inner() {
            return Ok(Self::Subquery(Box::new(Subquery {
                query: Query::postgres(
                    &required(link.get_subselect().as_select_stmt().as_inner(), "subquery")?,
                    tokens,
                )?,
                kind: SubqueryKind::postgres(required(
                    link.get_sub_link_type().as_inner(),
                    "subquery kind",
                )?)?,
                test: Self::postgres_optional(link.get_testexpr(), tokens)?,
                operator: Name::postgres(link.get_oper_name())?,
            })));
        }
        if let Some(array) = node.as_a_array_expr().as_inner() {
            return Ok(Self::Array(Self::postgres_list(
                array.get_elements(),
                tokens,
            )?));
        }
        if let Some(row) = node.as_row_expr().as_inner() {
            return Ok(Self::Row(Self::postgres_list(row.get_args(), tokens)?));
        }
        if let Some(list) = node.as_list().as_inner() {
            return Ok(Self::List(Self::postgres_list(list.get_items(), tokens)?));
        }
        if let Some(indirection) = node.as_a_indirection().as_inner() {
            return Ok(Self::Indirection(Box::new(Indirection {
                argument: Self::postgres(&indirection.get_arg(), tokens)?,
                items: Access::postgres_list(indirection.get_indirection(), tokens)?,
            })));
        }
        if let Some(argument) = node.as_named_arg_expr().as_inner() {
            return Ok(Self::NamedArgument(Box::new(NamedArgument {
                name: argument.get_name(),
                argument: Self::postgres(&argument.get_arg(), tokens)?,
            })));
        }
        if let Some(parameter) = node.as_param_ref().as_inner() {
            return Ok(Self::Parameter(Parameter {
                position: parameter.get_number() as u32,
                name: None,
                type_name: None,
            }));
        }
        if let Some(value) = node.as_sqlvalue_function().as_inner() {
            return Ok(Self::SqlValue(SqlValue::postgres(&value)?));
        }
        if let Some(coalesce) = node.as_coalesce_expr().as_inner() {
            return Self::postgres_coalesce(&coalesce, tokens);
        }
        if let Some(minmax) = node.as_min_max_expr().as_inner() {
            let name = match minmax.get_op().as_inner() {
                Some(syn::MinMaxOp::IsGreatest) => "greatest",
                Some(syn::MinMaxOp::IsLeast) => "least",
                _ => return AnalysisError::raise_unexpected_input("min/max operation"),
            };
            return Ok(Self::Call(Box::new(Call::special(
                name,
                Self::postgres_list(minmax.get_args(), tokens)?,
            ))));
        }
        if node.is_set_to_default() {
            return Ok(Self::Default);
        }
        if let Some(cursor) = node.as_current_of_expr().as_inner() {
            return Ok(Self::CurrentOf(super::CurrentOf {
                name: cursor.get_cursor_name(),
                parameter: cursor.get_cursor_param(),
            }));
        }
        if node.is_merge_support_func() {
            return Ok(Self::MergeAction);
        }
        if let Some(document) = super::document::DocumentExpr::postgres(node, tokens)? {
            return Ok(Self::Document(document));
        }
        if let Some(grouping) = node.as_grouping_func().as_inner() {
            return Ok(Self::Grouping(Self::postgres_list(
                grouping.get_args(),
                tokens,
            )?));
        }
        if let Some(value) = Literal::postgres_option(node) {
            return Ok(Self::Literal(value));
        }
        AnalysisError::raise_unexpected_input(&format!("expression syntax: {node:?}"))
    }

    pub(in crate::sem::unresolved) fn postgres_optional(
        node: syn::Node,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Option<Self>, AnalysisError> {
        node.as_inner()
            .map(|node| Self::postgres(&node, tokens))
            .transpose()
    }

    pub(in crate::sem::unresolved) fn postgres_list(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        nodes
            .as_inner()
            .unwrap_or_default()
            .into_iter()
            .map(|node| Self::postgres(&node, tokens))
            .collect()
    }
}

impl Literal {
    pub(in crate::sem::unresolved) fn postgres(value: &syn::AConst) -> Self {
        if let Some(value) = value.as_ival().get_ival() {
            Self::Integer(value)
        } else if let Some(value) = value.as_fval().get_fval() {
            Self::Numeric(value)
        } else if let Some(value) = value.as_boolval().get_boolval() {
            Self::Boolean(value)
        } else if let Some(value) = value.as_sval().get_sval() {
            Self::String(value)
        } else if let Some(value) = value.as_bsval().get_bsval() {
            Self::BitString(value)
        } else {
            Self::Null
        }
    }
}

impl OperatorKind {
    pub(in crate::sem::unresolved) fn postgres(
        kind: syn::AExprKind,
    ) -> Result<Self, AnalysisError> {
        Ok(match kind {
            syn::AExprKind::AexprOp => Self::Ordinary,
            syn::AExprKind::AexprOpAny => Self::Any,
            syn::AExprKind::AexprOpAll => Self::All,
            syn::AExprKind::AexprDistinct => Self::Distinct,
            syn::AExprKind::AexprNotDistinct => Self::NotDistinct,
            syn::AExprKind::AexprNullif => Self::NullIf,
            syn::AExprKind::AexprIn => Self::In,
            syn::AExprKind::AexprLike => Self::Like,
            syn::AExprKind::AexprIlike => Self::ILike,
            syn::AExprKind::AexprSimilar => Self::Similar,
            syn::AExprKind::AexprBetween => Self::Between,
            syn::AExprKind::AexprNotBetween => Self::NotBetween,
            syn::AExprKind::AexprBetweenSym => Self::BetweenSymmetric,
            syn::AExprKind::AexprNotBetweenSym => Self::NotBetweenSymmetric,
            _ => return AnalysisError::raise_unexpected_input("operator kind"),
        })
    }
}

impl BooleanOperator {
    pub(in crate::sem::unresolved) fn postgres(
        kind: syn::BoolExprType,
    ) -> Result<Self, AnalysisError> {
        match kind {
            syn::BoolExprType::AndExpr => Ok(Self::And),
            syn::BoolExprType::OrExpr => Ok(Self::Or),
            syn::BoolExprType::NotExpr => Ok(Self::Not),
            _ => AnalysisError::raise_unexpected_input("boolean operator"),
        }
    }
}

impl SubqueryKind {
    pub(in crate::sem::unresolved) fn postgres(
        kind: syn::SubLinkType,
    ) -> Result<Self, AnalysisError> {
        match kind {
            syn::SubLinkType::ExistsSublink => Ok(Self::Exists),
            syn::SubLinkType::AllSublink => Ok(Self::All),
            syn::SubLinkType::AnySublink => Ok(Self::Any),
            syn::SubLinkType::RowcompareSublink => Ok(Self::Row),
            syn::SubLinkType::ExprSublink => Ok(Self::Scalar),
            syn::SubLinkType::MultiexprSublink => Ok(Self::MultiExpression),
            syn::SubLinkType::ArraySublink => Ok(Self::Array),
            _ => AnalysisError::raise_unexpected_input("subquery kind"),
        }
    }
}

impl TypeName {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::TypeName,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let mut bounds = Vec::new();
        for bound in value.get_array_bounds().as_inner().unwrap_or_default() {
            let bound = required(bound.as_integer().get_ival(), "array bound")?;
            bounds.push(if bound < 0 { None } else { Some(bound) });
        }
        Ok(Self {
            name: Name::postgres(value.get_names())?,
            modifiers: Expr::postgres_list(value.get_typmods(), tokens)?,
            array_bounds: bounds,
            set_of: value.get_setof(),
            percent_type: value.get_pct_type(),
            location: source_span(value.get_location(), tokens),
        })
    }
}

fn source_span(location: i32, tokens: &IVec<syn::ScanToken>) -> Option<StringSpan> {
    if location < 0 {
        return None;
    }
    let mut span = None;
    let mut expect_name = true;
    for token in tokens {
        if token.get_start() < location
            || matches!(
                token.get_token().as_inner(),
                Some(syn::Token::SqlComment | syn::Token::CComment)
            )
        {
            continue;
        }
        if expect_name && token.is_ident() || !expect_name && token.is_dot() {
            let next = StringSpan::new(
                token.get_start() as usize,
                (token.get_end() - token.get_start()) as usize,
            );
            if let Some(span) = &mut span {
                StringSpan::extend(span, &next);
            } else {
                span = Some(next);
            }
            expect_name = !expect_name;
        } else {
            break;
        }
    }
    span
}

#[cfg(test)]
mod test_source_span {
    use crate::PostgresDialect;
    use crate::sem::unresolved::postgres::source_span;

    #[test]
    fn preserves_quoted_qualified_name_and_intervening_comment() -> testresult::TestResult {
        let sql = "select \"schema name\" /* source */ . \"function name\"(1)";
        let tokens = PostgresDialect::scan(sql)?;
        let span = source_span(7, &tokens).expect("function name span");
        assert_eq!(
            span.get_from(sql),
            Some("\"schema name\" /* source */ . \"function name\""),
        );
        assert!(source_span(-1, &tokens).is_none());
        Ok(())
    }
}

impl Call {
    fn special(name: &str, arguments: Vec<Expr>) -> Self {
        Self {
            kind: CallKind::special(name),
            name: Name::local(name),
            arguments,
            star: false,
            distinct: false,
            variadic: false,
            within_group: false,
            order: Vec::new(),
            filter: None,
            over: None,
            location: None,
            within_order: Vec::new(),
        }
    }

    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::FuncCall,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            name: Name::postgres(value.get_funcname())?,
            kind: CallKind::ordinary(),
            within_order: Vec::new(),
            arguments: Expr::postgres_list(value.get_args(), tokens)?,
            star: value.get_agg_star(),
            distinct: value.get_agg_distinct(),
            variadic: value.get_func_variadic(),
            within_group: value.get_agg_within_group(),
            order: Order::postgres_list(value.get_agg_order(), tokens)?,
            filter: Expr::postgres_optional(value.get_agg_filter(), tokens)?,
            over: value
                .get_over()
                .as_inner()
                .map(|window| Window::postgres(&window, tokens))
                .transpose()?,
            location: source_span(value.get_location(), tokens),
        })
    }
}

impl Access {
    pub(in crate::sem::unresolved) fn postgres_list(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut items = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            items.push(if let Some(indices) = node.as_a_indices().as_inner() {
                if indices.get_is_slice() {
                    Self::Slice(Slice {
                        lower: Expr::postgres_optional(indices.get_lidx(), tokens)?,
                        upper: Expr::postgres_optional(indices.get_uidx(), tokens)?,
                    })
                } else {
                    Self::Index(Expr::postgres(&indices.get_uidx(), tokens)?)
                }
            } else if node.is_a_star() {
                Self::Star
            } else {
                Self::Field(required(node.as_string().get_sval(), "field name")?)
            });
        }
        Ok(items)
    }
}

impl SqlValue {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::SqlValueFunction,
    ) -> Result<Self, AnalysisError> {
        let (name, precision) = match required(value.get_op().as_inner(), "SQL value")? {
            syn::SqlValueFunctionOp::SvfopCurrentDate => ("current_date", false),
            syn::SqlValueFunctionOp::SvfopCurrentTime => ("current_time", false),
            syn::SqlValueFunctionOp::SvfopCurrentTimeN => ("current_time", true),
            syn::SqlValueFunctionOp::SvfopCurrentTimestamp => ("current_timestamp", false),
            syn::SqlValueFunctionOp::SvfopCurrentTimestampN => ("current_timestamp", true),
            syn::SqlValueFunctionOp::SvfopLocaltime => ("localtime", false),
            syn::SqlValueFunctionOp::SvfopLocaltimeN => ("localtime", true),
            syn::SqlValueFunctionOp::SvfopLocaltimestamp => ("localtimestamp", false),
            syn::SqlValueFunctionOp::SvfopLocaltimestampN => ("localtimestamp", true),
            syn::SqlValueFunctionOp::SvfopCurrentRole => ("current_role", false),
            syn::SqlValueFunctionOp::SvfopCurrentUser => ("current_user", false),
            syn::SqlValueFunctionOp::SvfopUser => ("user", false),
            syn::SqlValueFunctionOp::SvfopSessionUser => ("session_user", false),
            syn::SqlValueFunctionOp::SvfopCurrentCatalog => ("current_catalog", false),
            syn::SqlValueFunctionOp::SvfopCurrentSchema => ("current_schema", false),
            _ => return AnalysisError::raise_unexpected_input("SQL value"),
        };
        Ok(Self {
            name: name.to_owned(),
            precision: if precision {
                Some(value.get_typmod())
            } else {
                None
            },
        })
    }
}

impl Literal {
    fn postgres_option(node: &syn::Node) -> Option<Self> {
        if let Some(value) = node.as_string().get_sval() {
            Some(Self::String(value))
        } else if let Some(value) = node.as_integer().get_ival() {
            Some(Self::Integer(value))
        } else if let Some(value) = node.as_float().get_fval() {
            Some(Self::Numeric(value))
        } else {
            node.as_boolean().get_boolval().map(Self::Boolean)
        }
    }
}

impl Expr {
    pub(crate) fn postgres_case(
        case: &syn::CaseExpr,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let mut arms = Vec::new();
        for arm in case.get_args().as_inner().unwrap_or_default() {
            let arm = required(arm.as_case_when().as_inner(), "case arm")?;
            arms.push(CaseArm {
                condition: Self::postgres(&arm.get_expr(), tokens)?,
                result: Self::postgres(&arm.get_result(), tokens)?,
            });
        }
        Ok(Self::Case(Box::new(Case {
            argument: Self::postgres_optional(case.get_arg(), tokens)?,
            arms,
            otherwise: Self::postgres_optional(case.get_defresult(), tokens)?,
        })))
    }
}

impl Expr {
    pub(crate) fn postgres_coalesce(
        coalesce: &syn::CoalesceExpr,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self::Call(Box::new(Call::special(
            "coalesce",
            Self::postgres_list(coalesce.get_args(), tokens)?,
        ))))
    }
}
