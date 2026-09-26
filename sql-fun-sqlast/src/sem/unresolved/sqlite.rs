mod query;

use crate::sem::unresolved::{
    Access, BooleanExpr, BooleanOperator, Call, CallKind, Case, CaseArm, Cast, Collate, Expr,
    InTable, Indirection, Literal, Name, NullTest, OperatorExpr, OperatorKind, Order, Parameter,
    Query, Raise, SqlValue, Subquery, SubqueryKind, TypeName, Window,
};
use crate::{sem::AnalysisError, syn::sqlite as syn};

impl Name {
    fn sqlite(value: &syn::QualifiedName) -> Self {
        let mut parts = Vec::new();
        if let Some(database) = value.db_name() {
            parts.push(database.value().to_owned());
        }
        parts.push(value.name().value().to_owned());
        Self::from_parts(&parts)
    }
}

impl Expr {
    pub(crate) fn sqlite(value: &syn::Expr) -> Result<Self, AnalysisError> {
        Ok(match value {
            syn::Expr::Literal(value) => Self::sqlite_literal(value),
            syn::Expr::Id(name) | syn::Expr::Name(name) => Self::Column(Name::local(name.value())),
            syn::Expr::Qualified(name) => Self::Column(Name::from_parts(&[
                name.table().value().to_owned(),
                name.column().value().to_owned(),
            ])),
            syn::Expr::DoublyQualified(name) => Self::Column(Name::from_parts(&[
                name.schema().value().to_owned(),
                name.table().value().to_owned(),
                name.column().value().to_owned(),
            ])),
            syn::Expr::Parenthesized(value) => Self::sqlite_parenthesized(value)?,
            syn::Expr::Unary(value) => Self::sqlite_unary(value)?,
            syn::Expr::Binary(value) => Self::sqlite_binary(value)?,
            syn::Expr::Between(value) => Self::sqlite_between(value)?,
            syn::Expr::IsNull(value) => Self::sqlite_is_null(value)?,
            syn::Expr::NotNull(value) => Self::sqlite_not_null(value)?,
            syn::Expr::Case(value) => {
                let mut arms = Vec::new();
                for arm in value.when_then_pairs() {
                    arms.push(CaseArm {
                        condition: Self::sqlite(arm.when())?,
                        result: Self::sqlite(arm.then())?,
                    });
                }
                Self::Case(Box::new(Case {
                    argument: value.base().as_deref().map(Self::sqlite).transpose()?,
                    arms,
                    otherwise: value.else_expr().as_deref().map(Self::sqlite).transpose()?,
                }))
            }
            syn::Expr::Cast(value) => Self::sqlite_cast(value)?,
            syn::Expr::Collate(value) => Self::sqlite_collate(value)?,
            syn::Expr::FunctionCall(value) => Self::Call(Box::new(Call::sqlite(value)?)),
            syn::Expr::FunctionCallStar(value) => {
                let mut call = Call::sqlite_base(value.name().value());
                call.star = true;
                call.sqlite_tail(value.filter_over())?;
                Self::Call(Box::new(call))
            }
            syn::Expr::Subquery(query) => Self::Subquery(Box::new(Subquery {
                query: Query::sqlite(query)?,
                kind: SubqueryKind::scalar(),
                test: None,
                operator: Name::default(),
            })),
            syn::Expr::Exists(query) => Self::Subquery(Box::new(Subquery {
                query: Query::sqlite(query)?,
                kind: SubqueryKind::exists(),
                test: None,
                operator: Name::default(),
            })),
            syn::Expr::InList(value) => Self::Operator(Box::new(OperatorExpr {
                name: Name::default(),
                kind: if *value.not() {
                    OperatorKind::not_in()
                } else {
                    OperatorKind::in_list()
                },
                left: Some(Self::sqlite(value.lhs())?),
                right: Some(Self::List(Self::sqlite_list(value.rhs())?)),
                escape: None,
            })),
            syn::Expr::InSelect(value) => {
                let expr = Self::Subquery(Box::new(Subquery {
                    query: Query::sqlite(value.rhs())?,
                    kind: SubqueryKind::any(),
                    test: Some(Self::sqlite(value.lhs())?),
                    operator: Name::local("="),
                }));
                if *value.not() {
                    Self::Boolean(Box::new(BooleanExpr {
                        operator: BooleanOperator::not(),
                        arguments: vec![expr],
                    }))
                } else {
                    expr
                }
            }
            syn::Expr::InTable(value) => Self::InTable(Box::new(InTable {
                argument: Self::sqlite(value.lhs())?,
                negated: *value.not(),
                table: Name::sqlite(value.rhs()),
                arguments: Self::sqlite_list(value.args())?,
            })),
            syn::Expr::Like(value) => Self::Operator(Box::new(OperatorExpr {
                name: Name::local(match value.op() {
                    syn::LikeOperator::Like => "like",
                    syn::LikeOperator::Glob => "glob",
                    syn::LikeOperator::Match => "match",
                    syn::LikeOperator::Regexp => "regexp",
                }),
                kind: if *value.not() {
                    OperatorKind::not_like()
                } else {
                    OperatorKind::like()
                },
                left: Some(Self::sqlite(value.lhs())?),
                right: Some(Self::sqlite(value.rhs())?),
                escape: value.escape().as_deref().map(Self::sqlite).transpose()?,
            })),
            syn::Expr::Variable(value) => Self::Parameter(Parameter {
                position: value.index().get(),
                name: value.name().clone(),
                type_name: value.col_type().clone(),
            }),
            syn::Expr::Raise(value) => Self::Raise(Box::new(Raise {
                action: super::RaiseAction::sqlite(value.resolve_type()),
                message: value.expr().as_deref().map(Self::sqlite).transpose()?,
            })),
            syn::Expr::Array(value) => Self::Array(Self::sqlite_list(value.syntax_elements())?),
            syn::Expr::Subscript(value) => Self::Indirection(Box::new(Indirection {
                argument: Self::sqlite(value.syntax_base())?,
                items: vec![Access::sqlite_index(value.syntax_index())?],
            })),
            syn::Expr::FieldAccess(value) => Self::Indirection(Box::new(Indirection {
                argument: Self::sqlite(value.base())?,
                items: vec![Access::sqlite_field(value.field())],
            })),
            syn::Expr::Default => Self::Default,
            // These are Turso planner outputs, never context-free SQL parser inputs.
            syn::Expr::Column(_)
            | syn::Expr::RowId(_)
            | syn::Expr::Register(_)
            | syn::Expr::SubqueryResult(_) => {
                return AnalysisError::raise_unexpected_input(
                    "resolved Turso expression in syntax input",
                );
            }
        })
    }

