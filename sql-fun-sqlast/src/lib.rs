#![deny(missing_docs)]
#![allow(clippy::pedantic)]
#![cfg_attr(
    test,
    allow(
        clippy::items_after_test_module,
        reason = "The repository coding guideline requires tests immediately after the implementation they cover"
    )
)]
#![doc = include_str!("lib.md")]

pub mod ast;
pub mod engine_specific;
pub mod sem;
pub mod syn;
pub mod types;

mod identifier;
mod parse_statements;
mod placeholdered_sql;
mod schema_dump_file;
mod sql_file;
#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(all(feature = "sqlite", feature = "workaround_turso_9791"))]
mod sqlite_parser;
mod trie_map;

use miette::LabeledSpan;
use sql_fun_core::IVec;

pub use self::sql_file::SqlFileSnapshot;

#[cfg(feature = "sqlite")]
pub use self::sqlite::{SqliteDialect, SqliteLexer, SqliteToken};

use std::{io::SeekFrom, path::Path, string::String as StdString};

use crate::{
    sem::{FrozenBaseContext, ParseContext, SchemaFileContext, SemAst},
    syn::ParseResult,
};

pub use self::{
    parse_statements::{ParseStatementError, parse_statements},
    placeholdered_sql::PlaceholderedSqlRenderError,
    schema_dump_file::{ParseDumpFileError, ParsedSchemaDump},
    trie_map::TrieMap,
};
pub use sql_fun_core::{SqlFunArgs, SqlFunArgsBuilder, SqlFunArgsError};

/// Semantic AST and parse context result
pub struct AstAndContextPair<TParseContext>(SemAst, TParseContext)
where
    TParseContext: ParseContext;

impl<TParseContext> AstAndContextPair<TParseContext>
where
    TParseContext: ParseContext,
{
    /// create instance
    pub const fn new(ast: SemAst, context: TParseContext) -> Self {
        Self(ast, context)
    }

    /// Analyzed statement produced by the selected parse context.
    #[must_use]
    pub const fn ast(&self) -> &SemAst {
        &self.0
    }

    /// Consume the analysis result and continue with its updated catalog context.
    pub fn into_context(self) -> TParseContext {
        self.1
    }

    /// Context containing diagnostics and catalog updates from analysis.
    #[must_use]
    pub const fn context(&self) -> &TParseContext {
        &self.1
    }
}

/// dialect for PostgreSQL
pub struct PostgresDialect;

impl PostgresDialect {
    /// parse SQL statement
    ///
    /// # Errors
    ///
    /// - [`pg_query::Error`] : syntax error
    ///
    pub fn parse(sql: &str) -> Result<ParseResult, pg_query::Error> {
        let parse_result = ::pg_query::parse(sql)?;
        Ok(ParseResult::from(parse_result.protobuf))
    }

    /// scan SQL Statement
    pub fn scan(sql: &str) -> Result<IVec<crate::syn::ScanToken>, pg_query::Error> {
        let scan_result = ::pg_query::scan(sql)?;
        let mut tokens = Vec::new();
        for token in scan_result.tokens {
            tokens.push(crate::syn::ScanToken::from(token));
        }
        Ok(tokens.into())
    }

    /// Load a schema file as an ordered layer over the supplied frozen base.
    ///
    /// Clears `content_hash` before reading, then sets the SHA-256 of the
    /// original UTF-8 file text before parsing (before psql preprocessing for
    /// PostgreSQL). The hash remains available on syntax/semantic failure;
    /// failed reads leave it unset. The file is never reopened to hash it.
    ///
    /// `args` and `metadata` explicitly select the current analysis environment.
    /// Configuration retained by `input_context` is historical only. The selected
    /// metadata dialect must match this loader.
    ///
    /// The base and its siblings remain unchanged. Successful results retain the
    /// file's semantic statements and recoverable problems; their frozen base
    /// can be passed to the next file without retaining those statements.
    ///
    /// # Errors
    ///
    /// Returns [`ParseDumpFileError`] for file, syntax, or fatal analysis failures.
    /// Earlier successfully loaded bases remain usable after a failure.
    ///
    pub fn parse_schema_file(
        dump_file: &Path,
        input_context: &FrozenBaseContext,
        content_hash: &mut Option<String>,
        args: &::sql_fun_core::SqlFunArgs,
        metadata: &::sql_fun_core::SqlFunMetadata,
    ) -> Result<ParsedSchemaDump, ParseDumpFileError> {
        *content_hash = None;
        let snapshot = SqlFileSnapshot::read(dump_file)?;
        *content_hash = Some(snapshot.content_hash().to_owned());
        Self::parse_schema_snapshot(&snapshot, input_context, args, metadata)
    }

