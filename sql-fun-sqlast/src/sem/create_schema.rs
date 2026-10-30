use sql_fun_core::IVec;

use crate::{ast::TryFromWithTokens, sem::AstAndContextPair, syn::ScanToken};

use super::{AnalysisError, ParseContext, SemAst};

/// analyzed [`crate::syn::CreateSchemaStmt`]
#[derive(Debug, Clone)]
pub struct CreateSchema {
    if_not_exists: bool,

    owner: Option<crate::sem::OwnerSpecification>,
    name: String,
    #[expect(dead_code)]
    elements: Vec<SemAst>,
}

impl CreateSchema {
    pub(crate) const fn if_not_exists(&self) -> bool {
        self.if_not_exists
    }
    pub(crate) const fn with_if_not_exists(mut self, value: bool) -> Self {
        self.if_not_exists = value;
        self
    }

    /// Retained ownership; None means no ownership information was supplied.
    pub const fn owner(&self) -> Option<&crate::sem::OwnerSpecification> {
        self.owner.as_ref()
    }
    /// Commit an ownership change, leaving the rest of the definition intact.
    pub(crate) fn change_owner(&mut self, owner: &crate::sem::OwnerSpecification) {
        self.owner = Some(owner.clone());
    }

    pub(crate) fn new(name: &str, elements: &[SemAst]) -> Self {
        Self {
            if_not_exists: false,
            owner: None,
            name: name.to_owned(),
            elements: elements.to_vec(),
        }
    }
    /// new instance
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// analyze [`crate::syn::CreateSchemaStmt`]
    pub fn analyze_create_schema<TParseContext>(
        context: TParseContext,
        syn: crate::syn::CreateSchemaStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        crate::ast::Schema::try_from_with_tokens(&syn, tokens)?
            .resolve_and_apply(context, &mut crate::CacheContext::new())
    }
}
