use crate::sem::{
    AnalysisError, BaseParseContext, FullName, InsertArbiter, InsertConflict, TableName,
};

/// A completed catalog name lookup, retaining the requested spelling separately
/// from catalog identities (schema-qualified in PostgreSQL). Missing and
/// ambiguous are distinct results.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CatalogNameLookup {
    requested: FullName,
    outcome: LookupOutcome,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
enum LookupOutcome {
    Found(FullName),
    Missing,
    Ambiguous(Vec<FullName>),
}
impl CatalogNameLookup {
    /// Record a unique catalog binding.
    pub fn found(requested: &FullName, resolved: &FullName) -> Self {
        Self {
            requested: requested.clone(),
            outcome: LookupOutcome::found(resolved),
        }
    }
    /// Record a completed lookup with no matching definition.
    pub fn missing(requested: &FullName) -> Self {
        Self {
            requested: requested.clone(),
            outcome: LookupOutcome::missing(),
        }
    }
    /// Record competing definitions after dialect visibility rules are applied.
    pub fn ambiguous(requested: &FullName, candidates: &[FullName]) -> Self {
        Self {
            requested: requested.clone(),
            outcome: LookupOutcome::ambiguous(candidates),
        }
    }
    /// Name supplied by the SQL statement.
    pub const fn requested(&self) -> &FullName {
        &self.requested
    }
    /// Catalog identity, present only for a unique binding.
    pub const fn resolved_name(&self) -> Option<&FullName> {
        self.outcome.resolved_name()
    }
    /// Whether the catalog found no visible definition.
    pub const fn is_missing(&self) -> bool {
        self.outcome.is_missing()
    }
    /// Competing catalog identities, present only for an ambiguous lookup.
    pub fn ambiguous_candidates(&self) -> Option<&[FullName]> {
        self.outcome.ambiguous_candidates()
    }
}
impl LookupOutcome {
    fn found(name: &FullName) -> Self {
        Self::Found(name.clone())
    }
    const fn missing() -> Self {
        Self::Missing
    }
    fn ambiguous(names: &[FullName]) -> Self {
        Self::Ambiguous(names.to_vec())
    }
    const fn resolved_name(&self) -> Option<&FullName> {
        if let Self::Found(name) = self {
            Some(name)
        } else {
            None
        }
    }
    const fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }
    fn ambiguous_candidates(&self) -> Option<&[FullName]> {
        if let Self::Ambiguous(names) = self {
            Some(names)
        } else {
            None
        }
    }
}

/// Catalog services required by conflict-target name resolution and inference.
///
/// These methods must return completed lookups, never guess a binding from its
/// spelling. A catalog that cannot perform a lookup must explicitly remain
/// unimplemented at its implementation boundary.
pub trait ConflictInfoRead {
    /// Source database encoding and default-collation provenance, absent for SQLite.
    fn database_collation(&self) -> Option<&crate::sem::DatabaseCollation>;

    /// Read one PostgreSQL collation at an exact schema-qualified name.
    /// The provider selects the variant eligible for its database encoding.
    /// Unqualified/catalog-qualified names and SQLite namespaces return `None`.
    fn collation_definition(&self, name: &FullName) -> Option<&crate::sem::CollationDefinition>;

    /// Read one PostgreSQL operator class at an exact schema-qualified name for
    /// the explicitly supplied access method. Does not select a default class.
    fn operator_class_definition(
        &self,
        name: &FullName,
        access_method: &str,
    ) -> Option<&crate::sem::OperatorClassDefinition>;

    /// Resolve a name in the explicit SQLite connection-local namespace.
    /// Available independently of the context's historical analysis dialect;
    /// qualified names are missing. No connection is inspected.
    fn lookup_sqlite_collation(&self, name: &FullName) -> CatalogNameLookup;

    /// Resolve a collation using the selected dialect's namespace rules.
    /// PostgreSQL uses explicit qualification or the active search path (and
    /// database encoding); SQLite uses its case-insensitive connection namespace.
    fn lookup_collation(&self, name: &FullName) -> CatalogNameLookup;

    /// Resolve a PostgreSQL operator class for the specified index access method.
    /// Qualification/search-path visibility precedes ambiguity detection. Conflict
    /// inference uses the `btree` access method. SQLite has no operator classes.
    fn lookup_operator_class(&self, name: &FullName, access_method: &str) -> CatalogNameLookup;

    /// Infer eligible unique arbiters using bound expressions and modifier
    /// identities. Respect collation, operator-family/input-type equivalence,
    /// partial predicate implication, and dialect-specific matching rules.
    /// Failed modifier lookups must never yield a successful arbiter binding.
    /// The destination identity can denote a writable view. Apply the dialect's
    /// view rewrite mapping and return actual underlying arbiter identities;
    /// do not assume every writable view exposes its base relation's indexes.
    fn infer_conflict_arbiters(
        &self,
        table: &TableName,
        conflict: &InsertConflict,
    ) -> Result<Vec<InsertArbiter>, AnalysisError>;
}

/// Existing catalog support, shared by production contexts until extractor
/// metadata can provide index modifiers and predicate implication services.
pub(crate) fn infer_from_catalog(
    context: &impl BaseParseContext,
    table: &TableName,
    conflict: &InsertConflict,
) -> Result<Vec<InsertArbiter>, AnalysisError> {
    if context.get_table(table).is_none() {
        return crate::sem::view_write::infer_view_arbiters(context, table, conflict);
    }
    if conflict.has_modifiers() {
        todo!("catalog index modifier eligibility metadata");
    }
    let mut columns = Vec::new();
    let mut only_columns = true;
    for expression in conflict.inference() {
        if let Some(name) = expression.index_column_name() {
            if !columns.contains(name) {
                columns.push(name.clone());
            }
        } else {
            only_columns = false;
        }
    }
    let mut arbiters = if only_columns {
        context
            .get_table(table)
            .expect("bound INSERT destination")
            .infer_insert_arbiters(table, &columns)
    } else {
        Vec::new()
    };
    let mut needs_implication = false;
    for index in context.indexes_for_table(table) {
        if let Some(eligible) = index.matches_inference(conflict)? {
            if eligible && !InsertArbiter::contains_backing_index(&arbiters, index.name()) {
                arbiters.push(InsertArbiter::index(index.name()));
            }
        } else {
            needs_implication = true;
        }
    }
    if arbiters.is_empty() && needs_implication {
        todo!("catalog partial-index predicate implication service");
    }
    Ok(arbiters)
}