    /// Analyze exactly the previously read snapshot, without reopening its file.
    /// The supplied args and selected metadata configure this analysis independently
    /// of the frozen catalog; metadata must select this loader's dialect.
    ///
    /// # Errors
    /// Returns syntax or fatal semantic analysis failures.
    pub fn parse_schema_snapshot(
        snapshot: &SqlFileSnapshot,
        input_context: &FrozenBaseContext,
        args: &::sql_fun_core::SqlFunArgs,
        metadata: &::sql_fun_core::SqlFunMetadata,
    ) -> Result<ParsedSchemaDump, ParseDumpFileError> {
        let sql_statements = snapshot.postgres_text();
        let dump_file = snapshot.path();

        let statements = self::parse_statements::split_postgres_statements(&sql_statements)?;
        let mut context = SchemaFileContext::new(dump_file, input_context, args, metadata);
        crate::sem::AnalysisError::check_dialect(
            crate::ast::Dialect::postgres(),
            context.dialect(),
        )?;
        let mut sem_results = Vec::new();
        for statement in statements {
            context.set_statement_span(StringSpan::from_str_in_str(&sql_statements, statement));
            let syn = Self::parse(statement)?;
            let tokens = Self::scan(statement)?;
            let AstAndContextPair(sem, rcontext) =
                SemAst::semantic_analysis(context, syn, &tokens)?;
            sem_results.push(sem);
            context = rcontext;
        }
        Ok(context.into_parsed(sem_results))
    }
}

#[cfg(test)]
mod test_schema_content_hash {
    use crate::PostgresDialect;
    use ::sql_fun_core::Hasher;

    #[rstest::rstest]
    #[case::success("CREATE TABLE hash_test (id INTEGER);", true)]
    #[case::syntax_error("CREATE TABLE broken (id", false)]
    #[case::psql(
        "\\restrict token\r\nCREATE TABLE hash_test (id INTEGER);\r\n\\unrestrict token\n",
        true
    )]
    fn hashes_original_text_before_analysis(#[case] sql: &str, #[case] succeeds: bool) {
        let file = test_helpers::schema_file(sql);
        let base = test_helpers::base_with_public_schema();
        let mut hash = Some("previous output".to_owned());
        let result = {
            let analysis_base = &base;
            let environment = crate::test_helpers::TestParseContext::default()
                .with_dialect(crate::ast::Dialect::postgres());
            PostgresDialect::parse_schema_file(
                file.path(),
                analysis_base,
                &mut hash,
                crate::sem::BaseParseContext::get_args(&environment),
                crate::sem::BaseParseContext::metadata(&environment),
            )
        };
        assert_eq!(result.is_ok(), succeeds, "{result:?}");
        assert_eq!(hash, Some(Hasher::hash_file(file.path()).unwrap()));
    }

    #[test]
    fn failed_read_clears_previous_hash() {
        let directory = ::tempfile::tempdir().unwrap();
        let base = test_helpers::base_with_public_schema();
        let mut hash = Some("previous output".to_owned());
        let error = {
            let analysis_base = &base;
            let environment = crate::test_helpers::TestParseContext::default()
                .with_dialect(crate::ast::Dialect::postgres());
            PostgresDialect::parse_schema_file(
                &directory.path().join("missing.sql"),
                analysis_base,
                &mut hash,
                crate::sem::BaseParseContext::get_args(&environment),
                crate::sem::BaseParseContext::metadata(&environment),
            )
        }
        .unwrap_err();
        assert!(error.is_io());
        assert!(hash.is_none());
    }

    mod test_helpers {
        pub(super) fn base_with_public_schema() -> crate::sem::FrozenBaseContext {
            use crate::sem::{ParseContext, SchemaName};
            let mut base = crate::test_helpers::TestParseContext::default();
            base.set_get_search_path_result(&[SchemaName::from("public")]);
            base.freeze()
        }

        pub(super) fn schema_file(sql: &str) -> ::tempfile::NamedTempFile {
            let file = ::tempfile::NamedTempFile::new().unwrap();
            std::fs::write(file.path(), sql).unwrap();
            file
        }
    }
}

