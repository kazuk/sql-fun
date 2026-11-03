use crate::{
    ast::{Comment as AstComment, TryFromWithTokens},
    sem::AstAndContextPair,
    syn::CommentStmt,
};

use crate::sem::{AnalysisError, ParseContext};

/// analyzed [`crate::syn::CommentStmt`]
#[derive(Debug, Clone)]
pub struct Comment {
    target: String,
    object_type: ObjectKind,
    comment: String,
}

/// PostgreSQL object-kind identifier used by catalog DDL operations.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct ObjectKind(u32);

impl ObjectKind {
    pub(crate) const AGGREGATE: Self = Self(2);
    pub(crate) const COLLATION: Self = Self(8);
    pub(crate) const CONVERSION: Self = Self(9);
    pub(crate) const DATABASE: Self = Self(10);
    pub(crate) const DOMAIN: Self = Self(13);
    pub(crate) const EVENT_TRIGGER: Self = Self(15);
    pub(crate) const FOREIGN_DATA_WRAPPER: Self = Self(17);
    pub(crate) const FOREIGN_SERVER: Self = Self(18);
    pub(crate) const FUNCTION: Self = Self(20);
    pub(crate) const LANGUAGE: Self = Self(22);
    pub(crate) const LARGE_OBJECT: Self = Self(23);
    pub(crate) const MATERIALIZED_VIEW: Self = Self(24);
    pub(crate) const OPERATOR_CLASS: Self = Self(25);
    pub(crate) const OPERATOR: Self = Self(26);
    pub(crate) const OPERATOR_FAMILY: Self = Self(27);
    pub(crate) const PROCEDURE: Self = Self(30);
    pub(crate) const PUBLICATION: Self = Self(31);
    pub(crate) const ROUTINE: Self = Self(35);
    pub(crate) const SCHEMA: Self = Self(37);
    pub(crate) const SEQUENCE: Self = Self(38);
    pub(crate) const SUBSCRIPTION: Self = Self(39);
    pub(crate) const STATISTICS: Self = Self(40);
    pub(crate) const TABLE: Self = Self(42);
    pub(crate) const TABLESPACE: Self = Self(43);
    pub(crate) const TEXT_SEARCH_CONFIGURATION: Self = Self(46);
    pub(crate) const TEXT_SEARCH_DICTIONARY: Self = Self(47);
    pub(crate) const TYPE: Self = Self(50);
    pub(crate) const VIEW: Self = Self(52);

    pub(crate) const fn code(&self) -> u32 {
        self.0
    }
}

impl From<u32> for ObjectKind {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl Comment {
    /// comment target
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// get comment target object type
    #[must_use]
    pub const fn object_type(&self) -> &ObjectKind {
        &self.object_type
    }

    /// get a comment text
    #[must_use]
    pub fn comment(&self) -> &str {
        &self.comment
    }

    /// Create a `COMMENT` statement.
    #[must_use]
    pub fn new(target: &str, object_type: ObjectKind, comment: &str) -> Self {
        Self {
            target: target.to_owned(),
            object_type,
            comment: comment.to_owned(),
        }
    }

    /// analyze [`crate::syn::CommentStmt`]
    pub fn analyze_comment<TParseContext>(
        context: TParseContext,
        syn: CommentStmt,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        AstComment::try_from_with_tokens(&syn, &::sql_fun_core::IVec::default())?.apply(context)
    }
}
