use crate::sem::{
    AnalysisError, CatalogTypeIdentity, FullName, OperatorFamilyIdentity, TypeReference,
};
use std::path::Path;

/// One exact operator overload's B-tree operator-family membership.
/// Multiple families and cross-type family entries remain separate facts.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct BtreeOperatorMembership {
    operator: OperatorIdentity,
    #[serde(deserialize_with = "::serde::Deserialize::deserialize")]
    negator: Option<OperatorIdentity>,
    negated_by: Vec<OperatorIdentity>,
    left: OperandType,
    right: OperandType,
    family: OperatorFamilyIdentity,
    family_left: CatalogTypeIdentity,
    family_right: CatalogTypeIdentity,
    strategy: u16,
    purpose: String,
    #[serde(deserialize_with = "::serde::Deserialize::deserialize")]
    sort_family: Option<OperatorFamilyIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
struct OperatorIdentity {
    schema: String,
    name: String,
}
impl OperatorIdentity {
    fn qualified_name(&self) -> FullName {
        FullName::with_schema(&self.schema, &self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
struct OperandType {
    name: CatalogTypeIdentity,
    array: bool,
}
impl OperandType {
    fn reference(&self) -> TypeReference {
        TypeReference::concrete_type_ref(&self.name.qualified_name(), self.array)
    }
}

impl BtreeOperatorMembership {
    /// Catalog-declared negator, absent when PostgreSQL declares none.
    #[must_use]
    pub fn negator(&self) -> Option<FullName> {
        self.negator.as_ref().map(OperatorIdentity::qualified_name)
    }
    /// Exact qualified operator identity; overload inputs are retained separately.
    #[must_use]
    pub fn operator(&self) -> FullName {
        self.operator.qualified_name()
    }
    /// Operator's declared left input, without substituting family inputs.
    #[must_use]
    pub fn left(&self) -> TypeReference {
        self.left.reference()
    }
    /// Operator's declared right input.
    #[must_use]
    pub fn right(&self) -> TypeReference {
        self.right.reference()
    }
    /// Family identity, including access method.
    #[must_use]
    pub const fn family(&self) -> &OperatorFamilyIdentity {
        &self.family
    }
    /// Family entry's left applicability type.
    #[must_use]
    pub const fn family_left(&self) -> &CatalogTypeIdentity {
        &self.family_left
    }
    /// Family entry's right applicability type.
    #[must_use]
    pub const fn family_right(&self) -> &CatalogTypeIdentity {
        &self.family_right
    }
    /// PostgreSQL B-tree strategy number (1 less, 2 less/equal, 3 equal,
    /// 4 greater/equal, 5 greater). No operator spelling is interpreted.
    #[must_use]
    pub const fn strategy(&self) -> u16 {
        self.strategy
    }
    /// Catalog purpose: `s` search/comparison or `o` ordering.
    #[must_use]
    pub fn purpose(&self) -> &str {
        &self.purpose
    }
    /// Whether this fact is a B-tree search/comparison member.
    #[must_use]
    pub fn is_comparison(&self) -> bool {
        self.family.access_method() == "btree"
            && self.purpose == "s"
            && (1..=5).contains(&self.strategy)
    }
    /// Optional ordering sort-family identity; not silently treated as comparison.
    #[must_use]
    pub const fn sort_family(&self) -> Option<&OperatorFamilyIdentity> {
        self.sort_family.as_ref()
    }
    fn supported_metadata(&self) -> bool {
        self.family.access_method() == "btree"
            && (1..=5).contains(&self.strategy)
            && matches!(self.purpose.as_str(), "s" | "o")
            && self
                .sort_family
                .as_ref()
                .is_none_or(|family| family.access_method() == "btree")
    }
    fn negator_matches(
        &self,
        operator: &FullName,
        left: &TypeReference,
        right: &TypeReference,
    ) -> bool {
        self.negated_by
            .iter()
            .any(|identity| identity.qualified_name() == *operator)
            && self.left() == *left
            && self.right() == *right
    }
    fn matches(&self, operator: &FullName, left: &TypeReference, right: &TypeReference) -> bool {
        self.operator() == *operator && self.left() == *left && self.right() == *right
    }
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct BtreeOperatorMemberships {
    format_version: u32,
    memberships: Vec<BtreeOperatorMembership>,
}
impl BtreeOperatorMemberships {
    pub(crate) fn load(path: &Path) -> Result<Option<Self>, AnalysisError> {
        // Missing legacy payloads are explicitly unavailable, never an empty catalog.
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let payload: Self = serde_json::from_reader(std::io::BufReader::new(file))?;
        if payload.format_version != 1
            || !payload
                .memberships
                .iter()
                .all(BtreeOperatorMembership::supported_metadata)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unsupported B-tree membership metadata: {}", path.display()),
            )
            .into());
        }
        Ok(Some(payload))
    }
    pub(crate) fn negator_lookup(
        &self,
        operator: &FullName,
        left: &TypeReference,
        right: &TypeReference,
    ) -> Vec<&BtreeOperatorMembership> {
        self.memberships
            .iter()
            .filter(|member| member.negator_matches(operator, left, right))
            .collect()
    }
    pub(crate) fn lookup(
        &self,
        operator: &FullName,
        left: &TypeReference,
        right: &TypeReference,
    ) -> Vec<&BtreeOperatorMembership> {
        self.memberships
            .iter()
            .filter(|member| member.matches(operator, left, right))
            .collect()
    }
}

#[cfg(test)]
mod tests;