    fn operator(name: &str, left: Option<Self>, right: Option<Self>) -> Self {
        Self::Operator(Box::new(OperatorExpr {
            name: Name::local(name),
            kind: OperatorKind::ordinary(),
            left,
            right,
            escape: None,
        }))
    }

    fn sqlite_list(values: &[syn::Expr]) -> Result<Vec<Self>, AnalysisError> {
        let mut expressions = Vec::new();
        for value in values {
            expressions.push(Self::sqlite(value)?);
        }
        Ok(expressions)
    }

    fn sqlite_literal(value: &syn::Literal) -> Self {
        match value {
            syn::Literal::CurrentDate(_) => Self::SqlValue(SqlValue {
                name: "current_date".to_owned(),
                precision: None,
            }),
            syn::Literal::CurrentTime(_) => Self::SqlValue(SqlValue {
                name: "current_time".to_owned(),
                precision: None,
            }),
            syn::Literal::CurrentTimestamp(_) => Self::SqlValue(SqlValue {
                name: "current_timestamp".to_owned(),
                precision: None,
            }),
            literal => Self::Literal(Literal::sqlite(literal)),
        }
    }
}

impl Literal {
    fn sqlite(value: &syn::Literal) -> Self {
        match value {
            syn::Literal::Numeric(value) => match value.value_text().parse::<i32>() {
                Ok(value) => Self::Integer(value),
                Err(_) => Self::Numeric(value.value_text().to_owned()),
            },
            syn::Literal::String(value) => {
                let value = value.value();
                Self::String(
                    if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
                        value[1..value.len() - 1].replace("''", "'")
                    } else {
                        value.to_owned()
                    },
                )
            }
            syn::Literal::Null(_) => Self::Null,
            syn::Literal::True(value) | syn::Literal::False(value) => Self::Boolean(value.value()),
            syn::Literal::Blob(value) => Self::Blob(value.value().to_owned()),
            syn::Literal::Keyword(value) => Self::Keyword(value.value().to_owned()),
            syn::Literal::CurrentDate(_)
            | syn::Literal::CurrentTime(_)
            | syn::Literal::CurrentTimestamp(_) => {
                unreachable!("SQL values are converted separately")
            }
        }
    }
}

