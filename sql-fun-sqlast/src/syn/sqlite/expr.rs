mod array;
mod between;
mod binary;
mod case;
mod cast;
mod collate;
mod column;
mod field_access;
mod function_call;
mod in_expr;
mod like;
mod null_test;
mod paren;
mod qualified;
mod raise;
mod subquery;
mod unary;
mod variable;

use turso_parser::ast::Expr as ExprIn;

use crate::syn::sqlite::{Literal, Name, Select};

pub use self::{
    array::{Array, Subscript},
    between::ExprBetween,
    binary::{ExprBinary, Operator},
    case::{CaseWhenThenPair, ExprCase},
    cast::ExprCast,
    collate::ExprCollate,
    column::{ExprColumn, ExprRowId},
    field_access::{FieldAccess, FieldAccessResolution},
    function_call::{ExprFunctionCall, ExprFunctionCallStar},
    in_expr::{ExprInList, ExprInSelect, ExprInTable},
    like::{ExprLike, LikeOperator},
    null_test::{ExprIsNull, ExprNotNull},
    paren::Parenthesized,
    qualified::{ExprDoublyQualified, Qualified},
    raise::ExprRaise,
    subquery::SubqueryResult,
    unary::{ExprUnary, UnaryOperator},
    variable::Variable,
};

/// expression
#[derive(Clone, Debug)]
pub enum Expr {
    /// between
    Between(ExprBetween),
    /// binary
    Binary(ExprBinary),
    /// register
    Register(usize),
    /// case
    Case(ExprCase),
    /// cast
    Cast(ExprCast),
    /// collate
    Collate(ExprCollate),
    /// doubly qualified
    DoublyQualified(ExprDoublyQualified),
    /// exists
    Exists(Select),
    /// function call
    FunctionCall(ExprFunctionCall),
    /// function call star
    FunctionCallStar(ExprFunctionCallStar),
    /// id
    Id(Name),
    /// column
    Column(ExprColumn),
    /// row id
    RowId(ExprRowId),
    /// in list
    InList(ExprInList),
    /// in select
    InSelect(ExprInSelect),
    /// in table
    InTable(ExprInTable),
    /// is null
    IsNull(ExprIsNull),
    /// like
    Like(ExprLike),
    /// literal
    Literal(Literal),
    /// name
    Name(Name),
    /// not null
    NotNull(ExprNotNull),
    /// parenthesized
    Parenthesized(Parenthesized),
    /// qualified
    Qualified(Qualified),
    /// raise
    Raise(ExprRaise),
    /// subquery
    Subquery(Select),
    /// unary
    Unary(ExprUnary),
    /// variable
    Variable(Variable),
    /// subquery result
    SubqueryResult(SubqueryResult),
    /// field access
    FieldAccess(FieldAccess),
    /// default
    Default,
    /// array
    Array(Array),
    /// subscript
    Subscript(Subscript),
}

impl Expr {
    /// create Register variant
    pub const fn register(size: usize) -> Self {
        Self::Register(size)
    }

    /// create Exists variant
    pub fn exists<SELECT>(select: SELECT) -> Self
    where
        SELECT: Into<Select>,
    {
        Self::Exists(select.into())
    }

    /// create Id variant
    pub fn id<NAME>(name: NAME) -> Self
    where
        NAME: Into<Name>,
    {
        Self::Id(name.into())
    }

    /// create Name variant
    pub fn name<NAME>(name: NAME) -> Self
    where
        NAME: Into<Name>,
    {
        Self::Name(name.into())
    }

    /// create Literal variant
    pub fn literal<Lit>(literal: Lit) -> Self
    where
        Lit: Into<Literal>,
    {
        Self::Literal(literal.into())
    }
}

impl From<ExprIn> for Expr {
    fn from(value: ExprIn) -> Self {
        match value {
            ExprIn::Between {
                lhs,
                not,
                start,
                end,
            } => Self::between(lhs, not, start, end),
            ExprIn::Binary(left, operator, right) => Self::binary(left, operator, right),
            ExprIn::Register(size) => Self::register(size),
            ExprIn::Case {
                base,
                when_then_pairs,
                else_expr,
            } => Self::case(base, when_then_pairs, else_expr),
            ExprIn::Cast { expr, type_name } => Self::cast(expr, type_name),
            ExprIn::Collate(expr, name) => Self::collate(expr, name),
            ExprIn::DoublyQualified(schema, table, column) => {
                Self::doubly_qualified(schema, table, column)
            }
            ExprIn::Exists(select) => Self::exists(select),
            ExprIn::FunctionCall {
                name,
                distinctness,
                args,
                order_by,
                within_group,
                filter_over,
            } => Self::function_call(
                name,
                distinctness,
                args,
                order_by,
                within_group,
                filter_over,
            ),
            ExprIn::FunctionCallStar { name, filter_over } => {
                Self::function_call_star(name, filter_over)
            }
            ExprIn::Id(name) => Self::id(name),
            ExprIn::Column {
                database,
                table,
                column,
                is_rowid_alias,
            } => Self::column(database, table, column, is_rowid_alias),
            ExprIn::RowId { database, table } => Self::row_id(database, table),
            ExprIn::InList { lhs, not, rhs } => Self::in_list(lhs, not, rhs),
            ExprIn::InSelect { lhs, not, rhs } => Self::in_select(lhs, not, rhs),
            ExprIn::InTable {
                lhs,
                not,
                rhs,
                args,
            } => Self::in_table(lhs, not, rhs, args),
            ExprIn::IsNull(expr) => Self::is_null(expr),
            ExprIn::NotNull(expr) => Self::not_null(expr),
            ExprIn::Like {
                lhs,
                not,
                op,
                rhs,
                escape,
            } => Self::like(lhs, not, op, rhs, escape),
            ExprIn::Literal(literal) => Self::literal(literal),
            ExprIn::Name(name) => Self::name(name),
            ExprIn::Parenthesized(exprs) => Self::parenthesized(exprs),
            ExprIn::Qualified(table, column) => Self::qualified(table, column),
            ExprIn::Raise(resolve_type, expr) => Self::raise(resolve_type, expr),
            ExprIn::Subquery(select) => Self::subquery(select),
            ExprIn::Unary(unary_operator, expr) => Self::unary(unary_operator, expr),
            ExprIn::Variable(v) => Self::variable(v),
            ExprIn::SubqueryResult {
                subquery_id,
                lhs,
                not_in,
                query_type,
            } => Self::subquery_result(subquery_id, lhs, not_in, query_type),
            ExprIn::FieldAccess {
                base,
                field,
                resolved,
            } => Self::field_access(base, field, resolved),
            ExprIn::Default => Self::Default,
            ExprIn::Array { elements } => Self::array(elements),
            ExprIn::Subscript { base, index } => Self::subscript(base, index),
        }
    }
}

impl From<Box<ExprIn>> for Expr {
    fn from(value: Box<ExprIn>) -> Self {
        Self::from(*value)
    }
}
