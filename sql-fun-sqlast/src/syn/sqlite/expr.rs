use turso_parser::ast::Expr as ExprIn;

use crate::syn::sqlite::{
    As, Distinctness, FunctionTail, LikeOperator, Literal, Name, QualifiedName, ResolveType,
    Select, SortedColumn, SubqueryType, TableInternalId, Type, UnaryOperator,
};

/// between expression
#[derive(derive_getters::Getters)]
pub struct ExprBetween {
    lhs: Box<Expr>,
    not: bool,
    start: Box<Expr>,
    end: Box<Expr>,
}

impl ExprBetween {
    /// create instance
    pub fn new(lhs: Expr, not: bool, start: Expr, end: Expr) -> Self {
        Self {
            lhs: Box::new(lhs),
            not,
            start: Box::new(start),
            end: Box::new(end),
        }
    }
}

/// binary expression
#[derive(derive_getters::Getters)]
pub struct ExprBinary {
    left: Box<Expr>,
    operator: Operator,
    right: Box<Expr>,
}

impl ExprBinary {
    /// create instance
    pub fn new(left: Expr, operator: Operator, right: Expr) -> Self {
        Self {
            left: Box::new(left),
            operator,
            right: Box::new(right),
        }
    }
}

/// case when
pub struct CaseWhenThenPair(Box<Expr>, Box<Expr>);

impl From<(Box<ExprIn>, Box<ExprIn>)> for CaseWhenThenPair {
    fn from(value: (Box<ExprIn>, Box<ExprIn>)) -> Self {
        Self::new(Expr::from(*value.0), Expr::from(*value.1))
    }
}

impl CaseWhenThenPair {
    /// create instance
    pub fn new(when: Expr, then: Expr) -> Self {
        Self(Box::new(when), Box::new(then))
    }

    /// when expression
    pub fn when(&self) -> &Expr {
        self.0.as_ref()
    }

    /// then expression
    pub fn then(&self) -> &Expr {
        self.1.as_ref()
    }
}

/// case expression
#[derive(derive_getters::Getters)]
pub struct ExprCase {
    base: Option<Box<Expr>>,
    when_then_pairs: Vec<CaseWhenThenPair>,
    else_expr: Option<Box<Expr>>,
}

impl ExprCase {
    /// create instance
    pub fn new(
        base: Option<Expr>,
        when_then_pairs: Vec<(Box<ExprIn>, Box<ExprIn>)>,
        else_expr: Option<Expr>,
    ) -> Self {
        Self {
            base: base.map(Box::new),
            when_then_pairs: when_then_pairs
                .into_iter()
                .map(CaseWhenThenPair::from)
                .collect(),
            else_expr: else_expr.map(Box::new),
        }
    }
}

/// cast expression
#[derive(derive_getters::Getters)]
pub struct ExprCast {
    expr: Box<Expr>,
    type_name: Option<Type>,
}

impl ExprCast {
    /// create instance
    pub fn new(expr: Expr, type_name: Option<Type>) -> Self {
        Self {
            expr: Box::new(expr),
            type_name,
        }
    }
}

/// collate expression
#[derive(derive_getters::Getters)]
pub struct ExprCollate {
    expr: Box<Expr>,
    name: Name,
}

impl ExprCollate {
    /// create instance
    pub fn new(expr: Expr, name: Name) -> Self {
        Self {
            expr: Box::new(expr),
            name,
        }
    }
}

/// doubly qualified expression
#[derive(derive_getters::Getters)]
pub struct ExprDoublyQualified {
    schema: Name,
    table: Name,
    column: Name,
}

impl ExprDoublyQualified {
    /// create instance
    pub fn new(schema: Name, table: Name, column: Name) -> Self {
        Self {
            schema,
            table,
            column,
        }
    }
}

/// function call expression
#[derive(derive_getters::Getters)]
pub struct ExprFunctionCall {
    name: Name,
    distinctness: Option<Distinctness>,
    args: Vec<Box<Expr>>,
    order_by: Vec<SortedColumn>,
    filter_over: FunctionTail,
}

impl ExprFunctionCall {
    /// create instance
    pub fn new(
        name: Name,
        distinctness: Option<Distinctness>,
        args: Vec<Expr>,
        order_by: Vec<SortedColumn>,
        filter_over: FunctionTail,
    ) -> Self {
        Self {
            name,
            distinctness,
            args: args.into_iter().map(Box::new).collect(),
            order_by,
            filter_over,
        }
    }
}

/// function call star
#[derive(derive_getters::Getters)]
pub struct ExprFunctionCallStar {
    name: Name,
    filter_over: FunctionTail,
}

