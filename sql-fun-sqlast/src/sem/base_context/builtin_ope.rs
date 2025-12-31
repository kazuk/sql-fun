use crate::{
    TrieMap,
    sem::{
        AnalysisError, BinaryOperatorDefinition, OperatorDefinition, TypeReference,
        UnaryOperatorDefinition,
    },
};
use std::{collections::HashMap, path::Path};

#[derive(serde::Deserialize)]
struct BinaryOperatorTypeDecl {
    left: String,
    right: String,
    result: String,
    strict: bool,
}

#[derive(serde::Deserialize)]
struct UnaryOperatorTypeDecl {
    input: String,
    result: String,
    strict: bool,
}

pub struct PgBuiltinOperators {}

impl PgBuiltinOperators {
    pub(super) fn load_binary<P>(path: P) -> Result<TrieMap<OperatorDefinition>, AnalysisError>
    where
        P: AsRef<Path>,
    {
        let master_definitions: HashMap<String, Vec<BinaryOperatorTypeDecl>> =
            super::deserialize_from_path(path)?;

        let mut master = TrieMap::new();
        for (k, v) in master_definitions.iter() {
            let mut overloads = Vec::new();
            for overload in v {
                let left = TypeReference::from_master_type_name(&overload.left);
                let right = TypeReference::from_master_type_name(&overload.right);
                let result = TypeReference::from_master_type_name(&overload.result);
                let strict = overload.strict;
                overloads.push(BinaryOperatorDefinition::new(
                    &left, &right, &result, strict,
                ));
            }
            master.insert(k, OperatorDefinition::binary(&overloads));
        }
        assert!(!master.is_empty());
        Ok(master)
    }

    pub(super) fn load_left_unary<P>(path: P) -> Result<TrieMap<OperatorDefinition>, AnalysisError>
    where
        P: AsRef<Path>,
    {
        let master_definitions: HashMap<String, Vec<UnaryOperatorTypeDecl>> =
            super::deserialize_from_path(path)?;
        let mut master = TrieMap::new();
        for (k, v) in master_definitions.iter() {
            let mut overloads = Vec::new();
            for overload in v {
                let input = TypeReference::from_master_type_name(&overload.input);
                let result = TypeReference::from_master_type_name(&overload.result);
                let strict = overload.strict;
                overloads.push(UnaryOperatorDefinition::new(&input, &result, strict));
            }
            master.insert(k, OperatorDefinition::left_unary(&overloads));
        }
        Ok(master)
    }

    pub(super) fn load_right_unary<P>(path: P) -> Result<TrieMap<OperatorDefinition>, AnalysisError>
    where
        P: AsRef<Path>,
    {
        let master_definitions: HashMap<String, Vec<UnaryOperatorTypeDecl>> =
            super::deserialize_from_path(path)?;
        let mut master = TrieMap::new();
        for (k, v) in master_definitions.iter() {
            let mut overloads = Vec::new();
            for overload in v {
                let input = TypeReference::from_master_type_name(&overload.input);
                let result = TypeReference::from_master_type_name(&overload.result);
                let strict = overload.strict;
                overloads.push(UnaryOperatorDefinition::new(&input, &result, strict));
            }
            master.insert(k, OperatorDefinition::right_unary(&overloads));
        }
        Ok(master)
    }
}
