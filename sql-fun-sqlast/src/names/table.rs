use std::fmt::Display;

use crate::names::FullName;

/// table name
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TableName(FullName);

impl From<&Self> for TableName {
    fn from(value: &Self) -> Self {
        value.clone()
    }
}

impl Display for TableName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl TableName {
    pub(crate) fn rename_reference(&mut self, old: &Self, new: &Self) {
        if self == old {
            *self = new.clone();
        }
    }

    /// gets name content
    #[must_use]
    pub const fn full_name(&self) -> &FullName {
        &self.0
    }
}

impl From<FullName> for TableName {
    fn from(value: FullName) -> Self {
        Self(value)
    }
}
