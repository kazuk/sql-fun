use crate::sem::convert;
pub use crate::syntax_options::ObjectKind;
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
        convert(&AstComment::try_from_with_tokens(
            &syn,
            &::sql_fun_core::IVec::default(),
        )?)
        .apply(context)
    }
}
