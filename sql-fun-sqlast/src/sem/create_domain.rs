use sql_fun_core::IVec;

use crate::{
    ast::TryFromWithTokens,
    sem::{AstAndContextPair, TypeReference},
    syn::ScanToken,
};

use super::{AnalysisError, FullName, ParseContext};

/// analyzed [`crate::syn::CreateDomainStmt`]
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateDomain {
    type_ref: TypeReference,
    base_type: Option<FullName>,
}

impl CreateDomain {
    pub(crate) fn new(type_ref: &TypeReference, base_type: &Option<FullName>) -> Self {
        Self {
            type_ref: type_ref.clone(),
            base_type: base_type.clone(),
        }
    }
    /// get name
    #[must_use]
    pub fn name(&self) -> &FullName {
        self.type_ref.full_name()
    }

    /// get base type reference
    #[must_use]
    pub const fn type_ref(&self) -> &TypeReference {
        &self.type_ref
    }

    /// analyze [`crate::syn::CreateDomainStmt`]
    pub fn analyze_create_domain<TParseContext>(
        context: TParseContext,
        syn: crate::syn::CreateDomainStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        crate::ast::Domain::try_from_with_tokens(&syn, tokens)?.resolve_and_apply(context)
    }
}
