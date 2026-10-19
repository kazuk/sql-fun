use crate::{sem::AstAndContextPair, syn::ListOpt};

use super::{AnalysisError, FullName, ParseContext, SemAst};

/// analyzed [`crate::syn::CreateEnumStmt`]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateEnum {
    #[serde(skip)]
    owner: Option<crate::sem::OwnerSpecification>,
    name: FullName,
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
            name: name.clone(),
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
    pub const fn name(&self) -> &FullName {
        &self.name
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

// Ownership changes do not change SQL type identity or the existing serialized type format.
impl PartialEq for CreateEnum {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.values == other.values
    }
}
impl Eq for CreateEnum {}
impl std::hash::Hash for CreateEnum {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.name, state);
        std::hash::Hash::hash(&self.values, state);
    }
}

impl EnumValueDefinition {
    /// Label text, preserving declaration order through `CreateEnum::values`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}
