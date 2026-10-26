use std::fmt::Display;

use crate::{
    sem::{AnalysisError, FullName},
    syn::RangeVar,
};

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

impl TryFrom<RangeVar> for TableName {
    type Error = AnalysisError;

    fn try_from(value: RangeVar) -> Result<Self, Self::Error> {
        Ok(Self(FullName::try_from(value)?))
    }
}

#[cfg(test)]
impl TableName {
    /// assert table name is expected value
    pub fn assert_table_name(&self, expected: &str) {
        assert_eq!(self.full_name().name().as_str(), expected)
    }
}

#[cfg(feature = "sqlite")]
use crate::syn::sqlite::{Name, QualifiedName};

#[cfg(feature = "sqlite")]
impl TryFrom<&QualifiedName> for TableName {
    type Error = AnalysisError;

    fn try_from(value: &QualifiedName) -> Result<Self, Self::Error> {
        Ok(Self(FullName::try_from(value)?))
    }
}

#[cfg(feature = "sqlite")]
impl TryFrom<&Name> for TableName {
    type Error = AnalysisError;

    fn try_from(value: &Name) -> Result<Self, Self::Error> {
        Ok(Self(FullName::from_local_name(value.value())))
    }
}
