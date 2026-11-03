//! Lightweight identity builders and assertions; no catalog initialization.
use crate::names::{OperatorName, TableName, ValidOperatorName};

impl ValidOperatorName {
    /// Construct an unqualified operator identity for a test.
    pub fn new(name: &str) -> Self {
        Self::from_parts(None, name)
    }
}
impl OperatorName {
    /// Construct an unqualified operator identity for a test.
    pub fn new(name: &str) -> Self {
        Self::from_parts(None, name)
    }
}
impl TableName {
    /// assert table name is expected value
    pub fn assert_table_name(&self, expected: &str) {
        assert_eq!(self.full_name().name().as_str(), expected)
    }
}
