use crate::{
    ast::EnumType as AstEnumType,
    names::FullName,
    sem::{
        AnalysisError, AstAndContextPair, OwnerSpecification, ParseContext, SemAst, TypeReference,
    },
};

/// Analyzed CREATE TYPE AS ENUM statement.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateEnum {
    #[serde(skip)]
    owner: Option<OwnerSpecification>,
    type_ref: TypeReference,
    values: Vec<EnumValueDefinition>,
}

impl CreateEnum {
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

// Analysis owned by the semantic definitions in this module.

impl CreateEnum {
    pub(crate) fn from_ast_and_apply<TContext>(
        ast: &AstEnumType,
        context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let enum_type = Self::new(&FullName::from_ast_name(ast.name())?, ast.values());
        let context = context.apply_create_enum(&enum_type)?;
        Ok(AstAndContextPair::new(
            SemAst::from_create_enum(&enum_type),
            context,
        ))
    }
}
