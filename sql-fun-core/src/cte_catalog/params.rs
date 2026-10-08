use super::CteName;
use crate::placehlodered_sql::{ParameterName, PlaceholderTypeHint};
use std::collections::{HashMap, hash_map::Entry};

use ::serde::ser::{SerializeMap, SerializeStruct};

/// renaming definition
#[derive(Debug, serde::Deserialize)]
pub struct CteParameterRename {
    #[serde(rename = "cte")]
    cte_name: CteName,
    #[serde(rename = "place_holder")]
    parameter_name: ParameterName,
}

impl CteParameterRename {
    /// CTE name
    #[must_use]
    pub const fn cte_name(&self) -> &CteName {
        &self.cte_name
    }

    /// target parameter name
    #[must_use]
    pub const fn parameter_name(&self) -> &ParameterName {
        &self.parameter_name
    }
}

/// parameter definition
#[derive(Debug)]
pub struct CteParameter {
    cte_name: CteName,
    name_in_query: ParameterName,
    type_hints: Vec<PlaceholderTypeHint>,
}

/// parameter value binding
#[derive(Debug, Clone)]
pub struct CteParameterBinding {
    cte_name: CteName,
    parameter_name: ParameterName,
    name_in_query: ParameterName,
    type_hints: Vec<PlaceholderTypeHint>,
}

impl CteParameterBinding {
    /// CTE name for parameter target
    #[must_use]
    pub const fn cte_name(&self) -> &CteName {
        &self.cte_name
    }

    /// parameter name
    #[must_use]
    pub const fn parameter_name(&self) -> &ParameterName {
        &self.parameter_name
    }

    /// parameter definition name in query
    #[must_use]
    pub const fn name_in_query(&self) -> &ParameterName {
        &self.name_in_query
    }

    /// enumerate type hints in definition
    #[must_use]
    pub fn type_hints(&self) -> &[PlaceholderTypeHint] {
        self.type_hints.as_ref()
    }

    /// create binding from parameter definition
    #[must_use]
    pub fn from_parameter(parameter: &CteParameter) -> Self {
        Self {
            cte_name: parameter.cte_name.clone(),
            parameter_name: parameter.name_in_query.clone(),
            name_in_query: parameter.name_in_query.clone(),
            type_hints: parameter.type_hints.clone(),
        }
    }

    /// create binding from parameter with rename
    #[must_use]
    pub fn from_parameter_with_rename(parameter: &CteParameter, name: &ParameterName) -> Self {
        Self {
            cte_name: parameter.cte_name.clone(),
            name_in_query: parameter.name_in_query.clone(),
            parameter_name: name.clone(),
            type_hints: parameter.type_hints.clone(),
        }
    }

    /// create clone binding with rename
    #[must_use]
    pub fn derive_with_rename(&self, name: &ParameterName) -> Self {
        Self {
            cte_name: self.cte_name.clone(),
            name_in_query: self.name_in_query.clone(),
            parameter_name: name.clone(),
            type_hints: self.type_hints.clone(),
        }
    }
}

/// A catalog entry's contribution to one resolved parameter.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CteQueryBinding {
    cte_name: CteName,
    type_hints: Vec<PlaceholderTypeHint>,
}

impl CteQueryBinding {
    fn new(cte_name: &CteName, hints: &[PlaceholderTypeHint]) -> Self {
        Self {
            cte_name: cte_name.clone(),
            type_hints: hints.to_owned(),
        }
    }

    /// Catalog entry supplying this binding.
    #[must_use]
    pub const fn cte_name(&self) -> &CteName {
        &self.cte_name
    }

    /// Type hints contributed by the catalog entry.
    #[must_use]
    pub fn type_hints(&self) -> &[PlaceholderTypeHint] {
        &self.type_hints
    }
}

/// collection of parameter binding
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct CteParameterBindingCollection {
    cte_bindings: Vec<CteQueryBinding>,
    query_type_hints: Vec<PlaceholderTypeHint>,
}

impl CteParameterBindingCollection {
    /// Type hints supplied by the application query.
    #[must_use]
    pub fn query_type_hints(&self) -> &[PlaceholderTypeHint] {
        &self.query_type_hints
    }

    /// Catalog entry names and their parameter type hints.
    #[must_use]
    pub fn cte_bindings(&self) -> &[CteQueryBinding] {
        &self.cte_bindings
    }

    /// add binding to parameter
    pub fn push_binding(&mut self, cte_name: &CteName, hints: &[PlaceholderTypeHint]) {
        self.cte_bindings
            .push(CteQueryBinding::new(cte_name, hints));
    }

    /// append all parameter binding
    pub fn append(&mut self, other: &Self) {
        self.cte_bindings.extend(other.cte_bindings.clone());
        self.query_type_hints.extend(other.query_type_hints.clone());
    }
}

/// parameter collection for CTE
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct CteParameterCollection {
    parameters: HashMap<ParameterName, CteParameterBindingCollection>,
}

// Preserve existing lockfile fingerprints: the base SQL is hashed separately,
// and only catalog contributions appeared in the previous parameter encoding.
struct LegacyFingerprint<'a>(&'a CteParameterCollection);

