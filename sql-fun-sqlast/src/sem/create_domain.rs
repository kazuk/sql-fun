use sql_fun_core::IVec;

use crate::{
    ast::TryFromWithTokens,
    sem::{AstAndContextPair, TypeReference},
    syn::ScanToken,
};

use super::{AnalysisError, FullName, ParseContext};

/// analyzed [`crate::syn::CreateDomainStmt`]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateDomain {
    #[serde(skip)]
    owner: Option<crate::sem::OwnerSpecification>,
    type_ref: TypeReference,
    base_type: Option<FullName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    category: Option<crate::sem::TypeCategory>,
}

impl CreateDomain {
    /// Retained ownership; None means no ownership information was supplied.
    /// Ownership is catalog state: it is omitted from the existing serialized
    /// type format and does not affect SQL type equality or hashing.
    pub const fn owner(&self) -> Option<&crate::sem::OwnerSpecification> {
        self.owner.as_ref()
    }
    /// Commit an ownership change, leaving the rest of the definition intact.
    pub(crate) fn change_owner(&mut self, owner: &crate::sem::OwnerSpecification) {
        self.owner = Some(owner.clone());
    }

    pub(crate) fn new(type_ref: &TypeReference, base_type: &Option<FullName>) -> Self {
        Self {
            owner: None,
            category: None,
            type_ref: type_ref.clone(),
            base_type: base_type.clone(),
        }
    }
    pub(crate) const fn category(&self) -> Option<crate::sem::TypeCategory> {
        self.category
    }

    pub(crate) const fn set_category(&mut self, category: Option<crate::sem::TypeCategory>) {
        self.category = category;
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

    /// Underlying type name retained by the legacy declaration, if available.
    ///
    /// This differs from `type_ref`, which identifies the domain itself.
    #[must_use]
    pub const fn base_type(&self) -> Option<&FullName> {
        self.base_type.as_ref()
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

// Ownership changes do not change SQL type identity or the existing serialized type format.
impl PartialEq for CreateDomain {
    fn eq(&self, other: &Self) -> bool {
        self.type_ref == other.type_ref && self.base_type == other.base_type
    }
}
impl Eq for CreateDomain {}
impl std::hash::Hash for CreateDomain {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.type_ref, state);
        std::hash::Hash::hash(&self.base_type, state);
    }
}
