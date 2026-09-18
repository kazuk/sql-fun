use itertools::Itertools;

use crate::{
    sem::AstAndContextPair,
    syn::{ListOpt, ObjectType, Opt},
};

use super::{AnalysisError, ParseContext, SemAst};

/// analyzed [`crate::syn::CommentStmt`]
#[derive(Debug, Clone)]
pub struct Comment {
    target: String,
    object_type: ObjectType,
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
    pub const fn object_type(&self) -> &ObjectType {
        &self.object_type
    }

    /// get a comment text
    #[must_use]
    pub fn comment(&self) -> &str {
        &self.comment
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

        let comment = syn.get_comment();
        let comment = Self {
            target: items,
            object_type,
            comment,
        };
        context = context.apply_comment(&comment)?;
        Ok(AstAndContextPair::new(SemAst::Comment(comment), context))
    }
}

/// analyze [`crate::syn::CommentStmt`]
#[deprecated = "use Comment::analyze_comment"]
pub fn analyze_comment<TParseContext>(
    context: TParseContext,
    syn: crate::syn::CommentStmt,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    Comment::analyze_comment(context, syn)
}
