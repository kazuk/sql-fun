use crate::{
    sem::{CatalogName, FullName, LocalName, SchemaName},
    syn::RangeVar,
};

use std::fmt::Display;

/// view name type
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ViewName(FullName);

impl Display for ViewName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)?;
        Ok(())
    }
}

impl ViewName {
    pub(crate) fn from_full_name(name: &FullName) -> Self {
        Self(name.clone())
    }

    fn to_option(string: String) -> Option<String> {
        if string.is_empty() {
            None
        } else {
            Some(string)
        }
    }

    pub(crate) const fn full_name(&self) -> &FullName {
        &self.0
    }
}

impl From<RangeVar> for ViewName {
    fn from(value: RangeVar) -> Self {
        Self::from(&value)
    }
}

impl From<&RangeVar> for ViewName {
    fn from(value: &RangeVar) -> Self {
        let catalog = Self::to_option(value.get_catalogname());
        let schema = Self::to_option(value.get_schemaname());
        let relation = value.get_relname();
        Self(FullName::new(
            &catalog.as_deref().map(CatalogName::from),
            &schema.as_deref().map(SchemaName::from),
            &LocalName::from(&relation),
        ))
    }
}
