use crate::sem::CursorInfo;

/// Explicit current-session portal bindings; names are exact and unique.
/// These are portal names, not PL/pgSQL refcursor variable names. A procedural
/// analyzer must resolve a variable's value before establishing a portal binding.
/// Bindings are not inherited from or frozen into a schema catalog.
#[cfg_attr(test, derive(Clone))]
#[derive(Debug, Default)]
pub(crate) struct CursorBindings {
    portals: std::collections::HashMap<String, CursorInfo>,
}

impl CursorBindings {
    /// Establish or replace the static facts for an exact portal name.
    pub(crate) fn bind(&mut self, name: &str, info: &CursorInfo) {
        self.portals.insert(name.to_owned(), info.clone());
    }
    /// Look up an exact portal name in the current scope.
    pub(crate) fn cursor(&self, name: &str) -> Option<&CursorInfo> {
        self.portals.get(name)
    }
    /// Remove a binding when the caller changes the analysis session scope.
    pub(crate) fn remove(&mut self, name: &str) {
        self.portals.remove(name);
    }
}
