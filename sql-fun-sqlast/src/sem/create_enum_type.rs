use crate::{sem::AstAndContextPair, syn::ListOpt};

use super::{AnalysisError, FullName, ParseContext, SemAst, TypeReference};

/// analyzed [`crate::syn::CreateEnumStmt`]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateEnum {
    #[serde(skip)]
    owner: Option<crate::sem::OwnerSpecification>,
    type_ref: TypeReference,
    values: Vec<EnumValueDefinition>,
}

impl CreateEnum {
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

    pub(crate) fn new(name: &FullName, values: &[String]) -> Self {
        Self {
            owner: None,
            type_ref: TypeReference::from_full_name(name.clone()),
            values: values
                .iter()
                .map(|value| EnumValueDefinition {
                    name: value.clone(),
                })
                .collect(),
        }
    }

    /// get name
    #[must_use]
    pub fn name(&self) -> &FullName {
        self.type_ref
            .full_name()
            .expect("enum identity is constructed from its declared name")
    }

    /// Reference identifying this enum type.
    #[must_use]
    pub(crate) const fn type_ref(&self) -> &TypeReference {
        &self.type_ref
    }

    pub(crate) fn with_name(&self, name: &FullName) -> Self {
        let mut definition = self.clone();
        definition.type_ref = TypeReference::from_full_name(name.clone());
        definition
    }

    /// get values
    #[must_use]
    pub const fn values(&self) -> &Vec<EnumValueDefinition> {
        &self.values
    }

    /// analyze [`crate::syn::CreateEnumStmt`]
    pub fn analyze_create_enum_type<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        create: crate::syn::CreateEnumStmt,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let name = FullName::try_from(create.get_type_name())?;
        let Some(enum_values) = create.get_vals().map(|v| v.as_string()) else {
            AnalysisError::raise_unexpected_none("create_enum_stmt.vals")?
        };
        let values = enum_values
            .iter()
            .map(|value| value.get_sval())
            .collect::<Vec<_>>();
        let enum_def = Self::new(&name, &values);
        let result_context = context.apply_create_enum(&enum_def)?;
        Ok(AstAndContextPair::new(
            SemAst::CreateEnum(enum_def),
            result_context,
        ))
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EnumValueDefinition {
    name: String,
}

// Ownership is not part of SQL type identity or the serialized definition.
impl PartialEq for CreateEnum {
    fn eq(&self, other: &Self) -> bool {
        self.type_ref == other.type_ref && self.values == other.values
    }
}
impl Eq for CreateEnum {}
impl std::hash::Hash for CreateEnum {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.type_ref, state);
        std::hash::Hash::hash(&self.values, state);
    }
}
