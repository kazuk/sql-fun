use crate::sem::{AliasName, AliasSpec, AnalysisError, DataSource, FuncCallExpr, Nullable};

/// A table invocation with completed name lookup and deferred signature/columns.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeferredTableFunction {
    call: FuncCallExpr,
    alias: AliasSpec,
}
impl DeferredTableFunction {
    pub(super) fn new(call: &FuncCallExpr, alias: &AliasSpec) -> Self {
        Self {
            call: call.clone(),
            alias: alias.clone(),
        }
    }
    /// Preserved invocation, including name lookup, supplied arguments and modifiers.
    pub const fn call(&self) -> &FuncCallExpr {
        &self.call
    }
    /// Explicit or SQL-derived table alias.
    pub const fn alias(&self) -> &AliasSpec {
        &self.alias
    }
    pub(super) fn push_alias_names(&self, names: &mut Vec<AliasSpec>) {
        names.push(self.alias.clone());
    }
    pub(super) fn resolve_alias<'a>(
        &self,
        source: &'a DataSource,
        name: &AliasName,
    ) -> Option<(&'a DataSource, Nullable)> {
        if self.alias.name() == name {
            Some((source, false))
        } else {
            None
        }
    }
    pub(super) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.call.walk(visitor)
    }
}
impl DataSource {
    pub(crate) fn deferred_table_function(call: &FuncCallExpr, alias: &AliasSpec) -> Self {
        Self::DeferredTableFunc(DeferredTableFunction::new(call, alias))
    }
    /// Table invocation whose signature and output columns remain deferred.
    pub const fn deferred_function(&self) -> Option<&DeferredTableFunction> {
        match self {
            Self::DeferredTableFunc(value) => Some(value),
            _ => None,
        }
    }
}
