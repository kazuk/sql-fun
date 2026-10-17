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

/// Explicit current-session portal bindings; names are exact and unique.
/// These are portal names, not PL/pgSQL refcursor variable names. A procedural
/// analyzer must resolve a variable's value before establishing a portal binding.
/// Bindings are not inherited from or frozen into a schema catalog.
#[derive(Debug, Default)]
pub struct CursorBindings {
    portals: std::collections::HashMap<String, CursorInfo>,
}
impl CursorBindings {
    /// Establish or replace the static facts for an exact portal name.
    pub fn bind(&mut self, name: &str, info: &CursorInfo) {
        self.portals.insert(name.to_owned(), info.clone());
    }
    /// Look up an exact portal name in the current scope.
    pub fn cursor(&self, name: &str) -> Option<&CursorInfo> {
        self.portals.get(name)
    }
    /// Remove a binding when the caller changes the analysis session scope.
    pub fn remove(&mut self, name: &str) {
        self.portals.remove(name);
    }
}