impl ::serde::Serialize for LegacyFingerprint<'_> {
    fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut outer = serializer.serialize_struct("CteParameterCollection", 1)?;
        outer.serialize_field("parameters", &LegacyBindingMap(&self.0.parameters))?;
        outer.end()
    }
}

struct LegacyBindingMap<'a>(&'a HashMap<ParameterName, CteParameterBindingCollection>);

impl ::serde::Serialize for LegacyBindingMap<'_> {
    fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        for (name, bindings) in self.0 {
            if bindings.cte_bindings.is_empty() {
                continue;
            }
            let previous_shape: Vec<_> = bindings
                .cte_bindings
                .iter()
                .map(|binding| (&binding.cte_name, &binding.type_hints))
                .collect();
            map.serialize_entry(name, &previous_shape)?;
        }
        map.end()
    }
}

impl CteParameterCollection {
    pub(super) fn legacy_fingerprint_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&LegacyFingerprint(self))
    }

    /// Include application-query parameters without inventing a catalog entry.
    pub fn include_base_query(&mut self, query: &crate::PlaceholderedSql) {
        for (name, hints) in query.parameters() {
            self.parameters
                .entry(name.clone())
                .or_default()
                .query_type_hints
                .extend(hints.clone());
        }
    }

    /// Binding metadata for one resolved parameter identity.
    #[must_use]
    pub fn binding(&self, name: &ParameterName) -> Option<&CteParameterBindingCollection> {
        self.parameters.get(name)
    }

    /// register parameter
    pub fn push_query_parameter(
        &mut self,
        cte_name: &CteName,
        parameter_name: &ParameterName,
        hints: &Vec<PlaceholderTypeHint>,
    ) {
        match self.parameters.entry(parameter_name.clone()) {
            Entry::Occupied(mut o) => {
                let bindings = o.get_mut();
                bindings.push_binding(cte_name, hints);
            }
            Entry::Vacant(v) => {
                let mut bindings = CteParameterBindingCollection::default();
                bindings.push_binding(cte_name, hints);
                v.insert(bindings);
            }
        }
    }

    /// register renaming parameter
    pub fn push_binding_with_rename(
        &mut self,
        _rename: &CteParameterRename,
        _binding: &CteParameterBindingCollection,
    ) {
        todo!()
    }

    /// register cascading parameter
    pub fn push_binding(
        &mut self,
        parameter_name: &ParameterName,
        binding: &CteParameterBindingCollection,
    ) {
        match self.parameters.entry(parameter_name.clone()) {
            Entry::Occupied(mut o) => {
                let bindings = o.get_mut();
                bindings.append(binding);
            }
            Entry::Vacant(v) => {
                v.insert(binding.clone());
            }
        }
    }

    /// return true when parameter exists
    #[must_use]
    pub fn exists(&self, parameter_name: &ParameterName) -> bool {
        self.parameters.contains_key(parameter_name)
    }

    /// get parameter names
    pub fn parameter_names(&self) -> impl Iterator<Item = &ParameterName> {
        self.parameters.keys()
    }

    /// marge parameter collection
    pub fn marge(&mut self, mut other: Self) {
        for (param_name, input_bindings) in &mut other.parameters {
            match self.parameters.entry(param_name.clone()) {
                Entry::Occupied(mut o) => {
                    let bindings = o.get_mut();
                    bindings.append(input_bindings);
                }
                Entry::Vacant(v) => {
                    v.insert(input_bindings.clone());
                }
            }
        }
    }

    fn iter(
        &self,
    ) -> std::collections::hash_map::Iter<'_, ParameterName, CteParameterBindingCollection> {
        self.parameters.iter()
    }
}

impl<'a> IntoIterator for &'a CteParameterCollection {
    type Item = (&'a ParameterName, &'a CteParameterBindingCollection);
    type IntoIter =
        std::collections::hash_map::Iter<'a, ParameterName, CteParameterBindingCollection>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// collection of renaming definitions
#[derive(Debug)]
pub struct ParameterRenameCollection(HashMap<ParameterName, Vec<CteParameterRename>>);

impl From<HashMap<String, Vec<CteParameterRename>>> for ParameterRenameCollection {
    fn from(value: HashMap<String, Vec<CteParameterRename>>) -> Self {
        Self(
            value
                .into_iter()
                .map(|(k, v)| (ParameterName::from(k.as_str()), v))
                .collect(),
        )
    }
}

impl ParameterRenameCollection {
    /// get parameter rename by parameter name
    #[must_use]
    pub fn get(&self, param_name: &ParameterName) -> Option<&Vec<CteParameterRename>> {
        self.0.get(param_name)
    }

    /// get rename definition with CTE and parameter name
    #[must_use]
    pub fn get_with(
        &self,
        cte_name: &CteName,
        parameter_name: &ParameterName,
    ) -> Option<&CteParameterRename> {
        if let Some(renames) = self.0.get(parameter_name) {
            renames.iter().find(|v| &v.cte_name == cte_name)
        } else {
            None
        }
    }

    /// iterate items
    pub fn iter(&self) -> impl Iterator<Item = (&ParameterName, &Vec<CteParameterRename>)> {
        self.0.iter()
    }
}
