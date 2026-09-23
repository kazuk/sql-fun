use super::Expr;

/// binary expression
#[derive(Clone, Debug, derive_getters::Getters)]
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

impl Expr {
    /// create binary variant
    pub fn binary<LEFT, OP, RIGHT>(left: LEFT, op: OP, right: RIGHT) -> Self
    where
        LEFT: Into<Self>,
        OP: Into<Operator>,
        RIGHT: Into<Self>,
    {
        Self::Binary(ExprBinary::new(left.into(), op.into(), right.into()))
    }
}

use turso_parser::ast::Operator as OperatorIn;

/// operator
#[derive(Clone, Debug)]
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
    /// array contains
    ArrayContains,
    /// array overlap
    ArrayOverlap,
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
            OperatorIn::ArrayContains => Self::ArrayContains,
            OperatorIn::ArrayOverlap => Self::ArrayOverlap,
        }
    }
}