impl ExprFunctionCallStar {
    /// create instance
    pub fn new(name: Name, filter_over: FunctionTail) -> Self {
        Self { name, filter_over }
    }
}

/// column expression
#[derive(derive_getters::Getters)]
pub struct ExprColumn {
    database: Option<usize>,
    table: TableInternalId,
    column: usize,
    is_rowid_alias: bool,
}

impl ExprColumn {
    /// create instance
    pub fn new(
        database: Option<usize>,
        table: TableInternalId,
        column: usize,
        is_rowid_alias: bool,
    ) -> Self {
        Self {
            database,
            table,
            column,
            is_rowid_alias,
        }
    }
}

/// row-id expression
#[derive(derive_getters::Getters)]
pub struct ExprRowId {
    database: Option<usize>,
    table: TableInternalId,
}

impl ExprRowId {
    /// create instance
    pub fn new(database: Option<usize>, table: TableInternalId) -> Self {
        Self { database, table }
    }
}

/// in list expression
#[derive(derive_getters::Getters)]
pub struct ExprInList {
    lhs: Box<Expr>,
    not: bool,
    rhs: Vec<Box<Expr>>,
}

impl ExprInList {
    /// create instance
    pub fn new(lhs: Expr, not: bool, rhs: Vec<Expr>) -> Self {
        Self {
            lhs: Box::new(lhs),
            not,
            rhs: rhs.into_iter().map(Box::new).collect(),
        }
    }
}

/// in select expression
#[derive(derive_getters::Getters)]
pub struct ExprInSelect {
    lhs: Box<Expr>,
    not: bool,
    rhs: Select,
}

impl ExprInSelect {
    /// create instance
    pub fn new(lhs: Expr, not: bool, rhs: Select) -> Self {
        Self {
            lhs: Box::new(lhs),
            not,
            rhs,
        }
    }
}

/// in table expression
#[derive(derive_getters::Getters)]
pub struct ExprInTable {
    lhs: Box<Expr>,
    not: bool,
    rhs: QualifiedName,
    args: Vec<Box<Expr>>,
}

impl ExprInTable {
    /// create instance
    pub fn new(lhs: Expr, not: bool, rhs: QualifiedName, args: Vec<Expr>) -> Self {
        Self {
            lhs: Box::new(lhs),
            not,
            rhs,
            args: args.into_iter().map(Box::new).collect(),
        }
    }
}

/// is null expression
pub struct ExprIsNull(Box<Expr>);

impl ExprIsNull {
    /// create instance
    pub fn new(expr: Expr) -> Self {
        Self(Box::new(expr))
    }

    /// expression
    pub fn expr(&self) -> &Expr {
        self.0.as_ref()
    }
}

/// like expression
#[derive(derive_getters::Getters)]
pub struct ExprLike {
    lhs: Box<Expr>,
    not: bool,
    op: LikeOperator,
    rhs: Box<Expr>,
    escape: Option<Box<Expr>>,
}

impl ExprLike {
    /// create instance
    pub fn new(lhs: Expr, not: bool, op: LikeOperator, rhs: Expr, escape: Option<Expr>) -> Self {
        Self {
            lhs: Box::new(lhs),
            not,
            op,
            rhs: Box::new(rhs),
            escape: escape.map(Box::new),
        }
    }
}

/// not null expression
pub struct ExprNotNull(Box<Expr>);

impl ExprNotNull {
    /// create instance
    pub fn new(expr: Expr) -> Self {
        Self(Box::new(expr))
    }

    /// expression
    pub fn expr(&self) -> &Expr {
        self.0.as_ref()
    }
}

/// parenthesized expression
pub struct Parenthesized(Vec<Box<Expr>>);

impl Parenthesized {
    /// create instance
    pub fn new(items: Vec<Expr>) -> Self {
        Self(items.into_iter().map(Box::new).collect())
    }

    /// expression
    pub fn expr(&self) -> &[Box<Expr>] {
        self.0.as_ref()
    }
}

/// qualified expression
#[derive(derive_getters::Getters)]
pub struct Qualified {
    table: Name,
    column: Name,
}

impl Qualified {
    /// create instance
    pub fn new(table: Name, column: Name) -> Self {
        Self { table, column }
    }
}

/// raise expression
#[derive(derive_getters::Getters)]
pub struct ExprRaise {
    resolve_type: ResolveType,
    expr: Option<Box<Expr>>,
}

