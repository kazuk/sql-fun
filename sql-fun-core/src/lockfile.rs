#[cfg(test)]
mod test_helpers;

use std::{
    fs::File,
    io::{Seek, SeekFrom},
    ops::Range,
    path::{Path, PathBuf},
};

use ::indexmap::IndexMap;
use ::serde_spanned::Spanned;

/// errors related lockfile
#[derive(thiserror::Error, Debug)]
pub enum LockfileError {
    /// IO error
    #[error("IO Error :\n\t{0}")]
    Io(#[from] std::io::Error),

    /// toml encode decode error
    #[error("TOML Error:\n\t{0}")]
    Toml(#[from] toml::de::Error),

    /// TOML serialization failure
    #[error(transparent)]
    Encode(#[from] toml::ser::Error),

    /// Invalid lockfile format or statement representation
    #[error("Invalid lockfile: {0}")]
    Invalid(String),

    /// Schema dump hash code mismatch
    #[error("Schema Dump Mismatch: {0} expected={1}. actual={2}")]
    SchemaDumpMismatch(PathBuf, String, String),
}

impl LockfileError {
    fn invalid<T>(message: &str) -> Result<T, Self> {
        Err(Self::Invalid(message.to_owned()))
    }

    fn raise_schema_dump_mismatch<P: AsRef<Path>, T>(
        path: P,
        expect_hash_code: &str,
        actual_hash_code: &str,
    ) -> Result<T, Self> {
        Err(Self::SchemaDumpMismatch(
            path.as_ref().to_path_buf(),
            String::from(expect_hash_code),
            String::from(actual_hash_code),
        ))
    }
}

/// Lockfile access service
#[derive(Debug)]
pub struct Lockfile {
    path: PathBuf,
    index: IndexMap<String, Range<usize>>,
}

/// A query function and its ordered executable statements.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LockfileEntry {
    query_finger_print: String,
    query_name: String,
    #[serde(flatten)]
    body: EntryBody,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
enum EntryBody {
    Single(LockfileStatement),
    Multiple(StatementList),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, try_from = "StatementListWire")]
struct StatementList {
    statements: Vec<LockfileStatement>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StatementListWire {
    statements: Vec<LockfileStatement>,
}

impl TryFrom<StatementListWire> for StatementList {
    type Error = &'static str;

    fn try_from(value: StatementListWire) -> Result<Self, Self::Error> {
        if value.statements.len() < 2 {
            return Err("statements requires at least two statements");
        }
        Ok(Self {
            statements: value.statements,
        })
    }
}

impl TryFrom<&[LockfileStatement]> for EntryBody {
    type Error = LockfileError;

    fn try_from(statements: &[LockfileStatement]) -> Result<Self, Self::Error> {
        match statements {
            [] => LockfileError::invalid("a query function requires a statement"),
            [statement] => Ok(Self::Single(statement.clone())),
            _ => Ok(Self::Multiple(StatementList {
                statements: statements.to_vec(),
            })),
        }
    }
}

impl EntryBody {
    fn statements(&self) -> &[LockfileStatement] {
        match self {
            Self::Single(statement) => std::slice::from_ref(statement),
            Self::Multiple(list) => &list.statements,
        }
    }
}

impl LockfileEntry {
    /// Construct a function entry, preserving statement order.
    ///
    /// # Errors
    /// Returns an error when no statements are supplied.
    pub fn new(
        fingerprint: &str,
        name: &str,
        statements: &[LockfileStatement],
    ) -> Result<Self, LockfileError> {
        let body = EntryBody::try_from(statements)?;
        Ok(Self {
            query_finger_print: fingerprint.to_owned(),
            query_name: name.to_owned(),
            body,
        })
    }

    /// Statements in application SQL order.
    #[must_use]
    pub fn statements(&self) -> &[LockfileStatement] {
        self.body.statements()
    }

    /// Require one statement at a connector's execution capability boundary.
    ///
    /// # Errors
    /// Returns an error for a multi-statement query function.
    pub fn single_statement(&self) -> Result<&LockfileStatement, LockfileError> {
        let [statement] = self.statements() else {
            return LockfileError::invalid(&format!(
                "query {} has multiple statements; this connector operation requires a single statement",
                self.query_name
            ));
        };
        Ok(statement)
    }
}

/// SQL and metadata belonging to one statement, without function identity.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockfileStatement {
    execute_sql: String,
    bind_parameters: Vec<QueryParameter>,
    result_set_shape: Vec<ResultSetField>,
}

impl LockfileStatement {
    /// Construct statement metadata without changing binding or column identities.
    #[must_use]
    pub fn new(sql: &str, parameters: &[QueryParameter], columns: &[ResultSetField]) -> Self {
        Self {
            execute_sql: sql.to_owned(),
            bind_parameters: parameters.to_vec(),
            result_set_shape: columns.to_vec(),
        }
    }
    /// Executable SQL returned by analysis.
    #[must_use]
    pub fn execute_sql(&self) -> &str {
        &self.execute_sql
    }
    /// Bind parameters with their actual one-based identities.
    #[must_use]
    pub fn bind_parameters(&self) -> &[QueryParameter] {
        &self.bind_parameters
    }
    /// Output fields in result order.
    #[must_use]
    pub fn result_set_shape(&self) -> &[ResultSetField] {
        &self.result_set_shape
    }
}

/// parameter for query
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QueryParameter {
    name: String,
    parameter_index: u16,
}

impl QueryParameter {
    /// Construct a named SQL binding with its actual one-based identity.
    #[must_use]
    pub fn new(name: &str, parameter_index: u16) -> Self {
        Self {
            name: name.to_owned(),
            parameter_index,
        }
    }
    /// Actual one-based SQL placeholder index.
    #[must_use]
    pub const fn parameter_index(&self) -> u16 {
        self.parameter_index
    }

    /// get name for query parameter
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// field in result set
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResultSetField {
    field_index: u16,
    field_name: String,
    sql_type: String,
}

impl ResultSetField {
    /// Construct an output field with its zero-based ordinal and SQL type name.
    #[must_use]
    pub fn new(index: u16, name: &str, sql_type: &str) -> Self {
        Self {
            field_index: index,
            field_name: name.to_owned(),
            sql_type: sql_type.to_owned(),
        }
    }

    /// field name
    #[must_use]
    pub fn field_name(&self) -> &str {
        &self.field_name
    }

    /// field index
    #[must_use]
    pub const fn field_index(&self) -> u16 {
        self.field_index
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SpannedLockfile {
    sql_fun_lockfile_version: String,
    schema_dump_hash_code: String,

    #[serde(default)]
    entries: Vec<Spanned<LockfileEntry>>,
}

#[derive(Debug, serde::Deserialize)]
struct EntryFragment {
    entries: Vec<LockfileEntry>,
}

impl Lockfile {
    /// create instance
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            index: IndexMap::default(),
        }
    }

    /// load and checks lockfile integrity
    ///
    /// # Errors
    ///
    /// [`LockfileError`]
    #[tracing::instrument(err, skip(path))]
    pub fn from_file<P: AsRef<Path>>(
        path: P,
        schema_dump_hash_code: &str,
    ) -> Result<Self, LockfileError> {
        let content = fs_err::read_to_string(&path)?;
        let spanned: SpannedLockfile = toml::from_str(&content)?;
        LockfileDocument::validate_version(&spanned.sql_fun_lockfile_version)?;
        tracing::info!("lockfile loaded: {} entries", spanned.entries.len());

        if spanned.schema_dump_hash_code != schema_dump_hash_code {
            LockfileError::raise_schema_dump_mismatch(
                &path,
                &spanned.schema_dump_hash_code,
                schema_dump_hash_code,
            )?;
        }
        let mut index = IndexMap::new();

        // Spanned<T> for array-of-tables only covers the table header
        // https://github.com/toml-rs/toml/issues/1206

        let mut entries = spanned.entries.iter().peekable();
        while let Some(entry) = entries.next() {
            let next_start = if let Some(next_entry) = entries.peek() {
                next_entry.span().start
            } else {
                content.len()
            };
            let span = entry.span().start..next_start;
            let item = entry.as_ref();
            index
                .entry(item.query_finger_print.clone())
                .insert_entry(span);
        }
        Ok(Self {
            path: path.as_ref().to_path_buf(),
            index,
        })
    }

    #[tracing::instrument(err, skip(file))]
    fn read_file_span(mut file: File, span: &Range<usize>) -> Result<Vec<u8>, LockfileError> {
        use std::io::Read;

        let len = span.end - span.start;
        file.seek(SeekFrom::Start(span.start as u64))?;
        let mut target_span = file.take(len as u64);

        let mut content = Vec::with_capacity(len);
        target_span.read_to_end(&mut content)?;
        Ok(content)
    }

    /// read specified lockfile entry
    ///
    /// # Errors
    ///
    /// - [`LockfileError`]
    #[tracing::instrument(ret, err, skip(self))]
    pub fn get_entry(
        &self,
        query_finger_print: &str,
    ) -> Result<Option<LockfileEntry>, LockfileError> {
        let Some(span) = self.index.get(&String::from(query_finger_print)) else {
            tracing::info!("entry not found");
            return Ok(None);
        };
        let content = Self::read_file_span(File::open(&self.path)?, span)?;
        let entry_fragment: EntryFragment = toml::from_slice(&content)?;
        let entry = entry_fragment.entries.into_iter().next();
        Ok(entry)
    }
}

/// Complete in-memory lockfile state used for regeneration and validation.
#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockfileDocument {
    sql_fun_lockfile_version: String,
    schema_dump_hash_code: String,
    #[serde(default)]
    entries: Vec<LockfileEntry>,
}

impl LockfileDocument {
    /// Construct an empty prospective document with the current format header.
    #[must_use]
    pub fn new(schema_hash: &str) -> Self {
        Self {
            sql_fun_lockfile_version: "1".to_owned(),
            schema_dump_hash_code: schema_hash.to_owned(),
            entries: Vec::new(),
        }
    }

    fn validate_version(version: &str) -> Result<(), LockfileError> {
        if version != "1" {
            return LockfileError::invalid(&format!("unsupported format version {version}"));
        }
        Ok(())
    }

    /// Load an existing document without requiring the current schema hash.
    /// Only an absent file returns `None`.
    ///
    /// # Errors
    /// Returns read, parse, and unsupported-format errors.
    pub fn load(path: &Path) -> Result<Option<Self>, LockfileError> {
        let content = match fs_err::read_to_string(path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let document: Self = toml::from_str(&content)?;
        Self::validate_version(&document.sql_fun_lockfile_version)?;
        Ok(Some(document))
    }

    /// Check an existing header against the selected schema.
    ///
    /// # Errors
    /// Returns a schema-hash mismatch without changing disk state.
    pub fn validate_schema(&self, path: &Path, hash: &str) -> Result<(), LockfileError> {
        if self.schema_dump_hash_code != hash {
            return LockfileError::raise_schema_dump_mismatch(
                path,
                &self.schema_dump_hash_code,
                hash,
            );
        }
        Ok(())
    }

    /// Add this session's eligible query function.
    pub fn push(&mut self, entry: LockfileEntry) {
        self.entries.push(entry);
    }

    /// Order all parent entries by Rust query function name.
    pub fn sort(&mut self) {
        self.entries
            .sort_by(|left, right| left.query_name.cmp(&right.query_name));
    }

    /// Compare with existing state; absence is equivalent only to zero entries.
    #[must_use]
    pub fn matches_existing(&self, existing: Option<&Self>) -> bool {
        if let Some(existing) = existing {
            self == existing
        } else {
            self.entries.is_empty()
        }
    }

    /// Encode the complete document before touching an existing file.
    ///
    /// # Errors
    /// Returns serialization failures.
    pub fn encode(&self) -> Result<String, LockfileError> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Delete and recreate a complete pre-serialized lockfile.
    ///
    /// # Errors
    /// Returns filesystem errors; this operation does not promise rollback.
    pub fn replace(path: &Path, content: &str) -> Result<(), LockfileError> {
        match fs_err::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        fs_err::write(path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod test_document {
    use super::{
        Lockfile, LockfileDocument, LockfileEntry,
        test_helpers::{Fixture, entry},
    };

    #[rstest::rstest]
    #[case::beginning(0)]
    #[case::middle(1)]
    #[case::end(2)]
    fn indexed_reader_recovers_nested_statements_without_adjacent_entries(#[case] multi: usize) {
        let fixture = Fixture::new();
        let mut document = LockfileDocument::new("hash");
        for (index, name) in ["a", "b", "c"].into_iter().enumerate() {
            document.push(entry(name, if index == multi { 2 } else { 1 }));
        }
        let content = fixture.write(&document);
        assert_eq!(content.matches("[[entries.statements]]").count(), 2);
        let reader = Lockfile::from_file(fixture.path(), "hash").unwrap();
        for (index, name) in ["a", "b", "c"].into_iter().enumerate() {
            let restored = reader.get_entry(name).unwrap().unwrap();
            assert_eq!(restored, entry(name, if index == multi { 2 } else { 1 }));
        }
        assert!(reader.get_entry("absent").unwrap().is_none());
        assert_eq!(
            LockfileDocument::load(&fixture.path()).unwrap().unwrap(),
            document
        );
    }

    #[rstest::rstest]
    #[case::absent(None, true)]
    #[case::empty(Some(0), true)]
    #[case::stale(Some(1), false)]
    fn validates_empty_prospective_state_without_writing(
        #[case] old_count: Option<usize>,
        #[case] matches: bool,
    ) {
        let fixture = Fixture::new();
        let prospective = LockfileDocument::new("hash");
        if let Some(count) = old_count {
            let mut previous = LockfileDocument::new("hash");
            if count != 0 {
                previous.push(entry("removed", 1));
            }
            fixture.write(&previous);
        }
        let previous_bytes = std::fs::read(fixture.path()).ok();
        let previous = LockfileDocument::load(&fixture.path()).unwrap();
        assert_eq!(prospective.matches_existing(previous.as_ref()), matches);
        assert_eq!(std::fs::read(fixture.path()).ok(), previous_bytes);
    }

    #[rstest::rstest]
    #[case::old_hash(
        "schema_dump_hash_code = 'old'\nsql_fun_lockfile_version = '1'\nentries = []",
        false
    )]
    #[case::old_version(
        "schema_dump_hash_code = 'hash'\nsql_fun_lockfile_version = '2'\nentries = []",
        true
    )]
    #[case::missing_header("entries = []", true)]
    #[case::malformed("invalid = [", true)]
    fn existing_empty_files_still_require_valid_headers(
        #[case] content: &str,
        #[case] load_fails: bool,
    ) {
        let fixture = Fixture::new();
        fixture.write_raw(content);
        let loaded = LockfileDocument::load(&fixture.path());
        assert_eq!(loaded.is_err(), load_fails);
        if let Ok(Some(document)) = loaded {
            assert!(!LockfileDocument::new("hash").matches_existing(Some(&document)));
            assert!(document.validate_schema(&fixture.path(), "hash").is_err());
        }
        assert_eq!(fixture.content(), content);
    }

    #[test]
    fn read_errors_are_not_treated_as_absence() {
        let directory = ::tempfile::tempdir().unwrap();
        assert!(LockfileDocument::load(directory.path()).is_err());
    }

    #[test]
    fn regenerates_sorted_complete_inventory() {
        let fixture = Fixture::new();
        let mut old = LockfileDocument::new("hash");
        old.push(entry("removed", 1));
        fixture.write(&old);
        let mut document = LockfileDocument::new("hash");
        document.push(entry("z", 2));
        document.push(entry("a", 1));
        document.sort();
        let content = fixture.write(&document);
        assert!(
            content.find("query_name = \"a\"").unwrap()
                < content.find("query_name = \"z\"").unwrap()
        );
        assert!(!content.contains("removed"));
        let reader = Lockfile::from_file(fixture.path(), "hash").unwrap();
        assert!(reader.get_entry("removed").unwrap().is_none());
    }

    #[test]
    fn rejects_mixed_and_degenerate_statement_shapes() {
        let mut value = ::serde_json::to_value(entry("query", 1)).unwrap();
        assert!(value.get("statements").is_none());
        value["statements"] = ::serde_json::to_value(entry("query", 2).statements()).unwrap();
        assert!(::serde_json::from_value::<LockfileEntry>(value).is_err());
        for count in [0, 1] {
            let value = ::serde_json::json!({"query_finger_print":"q", "query_name":"q", "statements":vec![entry("q", 1).statements()[0].clone(); count]});
            assert!(::serde_json::from_value::<LockfileEntry>(value).is_err());
        }
    }
}
