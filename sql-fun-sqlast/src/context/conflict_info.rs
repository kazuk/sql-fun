use crate::{
    names::{FullName, TableName},
    sem::{
        AnalysisError, CatalogNameLookup, CollationDefinition, ConflictArbiterInference,
        DatabaseCollation, InsertConflict, OperatorClassDefinition,
    },
};

/// Catalog services required by conflict-target name resolution and inference.
///
/// These methods must return completed lookups, never guess a binding from its
/// spelling. A catalog that cannot perform a lookup must explicitly remain
/// unimplemented at its implementation boundary.
///
/// Explicit conflict collation/operator-class references retain their completed
/// catalog lookups, including missing and ambiguous outcomes. Custom analysis
/// contexts provide these lookups and modifier-aware arbiter inference through
/// `ConflictInfoRead`; SQL spellings are not treated as resolved identities.
/// Production PostgreSQL catalogs load collation and operator-class definitions
/// from `collations.json` and `operator_classes.json` in the explicitly selected
/// catalog generation. Both files are required; incomplete or malformed
/// generations fail to load. Name lookup honors qualification, the current
/// search path, database encoding, and the requested access method.
/// `ConflictInfoRead` provides purpose-specific `database_collation`,
/// `collation_definition`, and `operator_class_definition` reads for
/// database-default provenance and qualified definitions with
/// operator-family/type relations. The provider determines which declarations
/// are eligible for the database encoding and requested access method. Frozen
/// catalogs retain the same read services.
///
/// SQLite lookup uses its connection-local, case-insensitive collation
/// namespace. BINARY, NOCASE, and RTRIM are available. API consumers must
/// explicitly declare custom names with [`crate::context::BaseContext::declare_sqlite_collation`]
/// before freezing the base; this declaration records availability for analysis
/// and does not install a runtime collation or inspect a connection.
///
pub trait ConflictInfoRead {
    /// Source database encoding and default-collation provenance, absent for SQLite.
    fn database_collation(&self) -> Option<&DatabaseCollation>;

    /// Read one PostgreSQL collation at an exact schema-qualified name.
    /// The provider selects the variant eligible for its database encoding.
    /// Unqualified/catalog-qualified names and SQLite namespaces return `None`.
    fn collation_definition(&self, name: &FullName) -> Option<&CollationDefinition>;

    /// Read one PostgreSQL operator class at an exact schema-qualified name for
    /// the explicitly supplied access method. Does not select a default class.
    fn operator_class_definition(
        &self,
        name: &FullName,
        access_method: &str,
    ) -> Option<&OperatorClassDefinition>;

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
    /// Reads table catalog facts. Writable-view rewrite analysis belongs to
    /// `ViewWriteEvaluation` and requires the active `ParseContext`.
    fn infer_conflict_arbiters(
        &self,
        table: &TableName,
        conflict: &InsertConflict,
    ) -> Result<ConflictArbiterInference, AnalysisError>;
}
