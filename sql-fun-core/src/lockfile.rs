use indexmap::IndexMap;
use serde_spanned::Spanned;
use std::{
    fs::File,
    io::{Seek, SeekFrom},
    ops::Range,
    path::{Path, PathBuf},
};

/// errors related lockfile
#[derive(thiserror::Error, Debug)]
pub enum LockfileError {
    /// IO error
    #[error("IO Error :\n\t{0}")]
    Io(#[from] std::io::Error),

    /// toml encode decode error
    #[error("TOML Error:\n\t{0}")]
    Toml(#[from] toml::de::Error),
}

/// Lockfile access service
#[derive(Debug)]
pub struct Lockfile {
    path: PathBuf,
    index: IndexMap<String, Range<usize>>,
}

/// item of lockfile
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LockfileEntry {
    query_finger_print: String,
    query_name: String,
    execute_sql: String,
    bind_parameters: Vec<QueryParameter>,
    result_set_shape: Vec<ResultSetField>,
}

/// parameter for query
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct QueryParameter {
    name: String,
    parameter_index: u16,
}

/// field in result set
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ResultSetField {
    field_index: u16,
    field_name: String,
    sql_type: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SpannedLockfile {
    sql_fun_lockfile_version: String,
    schema_dump_hash_code: String,

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
    #[tracing::instrument(err, skip(path))]
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, LockfileError> {
        let content = fs_err::read_to_string(&path)?;
        let spanned: SpannedLockfile = toml::from_str(&content)?;
        tracing::info!("lockfile loaded: {} entries", spanned.entries.len());
        let mut index = IndexMap::new();

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
