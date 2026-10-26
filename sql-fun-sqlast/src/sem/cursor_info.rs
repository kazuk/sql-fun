use crate::sem::TableName;

/// Static facts supplied by a session provider for one PostgreSQL portal.
/// Unknown facts remain unknown; position and lifetime are execution concerns.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CursorInfo {
    target: Option<TableName>,
    updatable: Option<bool>,
}
impl CursorInfo {
    /// Supply a qualified target identity and static update eligibility when known.
    pub fn new(target: Option<&TableName>, updatable: Option<bool>) -> Self {
        Self {
            target: target.cloned(),
            updatable,
        }
    }
    /// Qualified relation scanned by the cursor, if established by the provider.
    pub const fn target(&self) -> Option<&TableName> {
        self.target.as_ref()
    }
    /// Static update eligibility, independent of the cursor's current position.
    pub const fn updatable(&self) -> Option<bool> {
        self.updatable
    }
}
