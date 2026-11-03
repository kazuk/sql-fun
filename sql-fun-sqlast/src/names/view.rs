use crate::names::FullName;

use std::fmt::Display;

/// view name type
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ViewName(FullName);

impl From<&Self> for ViewName {
    fn from(value: &Self) -> Self {
        value.clone()
    }
}

impl Display for ViewName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)?;
        Ok(())
    }
}

impl ViewName {
    /// Construct a view name from an explicit catalog identity.
    pub fn from_full_name(name: &FullName) -> Self {
        Self(name.clone())
    }

    pub(crate) const fn full_name(&self) -> &FullName {
        &self.0
    }
}