impl ExprRaise {
    /// create instance
    pub fn new(resolve_type: ResolveType, expr: Option<Expr>) -> Self {
        Self {
            resolve_type,
            expr: expr.map(Box::new),
        }
    }
}

/// unary expression
#[derive(derive_getters::Getters)]
pub struct ExprUnary {
    unary_operator: UnaryOperator,
    expr: Box<Expr>,
}

impl ExprUnary {
    /// create instance
    pub fn new(unary_operator: UnaryOperator, expr: Expr) -> Self {
        Self {
            unary_operator,
            expr: Box::new(expr),
        }
    }
}

/// variable expression
pub struct Variable(String);

impl Variable {
    /// create instance
    pub fn new(name: String) -> Self {
        Self(name)
    }

    /// variable name
    pub fn name(&self) -> &str {
        self.0.as_str()
    }
}

/// sub query result expression
#[derive(derive_getters::Getters)]
pub struct SubqueryResult {
    subquery_id: TableInternalId,
    lhs: Option<Box<Expr>>,
    not_in: bool,
    query_type: SubqueryType,
}

impl SubqueryResult {
    /// create instance
    pub fn new(
        subquery_id: TableInternalId,
        lhs: Option<Expr>,
        not_in: bool,
        query_type: SubqueryType,
    ) -> Self {
        Self {
            subquery_id,
            lhs: lhs.map(Box::new),
            not_in,
            query_type,
        }
    }
}

/// result column expression
#[derive(derive_getters::Getters)]
pub struct ResultColumnExpr {
    expr: Box<Expr>,
    as_: Option<As>,
}

impl ResultColumnExpr {
    /// create instance
    pub fn new(expr: Expr, as_: Option<As>) -> Self {
        Self {
            expr: Box::new(expr),
            as_,
        }
    }
}

/// expression
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
}

impl From<ExprIn> for Expr {
    fn from(value: ExprIn) -> Self {
        match value {
            ExprIn::Between {
                lhs,
                not,
                start,
                end,
            } => Self::Between(ExprBetween::new(
                Expr::from(*lhs),
                not,
                Expr::from(*start),
                Expr::from(*end),
            )),
            ExprIn::Binary(expr, operator, expr1) => Self::Binary(ExprBinary::new(
                Expr::from(*expr),
                Operator::from(operator),
                Expr::from(*expr1),
            )),
            ExprIn::Register(siz) => Self::Register(siz),
            ExprIn::Case {
                base,
                when_then_pairs,
                else_expr,
            } => Self::Case(ExprCase::new(
                base.map(|e| Expr::from(*e)),
                when_then_pairs,
                else_expr.map(|e| Expr::from(*e)),
            )),
            ExprIn::Cast { expr, type_name } => {
                Self::Cast(ExprCast::new(Expr::from(*expr), type_name.map(Type::from)))
            }
            ExprIn::Collate(expr, name) => {
                Self::Collate(ExprCollate::new(Expr::from(*expr), Name::from(name)))
            }
            ExprIn::DoublyQualified(schema, table, column) => Self::DoublyQualified(
                ExprDoublyQualified::new(Name::from(schema), Name::from(table), Name::from(column)),
            ),
            ExprIn::Exists(select) => Self::Exists(Select::from(select)),
            ExprIn::FunctionCall {
                name,
                distinctness,
                args,
                order_by,
                filter_over,
            } => Self::FunctionCall(ExprFunctionCall::new(
                Name::from(name),
                distinctness.map(Distinctness::from),
                args.into_iter().map(|v| Expr::from(*v)).collect(),
                order_by.into_iter().map(SortedColumn::from).collect(),
                FunctionTail::from(filter_over),
            )),
            ExprIn::FunctionCallStar { name, filter_over } => Self::FunctionCallStar(
                ExprFunctionCallStar::new(Name::from(name), FunctionTail::from(filter_over)),
            ),
            ExprIn::Id(name) => Self::Id(Name::from(name)),
            ExprIn::Column {
                database,
                table,
                column,
                is_rowid_alias,
            } => Self::Column(ExprColumn::new(
                database,
                TableInternalId::from(table),
                column,
                is_rowid_alias,
            )),
            ExprIn::RowId { database, table } => {
                Self::RowId(ExprRowId::new(database, TableInternalId::from(table)))
            }
            ExprIn::InList { lhs, not, rhs } => Self::InList(ExprInList::new(
                Expr::from(*lhs),
                not,
                rhs.into_iter().map(|v| Expr::from(*v)).collect(),
            )),
            ExprIn::InSelect { lhs, not, rhs } => {
                Self::InSelect(ExprInSelect::new(Expr::from(*lhs), not, Select::from(rhs)))
            }
            ExprIn::InTable {
                lhs,
                not,
                rhs,
                args,
            } => Self::InTable(ExprInTable::new(
                Expr::from(*lhs),
                not,
                QualifiedName::from(rhs),
                args.into_iter().map(|v| Expr::from(*v)).collect(),
            )),
            ExprIn::IsNull(expr) => Self::IsNull(ExprIsNull::new(Expr::from(*expr))),
            ExprIn::Like {
                lhs,
                not,
                op,
                rhs,
                escape,
            } => Self::Like(ExprLike::new(
                Expr::from(*lhs),
                not,
                LikeOperator::from(op),
                Expr::from(*rhs),
                escape.map(|v| Expr::from(*v)),
            )),
            ExprIn::Literal(literal) => Self::Literal(Literal::from(literal)),
            ExprIn::Name(name) => Self::Name(Name::from(name)),
            ExprIn::NotNull(expr) => Self::NotNull(ExprNotNull::new(Expr::from(*expr))),
            ExprIn::Parenthesized(exprs) => Self::Parenthesized(Parenthesized::new(
                exprs.into_iter().map(|v| Expr::from(*v)).collect(),
            )),
            ExprIn::Qualified(table, column) => {
                Self::Qualified(Qualified::new(Name::from(table), Name::from(column)))
            }
            ExprIn::Raise(resolve_type, expr) => Self::Raise(ExprRaise::new(
                ResolveType::from(resolve_type),
                expr.map(|v| Expr::from(*v)),
            )),
            ExprIn::Subquery(select) => Self::Subquery(Select::from(select)),
            ExprIn::Unary(unary_operator, expr) => Self::Unary(ExprUnary::new(
                UnaryOperator::from(unary_operator),
                Expr::from(*expr),
            )),
            ExprIn::Variable(v) => Self::Variable(Variable::new(v)),
            ExprIn::SubqueryResult {
                subquery_id,
                lhs,
                not_in,
                query_type,
            } => Self::SubqueryResult(SubqueryResult::new(
                TableInternalId::from(subquery_id),
                lhs.map(|e| Expr::from(*e)),
                not_in,
                SubqueryType::from(query_type),
            )),
        }
    }
}