// TODO(msrv >= 1.98): replace with str::substr_range
fn offset_in_string(haystack: &StdString, needle: &str) -> Option<usize> {
    let base = haystack.as_ptr() as usize;
    let ptr = needle.as_ptr() as usize;

    if ptr >= base && ptr <= base + haystack.len() {
        Some(ptr - base)
    } else {
        None // needle not in haystack
    }
}

///  offset and length for SQL statement
#[derive(Debug, Clone, Copy, Default)]
pub struct StringSpan {
    offset: usize,
    len: usize,
}

impl StringSpan {
    /// Byte offset retained when this span was created.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// create a new instance
    pub const fn new(offset: usize, len: usize) -> Self {
        Self { offset, len }
    }

    /// get span text from source
    pub fn get_from<'a>(&self, source: &'a str) -> Option<&'a str> {
        if source.len() < self.offset + self.len {
            None
        } else {
            Some(&source[self.offset..][..self.len])
        }
    }

    /// get content span
    ///
    /// # Panics
    ///
    /// content not in container range
    #[must_use]
    pub fn from_str_in_str(container: &StdString, content: &str) -> Self {
        // TODO(msrv >= 1.98): replace with str::substr_range
        let offset = offset_in_string(container, content).expect("content in container");
        let len = content.len();
        Self { offset, len }
    }

    /// seek position for this string span
    pub const fn seek_pos(&self) -> SeekFrom {
        SeekFrom::Start(self.offset as u64)
    }

    /// get length of this span
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Return true when len equals 0
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn new_labeled_span(&self, label: &str) -> LabeledSpan {
        LabeledSpan::new(Some(String::from(label)), self.offset, self.len)
    }

    fn from_scan_token(tok: &crate::syn::ScanToken) -> Self {
        Self {
            offset: tok.get_start() as usize,
            len: (tok.get_end() - tok.get_start()) as usize,
        }
    }

    /// get end position
    pub const fn end_pos(&self) -> usize {
        self.offset + self.len
    }

    /// merge two span
    pub fn extend(&mut self, other: &Self) {
        self.offset = std::cmp::min(self.offset, other.offset);
        self.len = std::cmp::max(self.end_pos(), other.end_pos()) - self.offset;
    }
}

pub mod test_helpers;

#[cfg(test)]
mod tests {

    use crate::{
        PostgresDialect,
        syn::{ListOpt, Opt},
    };

    use clap::Parser;
    use sql_fun_core::SqlFunArgs;
    use testresult::TestResult;

    pub struct EnableStackOverflowBacktrace {}

    #[rstest::fixture]
    pub fn enable_stack_overflow_backtrace() -> EnableStackOverflowBacktrace {
        #[expect(unsafe_code)]
        unsafe {
            backtrace_on_stack_overflow::enable()
        };
        EnableStackOverflowBacktrace {}
    }

    #[rstest::fixture]
    pub fn context_args() -> SqlFunArgs {
        SqlFunArgs::try_parse_from(vec![
            "sqlfun",
            "--metadata-file",
            "sql_fun.metadata.toml",
            "--sql-fun-home",
            env!("SQL_FUN_HOME"),
        ])
        .unwrap()
    }

