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
