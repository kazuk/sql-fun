use turso_parser::ast::UnaryOperator as UnaryOperatorIn;

/// unary operator
pub enum UnaryOperator {
    /// bitwise not operator
    BitwiseNot,
    /// negative value
    Negative,
    /// logical not operator
    Not,
    /// positive value
    Positive,
}

impl From<UnaryOperatorIn> for UnaryOperator {
    fn from(value: UnaryOperatorIn) -> Self {
        match value {
            UnaryOperatorIn::BitwiseNot => Self::BitwiseNot,
            UnaryOperatorIn::Negative => Self::Negative,
            UnaryOperatorIn::Not => Self::Not,
            UnaryOperatorIn::Positive => Self::Positive,
        }
    }
}