use turso_parser::ast::Operator as OperatorIn;

/// operator
pub enum Operator {
    /// add
    Add,
    /// logical and
    And,
    /// arrow right
    ArrowRight,
    /// arrow right shift
    ArrowRightShift,
    /// bitwise and
    BitwiseAnd,
    /// bitwise or
    BitwiseOr,
    /// bitwise not
    BitwiseNot,
    /// concat
    Concat,
    /// equals
    Equals,
    /// divide
    Divide,
    /// grater
    Grater,
    /// grater equals
    GraterEquals,
    /// is
    Is,
    /// is not
    IsNot,
    /// left shift
    LeftShift,
    /// less
    Less,
    /// less equals
    LessEqales,
    /// modulus
    Modulus,
    /// multiply
    Multiply,
    /// not equals
    NotEquals,
    /// logical or
    Or,
    /// right shift
    RightShift,
    /// subtract
    Subtract,
}

impl From<OperatorIn> for Operator {
    fn from(value: OperatorIn) -> Self {
        match value {
            OperatorIn::Add => Self::Add,
            OperatorIn::And => Self::And,
            OperatorIn::ArrowRight => Self::ArrowRight,
            OperatorIn::ArrowRightShift => Self::ArrowRightShift,
            OperatorIn::BitwiseAnd => Self::BitwiseAnd,
            OperatorIn::BitwiseOr => Self::BitwiseOr,
            OperatorIn::BitwiseNot => Self::BitwiseNot,
            OperatorIn::Concat => Self::Concat,
            OperatorIn::Equals => Self::Equals,
            OperatorIn::Divide => Self::Divide,
            OperatorIn::Greater => Self::Grater,
            OperatorIn::GreaterEquals => Self::GraterEquals,
            OperatorIn::Is => Self::Is,
            OperatorIn::IsNot => Self::IsNot,
            OperatorIn::LeftShift => Self::LeftShift,
            OperatorIn::Less => Self::Less,
            OperatorIn::LessEquals => Self::LessEqales,
            OperatorIn::Modulus => Self::Modulus,
            OperatorIn::Multiply => Self::Multiply,
            OperatorIn::NotEquals => Self::NotEquals,
            OperatorIn::Or => Self::Or,
            OperatorIn::RightShift => Self::RightShift,
            OperatorIn::Subtract => Self::Subtract,
        }
    }
}