impl super::RaiseAction {
    const fn sqlite(value: &syn::ResolveType) -> Self {
        match value {
            syn::ResolveType::Rollback => Self::Rollback,
            syn::ResolveType::Abort => Self::Abort,
            syn::ResolveType::Fail => Self::Fail,
            syn::ResolveType::Ignore => Self::Ignore,
            syn::ResolveType::Replace => Self::Replace,
        }
    }
}

impl TypeName {
    fn sqlite_empty() -> Self {
        Self {
            name: Name::default(),
            modifiers: Vec::new(),
            array_bounds: Vec::new(),
            set_of: false,
            percent_type: false,
            location: None,
        }
    }

    fn sqlite(value: &syn::Type) -> Result<Self, AnalysisError> {
        let modifiers = match value.size() {
            Some(syn::TypeSize::MaxSize(size)) => vec![Expr::sqlite(size)?],
            Some(syn::TypeSize::TypeSize(size, scale)) => {
                vec![Expr::sqlite(size)?, Expr::sqlite(scale)?]
            }
            None => Vec::new(),
        };
        Ok(Self {
            name: Name::local(value.name()),
            modifiers,
            ..Self::sqlite_empty()
        })
    }
}

impl Call {
    fn sqlite_base(name: &str) -> Self {
        Self {
            kind: CallKind::ordinary(),
            name: Name::local(name),
            arguments: Vec::new(),
            star: false,
            distinct: false,
            variadic: false,
            within_group: false,
            within_order: Vec::new(),
            order: Vec::new(),
            filter: None,
            over: None,
            location: None,
        }
    }

    fn sqlite(value: &syn::ExprFunctionCall) -> Result<Self, AnalysisError> {
        let mut call = Self::sqlite_base(value.name().value());
        call.arguments = Expr::sqlite_list(value.args())?;
        call.distinct = matches!(value.distinctness(), Some(syn::Distinctness::Distinct));
        call.order = Order::sqlite_list(value.order_by())?;
        if !value.syntax_within_group().is_empty() {
            call.within_group = true;
            call.within_order = Order::sqlite_list(value.syntax_within_group())?;
        }
        call.sqlite_tail(value.filter_over())?;
        Ok(call)
    }

    fn sqlite_tail(&mut self, tail: &syn::FunctionTail) -> Result<(), AnalysisError> {
        self.filter = tail
            .filter_clause()
            .as_deref()
            .map(Expr::sqlite)
            .transpose()?;
        self.over = tail
            .over_clause()
            .as_ref()
            .map(Window::sqlite_over)
            .transpose()?;
        Ok(())
    }
}

impl Access {
    fn sqlite_index(value: &syn::Expr) -> Result<Self, AnalysisError> {
        Ok(Self::Index(Expr::sqlite(value)?))
    }
    fn sqlite_field(value: &syn::Name) -> Self {
        Self::Field(value.value().to_owned())
    }
}