    #[ctor::ctor(unsafe)]
    fn init_tracing() {
        use tracing_subscriber::{
            EnvFilter,
            fmt::format::{FmtSpan, debug_fn},
        };

        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"));

        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_test_writer()
            .with_span_events(FmtSpan::ACTIVE)
            .fmt_fields(debug_fn(|writer, field, value| {
                if field.name() == "message" {
                    write!(writer, "{value:?}")
                } else {
                    Ok(())
                }
            }))
            .try_init();
    }

    #[test]
    fn test_parse_simple_query() -> TestResult {
        let result = PostgresDialect::parse("select * from users where id=0")?;
        let Some(relname) = result
            .get_stmts()
            .get(0)
            .get_stmt()
            .as_select_stmt()
            .get_from_clause()
            .get(0)
            .as_range_var()
            .get_relname()
        else {
            panic!("select relname failed returns None")
        };

        assert_eq!(&relname, "users");

        Ok(())
    }

    #[test]
    fn test_create_composite_type() -> TestResult {
        let result = PostgresDialect::parse("CREATE TYPE compfoo AS (f1 int, f2 text);")?;
        let stmt = result.get_stmts().get(0).get_stmt();

        let Some(ct) = stmt.as_composite_type_stmt().as_inner() else {
            eprintln!("{stmt:?}");
            panic!();
        };

        eprintln!("{ct:?}");
        Ok(())
    }

    #[test]
    fn test_create_range_type() -> TestResult {
        let result = PostgresDialect::parse(
            "CREATE TYPE float8_range AS RANGE (subtype = float8, subtype_diff = float8mi);",
        )?;
        let stmt = result.get_stmts().get(0).get_stmt();

        let Some(ct) = stmt.as_create_range_stmt().as_inner() else {
            eprintln!("{stmt:?}");
            panic!();
        };

        eprintln!("{ct:?}");
        Ok(())
    }

    #[test]
    fn test_create_base_type() -> TestResult {
        let result = PostgresDialect::parse(
            "CREATE TYPE box (
    INTERNALLENGTH = 16,
    INPUT = my_box_in_function,
    OUTPUT = my_box_out_function
);",
        )?;
        let stmt = result.get_stmts().get(0).get_stmt();

        let Some(ct) = stmt.as_define_stmt().as_inner() else {
            eprintln!("{stmt:?}");
            panic!();
        };

        eprintln!("{ct:?}");
        Ok(())
    }

    #[rstest::rstest]
    fn parse_adventure_works_schema(
        context_args: SqlFunArgs,
        _enable_stack_overflow_backtrace: EnableStackOverflowBacktrace,
    ) -> TestResult {
        crate::test_helpers::small_stack_regression(2 * 1024 * 1024, || {
            let dump = crate::test_helpers::adventure_works_schema(&context_args)
                .expect("full AdventureWorks schema on a small stack");
            let view = crate::sem::BaseParseContext::get_view(
                &dump,
                &crate::sem::ViewName::from_full_name(&crate::sem::FullName::with_schema(
                    "humanresources",
                    "vemployee",
                )),
            )
            .expect("AdventureWorks employee view");
            for name in [
                "businessentityid",
                "title",
                "firstname",
                "middlename",
                "lastname",
                "suffix",
                "jobtitle",
                "phonenumber",
                "phonenumbertype",
                "emailaddress",
                "emailpromotion",
                "addressline1",
                "addressline2",
                "city",
                "stateprovincename",
                "postalcode",
                "countryregionname",
                "additionalcontactinfo",
            ] {
                assert!(
                    view.has_column(&crate::sem::ColumnName::from(name)),
                    "{name}"
                );
            }
            assert_eq!(
                view.get_column_def(&crate::sem::ColumnName::from("businessentityid"))
                    .expect("employee id")
                    .get_type_cloned(),
                Some(crate::sem::PgBuiltInType::int4())
            );
        });
        Ok(())
    }
}
