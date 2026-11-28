use crate::{
    TrieMap,
    context::base_context::deserialize_from_path,
    names::{FullName, OperatorName},
    sem::{
        AnalysisError, BinaryOperatorDefinition, OperatorDefinition, OperatorIdentity,
        OperatorLinks, TypeReference, UnaryOperatorDefinition,
    },
};
use std::{collections::HashMap, path::Path};

#[derive(serde::Deserialize)]
struct BinaryOperatorTypeDecl {
    left: String,
    right: String,
    result: String,
    strict: bool,
    settings: Option<OperatorSettingsDecl>,
}

#[derive(serde::Deserialize)]
struct UnaryOperatorTypeDecl {
    input: String,
    result: String,
    strict: bool,
    settings: Option<OperatorSettingsDecl>,
}

#[derive(serde::Deserialize)]
struct OperatorSettingsDecl {
    restriction: Option<String>,
    join: Option<String>,
    join_arity: Option<usize>,
    #[serde(default, deserialize_with = "present_commutator")]
    commutator: Option<Option<CommutatorName>>,
    negator: Option<String>,
    hashes: bool,
    merges: bool,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum CommutatorName {
    Qualified(String, String),
    Builtin(String),
}

impl CommutatorName {
    fn operator_name(&self) -> OperatorName {
        match self {
            Self::Qualified(schema, name) => OperatorName::from_parts(Some(schema), name),
            Self::Builtin(name) => OperatorName::from_parts(Some("pg_catalog"), name),
        }
    }
}

fn present_commutator<'de, D>(deserializer: D) -> Result<Option<Option<CommutatorName>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    <Option<CommutatorName> as serde::Deserialize>::deserialize(deserializer).map(Some)
}

impl OperatorSettingsDecl {
    fn resolve(&self, left: Option<&TypeReference>, right: &TypeReference) -> OperatorLinks {
        let function = |name: &str| FullName::with_schema("pg_catalog", name);
        let commutator = self.commutator.as_ref().map(|commutator| {
            commutator.as_ref().zip(left).map(|(name, left)| {
                OperatorIdentity::from_signature(
                    name.operator_name(),
                    Some(right.clone()),
                    left.clone(),
                )
            })
        });
        let negator = self
            .negator
            .as_deref()
            .map(|name| OperatorIdentity::builtin_signature(name, left.cloned(), right.clone()));
        OperatorLinks::new(
            self.restriction.as_deref().map(|name| (function(name), 4)),
            self.join
                .as_deref()
                .zip(self.join_arity)
                .map(|(name, arity)| (function(name), arity)),
            commutator,
            negator,
            self.hashes,
            self.merges,
        )
    }
}

/// Loads builtin operator definitions from the selected master files.
pub struct PgBuiltinOperators {}

impl PgBuiltinOperators {
    pub(super) fn load_binary<P>(path: P) -> Result<TrieMap<OperatorDefinition>, AnalysisError>
    where
        P: AsRef<Path>,
    {
        let master_definitions: HashMap<String, Vec<BinaryOperatorTypeDecl>> =
            deserialize_from_path(path)?;

        let mut master = TrieMap::new();
        for (k, v) in &master_definitions {
            let mut overloads = Vec::new();
            for overload in v {
                let left = TypeReference::from_master_type_name(&overload.left);
                let right = TypeReference::from_master_type_name(&overload.right);
                let result = TypeReference::from_master_type_name(&overload.result);
                let strict = overload.strict;
                overloads.push(
                    BinaryOperatorDefinition::new(&left, &right, &result, strict).with_settings(
                        overload
                            .settings
                            .as_ref()
                            .map(|settings| settings.resolve(Some(&left), &right)),
                    ),
                );
            }
            master.insert(
                &OperatorName::from_parts(Some("pg_catalog"), k).to_name_string(),
                OperatorDefinition::binary(&overloads),
            );
        }
        assert!(!master.is_empty());
        Ok(master)
    }

    pub(super) fn load_left_unary<P>(path: P) -> Result<TrieMap<OperatorDefinition>, AnalysisError>
    where
        P: AsRef<Path>,
    {
        let master_definitions: HashMap<String, Vec<UnaryOperatorTypeDecl>> =
            deserialize_from_path(path)?;
        let mut master = TrieMap::new();
        for (k, v) in &master_definitions {
            let mut overloads = Vec::new();
            for overload in v {
                let input = TypeReference::from_master_type_name(&overload.input);
                let result = TypeReference::from_master_type_name(&overload.result);
                let strict = overload.strict;
                overloads.push(UnaryOperatorDefinition::new(&input, &result, strict));
            }
            master.insert(
                &OperatorName::from_parts(Some("pg_catalog"), k).to_name_string(),
                OperatorDefinition::left_unary(&overloads),
            );
        }
        Ok(master)
    }

    pub(super) fn load_right_unary<P>(path: P) -> Result<TrieMap<OperatorDefinition>, AnalysisError>
    where
        P: AsRef<Path>,
    {
        let master_definitions: HashMap<String, Vec<UnaryOperatorTypeDecl>> =
            deserialize_from_path(path)?;
        let mut master = TrieMap::new();
        for (k, v) in &master_definitions {
            let mut overloads = Vec::new();
            for overload in v {
                let input = TypeReference::from_master_type_name(&overload.input);
                let result = TypeReference::from_master_type_name(&overload.result);
                let strict = overload.strict;
                overloads.push(
                    UnaryOperatorDefinition::new(&input, &result, strict).with_settings(
                        overload
                            .settings
                            .as_ref()
                            .map(|settings| settings.resolve(None, &input)),
                    ),
                );
            }
            master.insert(
                &OperatorName::from_parts(Some("pg_catalog"), k).to_name_string(),
                OperatorDefinition::right_unary(&overloads),
            );
        }
        Ok(master)
    }
}