const fn sqlite_operator(value: &syn::Operator) -> &'static str {
    match value {
        syn::Operator::Add => "+",
        syn::Operator::Subtract => "-",
        syn::Operator::Multiply => "*",
        syn::Operator::Divide => "/",
        syn::Operator::Modulus => "%",
        syn::Operator::Equals => "=",
        syn::Operator::NotEquals => "<>",
        syn::Operator::Grater => ">",
        syn::Operator::GraterEquals => ">=",
        syn::Operator::Less => "<",
        syn::Operator::LessEqales => "<=",
        syn::Operator::Is => "is",
        syn::Operator::IsNot => "is not",
        syn::Operator::Concat => "||",
        syn::Operator::BitwiseAnd => "&",
        syn::Operator::BitwiseOr => "|",
        syn::Operator::BitwiseNot => "~",
        syn::Operator::LeftShift => "<<",
        syn::Operator::RightShift => ">>",
        syn::Operator::ArrowRight => "->",
        syn::Operator::ArrowRightShift => "->>",
        syn::Operator::ArrayContains => "@>",
        syn::Operator::ArrayOverlap => "&&",
        syn::Operator::And => "and",
        syn::Operator::Or => "or",
    }
}

impl Expr {
    pub(crate) fn sqlite_parenthesized(value: &syn::Parenthesized) -> Result<Self, AnalysisError> {
        Ok(Self::Parenthesized(Self::sqlite_list(value.expr())?))
    }

    pub(crate) fn sqlite_unary(value: &syn::ExprUnary) -> Result<Self, AnalysisError> {
        Ok({
            let argument = Self::sqlite(value.expr())?;
            match value.unary_operator() {
                syn::UnaryOperator::Not => Self::Boolean(Box::new(BooleanExpr {
                    operator: BooleanOperator::not(),
                    arguments: vec![argument],
                })),
                syn::UnaryOperator::Positive => Self::operator("+", None, Some(argument)),
                syn::UnaryOperator::Negative => Self::operator("-", None, Some(argument)),
                syn::UnaryOperator::BitwiseNot => Self::operator("~", None, Some(argument)),
            }
        })
    }

    pub(crate) fn sqlite_binary(value: &syn::ExprBinary) -> Result<Self, AnalysisError> {
        Ok({
            let left = Self::sqlite(value.left())?;
            let right = Self::sqlite(value.right())?;
            match value.operator() {
                syn::Operator::And => Self::Boolean(Box::new(BooleanExpr {
                    operator: BooleanOperator::and(),
                    arguments: vec![left, right],
                })),
                syn::Operator::Or => Self::Boolean(Box::new(BooleanExpr {
                    operator: BooleanOperator::or(),
                    arguments: vec![left, right],
                })),
                operator => Self::operator(sqlite_operator(operator), Some(left), Some(right)),
            }
        })
    }

    pub(crate) fn sqlite_between(value: &syn::ExprBetween) -> Result<Self, AnalysisError> {
        Ok(Self::Operator(Box::new(OperatorExpr {
            name: Name::default(),
            kind: if *value.not() {
                OperatorKind::not_between()
            } else {
                OperatorKind::between()
            },
            left: Some(Self::sqlite(value.lhs())?),
            right: Some(Self::List(vec![
                Self::sqlite(value.start())?,
                Self::sqlite(value.end())?,
            ])),
            escape: None,
        })))
    }

    pub(crate) fn sqlite_is_null(value: &syn::ExprIsNull) -> Result<Self, AnalysisError> {
        Ok(Self::NullTest(Box::new(NullTest {
            argument: Self::sqlite(value.expr())?,
            negated: false,
            row: false,
        })))
    }

    pub(crate) fn sqlite_not_null(value: &syn::ExprNotNull) -> Result<Self, AnalysisError> {
        Ok(Self::NullTest(Box::new(NullTest {
            argument: Self::sqlite(value.expr())?,
            negated: true,
            row: false,
        })))
    }

    pub(crate) fn sqlite_collate(value: &syn::ExprCollate) -> Result<Self, AnalysisError> {
        Ok(Self::Collate(Box::new(Collate {
            argument: Self::sqlite(value.expr())?,
            name: Name::local(value.name().value()),
        })))
    }
}

impl Expr {
    pub(crate) fn sqlite_cast(value: &syn::ExprCast) -> Result<Self, AnalysisError> {
        Ok({
            let target = match value.type_name() {
                Some(target) => TypeName::sqlite(target)?,
                None => TypeName::sqlite_empty(),
            };
            Self::Cast(Box::new(Cast {
                argument: Self::sqlite(value.expr())?,
                target,
            }))
        })
    }
}
