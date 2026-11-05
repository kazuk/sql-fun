mod ddl;
use crate::{
    ast::{Domain, TryFromWithTokens},
    context::ParseContext,
    names::{FullName, TableName},
    sem::{
        AnalysisError, AstAndContextPair, CatalogNameLookup, DeclarationCollation,
        OwnerSpecification, TypeCategory, TypeReference, WithTypmod,
    },
    syn::{CreateDomainStmt, ScanToken},
};
use sql_fun_core::IVec;

/// analyzed [`crate::syn::CreateDomainStmt`]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateDomain {
    #[serde(skip)]
    owner: Option<OwnerSpecification>,
    type_ref: TypeReference,
    base_type: Option<TypeReference>,
    #[serde(default)]
    declaration_collation: DeclarationCollation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    requested_base: Option<Box<WithTypmod<TypeReference>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    category: Option<TypeCategory>,
}

impl CreateDomain {
    /// Requested declaration type and modifiers, separate from canonical identity.
    pub fn requested_base(&self) -> Option<&WithTypmod<TypeReference>> {
        self.requested_base.as_deref()
    }

    pub(crate) fn retain_requested_type(&mut self, value: &WithTypmod<TypeReference>) {
        self.requested_base = Some(Box::new(value.clone()));
    }

    /// Completed explicit base-type collation validation.
    pub const fn declaration_collation(&self) -> &DeclarationCollation {
        &self.declaration_collation
    }

    pub(crate) fn validate_collation(
        &mut self,
        context: &mut impl ParseContext,
        lookup: Option<&CatalogNameLookup>,
    ) -> Result<(), AnalysisError> {
        self.declaration_collation =
            DeclarationCollation::validate(context, self.base_type.as_ref(), lookup);
        self.declaration_collation
            .report(context, self.base_type.as_ref())
    }

    pub(crate) const fn declarations_can_apply(&self) -> bool {
        self.base_type.is_some() && self.declaration_collation.can_apply()
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        if let Some(base) = &mut self.base_type {
            base.rename_relation(old, new);
        }
    }

    /// Retained ownership; None means no ownership information was supplied.
    /// Ownership is catalog state: it is omitted from the existing serialized
    /// type format and does not affect SQL type equality or hashing.
    pub const fn owner(&self) -> Option<&OwnerSpecification> {
        self.owner.as_ref()
    }
    /// Commit an ownership change, leaving the rest of the definition intact.
    pub(crate) fn change_owner(&mut self, owner: &OwnerSpecification) {
        self.owner = Some(owner.clone());
    }

    pub(crate) fn new(type_ref: &TypeReference, base_type: &Option<TypeReference>) -> Self {
        Self {
            owner: None,
            category: None,
            type_ref: type_ref.clone(),
            base_type: base_type.clone(),
            declaration_collation: DeclarationCollation::default(),
            requested_base: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        names::FullName,
        sem::{TypeReference, create_domain::CreateDomain},
    };

    #[rstest::rstest]
    #[case::unresolved(None)]
    #[case::scalar(Some(TypeReference::from_master_type_name("int4")))]
    #[case::array(Some(TypeReference::from_master_type_name("_int4")))]
    #[case::nested_domain(Some(TypeReference::from_full_name(FullName::with_schema(
        "public", "amounts"
    ))))]
    fn domain_base_survives_serialization(#[case] base: Option<TypeReference>) {
        let reference = TypeReference::from_full_name(FullName::with_schema("public", "amount"));
        let domain = CreateDomain::new(&reference, &base);
        let serialized = ::serde_json::to_value(&domain).unwrap();
        assert_eq!(
            serialized["base_type"],
            ::serde_json::to_value(&base).unwrap()
        );
        let restored: CreateDomain = ::serde_json::from_value(serialized).unwrap();
        assert_eq!(restored.base_type(), base.as_ref());
        assert_eq!(restored.type_ref(), &reference);
        assert_eq!(restored, domain);
    }
}

impl CreateDomain {
    pub(crate) const fn category(&self) -> Option<TypeCategory> {
        self.category
    }

    pub(crate) const fn set_category(&mut self, category: Option<TypeCategory>) {
        self.category = category;
    }

    /// get name
    #[must_use]
    pub fn name(&self) -> &FullName {
        self.type_ref
            .full_name()
            .expect("domain identity is constructed from its declared name")
    }

    /// Resolved base type, including array qualification, when lookup succeeded.
    pub(crate) const fn base_type(&self) -> Option<&TypeReference> {
        self.base_type.as_ref()
    }

    /// Reference identifying this domain, distinct from its base type.
    #[must_use]
    pub const fn type_ref(&self) -> &TypeReference {
        &self.type_ref
    }

    /// analyze [`crate::syn::CreateDomainStmt`]
    pub fn analyze_create_domain<TParseContext>(
        context: TParseContext,
        syn: CreateDomainStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::from_ast_and_apply(&Domain::try_from_with_tokens(&syn, tokens)?, context)
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

impl CreateDomain {
    pub(crate) fn row_table_name(&self, context: &impl ParseContext) -> Option<TableName> {
        self.base_type()?
            .definition_from_catalog(context)?
            .row_table_name(context)
    }
}
