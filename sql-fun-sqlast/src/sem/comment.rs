use itertools::Itertools;

use crate::{
    sem::AstAndContextPair,
    syn::{ListOpt, Opt},
};

use super::{AnalysisError, ParseContext, SemAst};

/// analyzed [`crate::syn::CommentStmt`]
#[derive(Debug, Clone)]
pub struct Comment {
    target: String,
    object_type: ObjectKind,
    comment: String,
}

/// PostgreSQL object-kind identifier used by `COMMENT`.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct ObjectKind(u32);

impl ObjectKind {
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
        mut context: TParseContext,
        syn: crate::syn::CommentStmt,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let items = if let Some(list) = syn.get_object().as_list().as_inner() {
            let Some(items) = list.get_items().map(|v| v.as_string()) else {
                AnalysisError::raise_unexpected_none("comment_stmt.object.items")?
            };
            items.iter().map(crate::syn::String::get_sval).join(".")
        } else {
            let Some(sval) = syn.get_object().as_string().get_sval() else {
                AnalysisError::raise_unexpected_none("comment_stmt.object")?
            };
            sval
        };

        let Some(object_type) = syn.get_objtype().as_inner() else {
            AnalysisError::raise_unexpected_none("comment_stmt.objtype")?
        };

        let comment = Self::new(
            &items,
            ObjectKind::from(object_type as u32),
            &syn.get_comment(),
        );
        context = context.apply_comment(&comment)?;
        Ok(AstAndContextPair::new(SemAst::Comment(comment), context))
    }
}
