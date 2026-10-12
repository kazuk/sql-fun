#[cfg(test)]
mod test_helpers;

use crate::{cache::Cache, file_hash_cache::FileHashCache, selected_context::SelectedContext};
use ::sql_fun_core::{Hasher, PostgresEngineVersion, SqlDialect, SqlFunArgs, SqlFunMetadata};
use ::sql_fun_sqlast::{
    ParseDumpFileError, PostgresDialect, SqlFileSnapshot,
    sem::{AnalysisProblem, BaseContext, FrozenBaseContext},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Hierarchical SHA-256 identity of a frozen schema, independent of session authority.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SchemaContextKey(String);

impl SchemaContextKey {
    fn root(version: &PostgresEngineVersion, search_path: &[String]) -> Self {
        let mut hasher = Hasher::new();
        hasher.append_str(&SqlDialect::PostgreSQL.to_string());
        hasher.append_str(&version.to_string());
        hasher
            .append_str(&::serde_json::to_string(search_path).expect("string array serialization"));
        Self(hasher.finish())
    }

    fn layer(&self, hash: &str, path: &Path) -> Result<Self, SchemaLoadError> {
        let Some(path) = path.to_str() else {
            return Err(SchemaLoadError::invalid(
                "canonical schema path is not UTF-8",
            ));
        };
        let mut hasher = Hasher::new();
        hasher.append_str(&self.0);
        hasher.append_str(hash);
        hasher.append_str(path);
        Ok(Self(hasher.finish()))
    }

    /// Lowercase hexadecimal SHA-256.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Ordered recoverable diagnostics belonging to one explicitly loaded layer.
#[derive(Debug)]
pub struct LayerDiagnostics {
    path: PathBuf,
    problems: Vec<AnalysisProblem>,
}

impl LayerDiagnostics {
    /// Canonical file whose analysis produced these diagnostics.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Recoverable problems in original analysis order.
    #[must_use]
    pub fn problems(&self) -> &[AnalysisProblem] {
        &self.problems
    }
}

/// A successful immutable analysis, including all prefix diagnostics.
#[derive(Debug)]
pub struct CachedSchemaAnalysis {
    base: FrozenBaseContext,
    diagnostics: Vec<Arc<LayerDiagnostics>>,
}

impl CachedSchemaAnalysis {
    /// Shared catalog; semantic analysis never requires a cache lock.
    #[must_use]
    pub const fn base_context(&self) -> &FrozenBaseContext {
        &self.base
    }
    /// Layer diagnostics in loading order, unchanged on a cache hit.
    #[must_use]
    pub fn diagnostics(&self) -> &[Arc<LayerDiagnostics>] {
        &self.diagnostics
    }
}

#[derive(Debug)]
struct CacheableSchemaError(String);

impl CacheableSchemaError {
    fn classify(error: &ParseDumpFileError) -> Result<Self, SchemaLoadError> {
        if error.is_io() {
            Err(SchemaLoadError::source(error))
        } else {
            Ok(Self(error.to_string()))
        }
    }
}

type Outcome = Arc<Result<Arc<CachedSchemaAnalysis>, CacheableSchemaError>>;

#[derive(Clone)]
struct LoadedLayer {
    key: SchemaContextKey,
    hash: Option<String>,
    outcome: Outcome,
}

/// An initialization failure or request-specific content mismatch.
#[derive(Clone, Debug, thiserror::Error)]
#[error("{message}")]
pub struct SchemaLoadError {
    message: String,
}

impl SchemaLoadError {
    fn invalid(message: &str) -> Self {
        Self {
            message: message.to_owned(),
        }
    }
    pub(crate) fn source(error: &impl std::fmt::Display) -> Self {
        Self::invalid(&error.to_string())
    }
    fn mismatch(expected: &str, actual: &str) -> Self {
        Self::invalid(&format!(
            "schema dump changed: expected {expected}, actual {actual}; retry with the current file hash"
        ))
    }
}

/// Explicit datasource input; creation does not discover or open project files.
///
/// The caller resolves the requested datasource in its explicitly selected
/// metadata. Extension files are supplied in the caller's chosen order.
pub struct SchemaSelection {
    args: SqlFunArgs,
    metadata: SqlFunMetadata,
    version: PostgresEngineVersion,
    search_path: Vec<String>,
    extensions: Vec<PathBuf>,
    dump: PathBuf,
    expected_hash: String,
}

impl SchemaSelection {
    /// Validate a selected datasource and all inputs before any schema load.
    ///
    /// # Errors
    /// Rejects missing/unknown datasource, unsupported dialect, invalid engine
    /// configuration, and missing or malformed expected content hashes.
    pub fn new(
        datasource: &str,
        args: &SqlFunArgs,
        metadata: &SqlFunMetadata,
        extensions: &[PathBuf],
        dump: &Path,
        expected_hash: &str,
    ) -> Result<Self, SchemaLoadError> {
        if datasource.is_empty() || datasource != metadata.name() {
            return Err(SchemaLoadError::invalid(
                "missing or unknown selected datasource",
            ));
        }
        if args
            .sql_dialect(metadata)
            .map_err(|e| SchemaLoadError::source(&e))?
            != SqlDialect::PostgreSQL
        {
            return Err(SchemaLoadError::invalid(
                "schema cache initialization currently supports PostgreSQL",
            ));
        }
        if expected_hash.len() != 64
            || !expected_hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(SchemaLoadError::invalid(
                "expected_hash must be a lowercase hexadecimal SHA-256",
            ));
        }
        Ok(Self {
            args: args.clone(),
            metadata: metadata.clone(),
            version: args
                .postgres_version(metadata)
                .map_err(|e| SchemaLoadError::source(&e))?,
            search_path: args
                .postgres_search_path(metadata)
                .map_err(|e| SchemaLoadError::source(&e))?,
            extensions: extensions.to_vec(),
            dump: dump.to_path_buf(),
            expected_hash: expected_hash.to_owned(),
        })
    }
}

/// Independently owned result ready for a session to register.
pub struct InitializedSchema {
    key: SchemaContextKey,
    actual_hash: String,
    analysis: Arc<CachedSchemaAnalysis>,
    selected: FrozenBaseContext,
}

impl InitializedSchema {
    /// Cache identity, not a session handle.
    #[must_use]
    pub const fn key(&self) -> &SchemaContextKey {
        &self.key
    }
    /// Hash of the original application dump contents.
    #[must_use]
    pub fn actual_hash(&self) -> &str {
        &self.actual_hash
    }
    /// Request-specific metadata/args over the shared catalog.
    ///
    /// Query consumers create their own mutable context over this frozen view.
    #[must_use]
    pub const fn base_context(&self) -> &FrozenBaseContext {
        &self.selected
    }
    /// Successful shared analysis retained independently of the service.
    #[must_use]
    pub fn analysis(&self) -> &CachedSchemaAnalysis {
        self.analysis.as_ref()
    }
}

/// Server-lifetime schema cache for one explicitly selected builtin installation.
///
/// Extracted builtin JSON masters are fixed for the service lifetime; restart
/// after re-extracting them. Builtin schema.sql uses the ordinary watched file layer.
/// A fixed installation prevents identical root keys aliasing different catalogs.
/// File layers are watched and identified by their original content hashes.
/// No contexts are evicted. Keep an Arc to this service in the server/session host.
pub struct SchemaCacheService {
    home: PathBuf,
    hashes: Arc<FileHashCache>,
    roots: Cache<SchemaContextKey, LoadedLayer, SchemaLoadError>,
    layers: Arc<Cache<SchemaContextKey, Outcome, SchemaLoadError>>,
    inflight: Cache<LayerLoadKey, LoadedLayer, SchemaLoadError>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct LayerLoadKey {
    parent: SchemaContextKey,
    path: PathBuf,
    generation: u64,
    observed_hash: String,
}

impl SchemaCacheService {
    /// Create a service scoped to the explicit builtin installation.
    ///
    /// # Errors
    /// Rejects unavailable installation paths. Does not read project metadata.
    pub fn new(home: &Path) -> Result<Self, SchemaLoadError> {
        Ok(Self {
            home: home
                .canonicalize()
                .map_err(|e| SchemaLoadError::source(&e))?,
            hashes: Arc::new(FileHashCache::new()),
            roots: Cache::new(true),
            layers: Arc::new(Cache::new(true)),
            inflight: Cache::new(false),
        })
    }

    pub(crate) fn require_installation(&self, home: &Path) -> Result<(), SchemaLoadError> {
        if home == self.home {
            Ok(())
        } else {
            Err(SchemaLoadError::invalid(
                "server builtin installation already selected; restart to select another installation",
            ))
        }
    }

    /// Load explicit builtin/extension/application layers and validate the request.
    ///
    /// Actual-content outcomes are published before expected-hash validation.
    /// Cancellation drops only this caller's receiver, never the server-owned load.
    ///
    /// # Panics
    /// Panics on a loader panic or violated cache invariant. The server
    /// executable aborts the process under its temporary fail-fast policy.
    ///
    /// # Errors
    /// Returns file/configuration errors, deterministic SQL errors, or a
    /// request-level expected/actual mismatch. I/O errors are never retained.
    pub async fn initialize(
        &self,
        selection: &SchemaSelection,
    ) -> Result<InitializedSchema, SchemaLoadError> {
        let root_key = SchemaContextKey::root(&selection.version, &selection.search_path);
        let home = self.home.clone();
        let version = selection.version.clone();
        let search_path = selection.search_path.clone();
        let key = root_key.clone();
        let mut layer = self
            .roots
            .load(&root_key, move || async move {
                ::tokio::task::spawn_blocking(move || {
                    Self::load_root(&home, &version, &search_path, &key)
                })
                .await
                .expect("builtin loader panicked")
            })
            .await?;
        let builtin_sql = selection
            .version
            .definition_base_path(&self.home)
            .join("schema.sql");
        layer = self.load_layer(&layer, &builtin_sql).await?;
        Self::successful(&layer.outcome)?;
        for extension in &selection.extensions {
            layer = self.load_layer(&layer, extension).await?;
            Self::successful(&layer.outcome)?;
        }
        layer = self.load_layer(&layer, &selection.dump).await?;
        // Mismatch takes precedence even over deterministic SQL failure, after publication.
        let actual_hash = layer
            .hash
            .expect("application file layer has a content hash");
        if actual_hash != selection.expected_hash {
            return Err(SchemaLoadError::mismatch(
                &selection.expected_hash,
                &actual_hash,
            ));
        }
        let analysis = Arc::clone(Self::successful(&layer.outcome)?);
        let selected =
            SelectedContext::freeze(&analysis.base, &selection.args, &selection.metadata);
        Ok(InitializedSchema {
            key: layer.key,
            actual_hash,
            analysis,
            selected,
        })
    }

    fn successful(outcome: &Outcome) -> Result<&Arc<CachedSchemaAnalysis>, SchemaLoadError> {
        outcome
            .as_ref()
            .as_ref()
            .map_err(|error| SchemaLoadError::source(&error.0))
    }

    fn load_root(
        home: &Path,
        version: &PostgresEngineVersion,
        search_path: &[String],
        key: &SchemaContextKey,
    ) -> Result<LoadedLayer, SchemaLoadError> {
        // Only schema-relevant inputs enter the shared root. No datasource config
        // or ambient environment is retained by the shared builtin catalog.
        let metadata: SqlFunMetadata = ::serde_json::from_value(::serde_json::json!({
            "engine": "PostgreSQL", "dialect": "postgres", "version": version.to_string(), "search_path": search_path,
        })).map_err(|e| SchemaLoadError::source(&e))?;
        let args = SqlFunArgs::builder(&HashMap::new())
            .map_err(|e| SchemaLoadError::source(&e))?
            .sql_fun_home(home)
            .postgres_version(version.clone())
            .postgres_search_path(search_path.to_vec())
            .term_color(::sql_fun_core::TerminalColor::Never)
            .build()
            .map_err(|e| SchemaLoadError::source(&e))?;
        let base = BaseContext::new(&args, &metadata).map_err(|e| SchemaLoadError::source(&e))?;
        Ok(LoadedLayer {
            key: key.clone(),
            hash: None,
            outcome: Arc::new(Ok(Arc::new(CachedSchemaAnalysis {
                base: base.into(),
                diagnostics: Vec::new(),
            }))),
        })
    }

    fn outcome(
        parsed: Result<::sql_fun_sqlast::ParsedSchemaDump, ParseDumpFileError>,
        path: &Path,
        prefix: &[Arc<LayerDiagnostics>],
    ) -> Result<Outcome, SchemaLoadError> {
        let result = match parsed {
            Ok(parsed) => {
                let mut diagnostics = prefix.to_vec();
                diagnostics.push(Arc::new(LayerDiagnostics {
                    path: path.to_path_buf(),
                    problems: parsed.problems().to_vec(),
                }));
                Ok(Arc::new(CachedSchemaAnalysis {
                    base: parsed.base_context().clone(),
                    diagnostics,
                }))
            }
            Err(error) => Err(CacheableSchemaError::classify(&error)?),
        };
        Ok(Arc::new(result))
    }

    async fn load_layer(
        &self,
        parent: &LoadedLayer,
        path: &Path,
    ) -> Result<LoadedLayer, SchemaLoadError> {
        Self::successful(&parent.outcome)?;
        let hashes = Arc::clone(&self.hashes);
        let path = path.to_path_buf();
        let (path, token, cached) = ::tokio::task::spawn_blocking(move || {
            let path = path
                .canonicalize()
                .map_err(|e| SchemaLoadError::source(&e))?;
            let token = hashes
                .begin(&path)
                .map_err(|e| SchemaLoadError::source(&e))?;
            let cached = hashes.lookup(&path, token);
            Ok::<_, SchemaLoadError>((path, token, cached))
        })
        .await
        .expect("file lookup task panicked")?;
        if let Some(hash) = cached {
            let key = parent.key.layer(&hash, &path)?;
            if let Some(outcome) = self.layers.lookup(&key) {
                return Ok(LoadedLayer {
                    key,
                    hash: Some(hash),
                    outcome,
                });
            }
        }
        let snapshot = ::tokio::task::spawn_blocking(move || {
            SqlFileSnapshot::read(&path).map_err(|e| SchemaLoadError::source(&e))
        })
        .await
        .expect("SQL reader panicked")?;
        self.resolve_layer(parent, snapshot, token).await
    }

    async fn resolve_layer(
        &self,
        parent: &LoadedLayer,
        snapshot: SqlFileSnapshot,
        token: crate::file_hash_cache::ReadToken,
    ) -> Result<LoadedLayer, SchemaLoadError> {
        let path = snapshot.path().to_path_buf();
        let hash = snapshot.content_hash();
        let candidate = parent.key.layer(hash, &path)?;
        if let Some(outcome) = self.layers.lookup(&candidate) {
            let _ = self.hashes.populate(&path, token, hash);
            return Ok(LoadedLayer {
                key: candidate,
                hash: Some(hash.to_owned()),
                outcome,
            });
        }
        let pending = LayerLoadKey {
            parent: parent.key.clone(),
            path: path.clone(),
            generation: FileHashCache::generation(token),
            observed_hash: hash.to_owned(),
        };
        let parent = parent.clone();
        let hashes = Arc::clone(&self.hashes);
        let layers = Arc::clone(&self.layers);
        self.inflight
            .load(&pending, move || async move {
                ::tokio::task::spawn_blocking(move || {
                    let base = Self::successful(&parent.outcome)?;
                    let parsed = PostgresDialect::parse_schema_snapshot(&snapshot, &base.base);
                    let outcome = Self::outcome(parsed, &path, &base.diagnostics)?;
                    let actual = snapshot.content_hash().to_owned();
                    let key = parent.key.layer(&actual, &path)?;
                    let outcome = layers.publish(&key, &outcome);
                    // Context publication is valid independently of path invalidation.
                    // Removal after read must not discard the content-addressed result.
                    let _ = hashes.populate(&path, token, &actual);
                    Ok(LoadedLayer {
                        key,
                        hash: Some(actual),
                        outcome,
                    })
                })
                .await
                .expect("schema loader panicked")
            })
            .await
    }
}

#[cfg(test)]
mod tests;
