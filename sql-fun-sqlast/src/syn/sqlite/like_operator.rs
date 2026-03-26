use turso_parser::ast::LikeOperator as LikeOperatorIn;

/// like operator
pub enum LikeOperator {
    /// glob
    Glob,
    /// like
    Like,
    /// match
    Match,
    /// regular expression
    Regexp,
}

impl From<LikeOperatorIn> for LikeOperator {
    fn from(value: LikeOperatorIn) -> Self {
        match value {
            LikeOperatorIn::Glob => Self::Glob,
            LikeOperatorIn::Like => Self::Like,
            LikeOperatorIn::Match => Self::Match,
            LikeOperatorIn::Regexp => Self::Regexp,
        }
    }
}
