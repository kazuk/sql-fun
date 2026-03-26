#![deny(missing_docs)]
#![allow(clippy::pedantic)]
#![doc = include_str!("lib.md")]

pub mod sem;
pub mod syn;

mod schema_dump_file;
mod sql_file;
#[cfg(feature = "sqlite")]
mod sqlite;
mod trie_map;

use miette::LabeledSpan;
use sql_fun_core::IVec;

use self::sql_file::read_sql_file;

#[cfg(feature = "sqlite")]
pub use self::sqlite::{SqliteDialect, SqliteLexer, SqliteToken};

pub use self::trie_map::TrieMap;

use std::{io::SeekFrom, path::Path, string::String as StdString};

use crate::{
    sem::{BaseParseContext, ParseContext, SchemaFileContext, SemAst},
    syn::ParseResult,
};

pub use self::schema_dump_file::{ParseDumpFileError, ParsedSchemaDump};
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

    /// load schema dump file
    ///
    /// # Errors
    ///
    /// - [`ParseDumpFileError`]
    ///
    pub fn parse_schema_file<TContext>(
        dump_file: &Path,
        input_context: TContext,
    ) -> Result<ParsedSchemaDump<TContext>, ParseDumpFileError>
    where
        TContext: BaseParseContext + std::fmt::Debug,
    {
        let sql_statements = read_sql_file(dump_file)?;

        let statements = ::pg_query::split_with_parser(&sql_statements)?;
        let mut context = SchemaFileContext::new(dump_file, input_context);
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
        Ok(ParsedSchemaDump::new(sem_results, context))
    }
}

/// parse SQL statement
///
/// # Errors
///
/// - [`pg_query::Error`] : syntax error
///
#[deprecated = "use PostgresDialect::parse"]
pub fn parse(sql: &str) -> Result<ParseResult, pg_query::Error> {
    PostgresDialect::parse(sql)
}

/// scan SQL Statement
#[deprecated = "use PostgresDialect::scan"]
pub fn scan(sql: &str) -> Result<IVec<crate::syn::ScanToken>, pg_query::Error> {
    PostgresDialect::scan(sql)
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
    /// create a new instance
    pub fn new(offset: usize, len: usize) -> Self {
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

    use std::path::PathBuf;

    use crate::{
        PostgresDialect,
        sem::{BaseContext, BaseParseContext},
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

    #[ignore]
    #[rstest::rstest]
    fn parse_adventure_works_schema(
        context_args: SqlFunArgs,
        _enable_stack_overflow_backtrace: EnableStackOverflowBacktrace,
    ) -> TestResult {
        let mut analyze_context = BaseContext::new(context_args.clone())?;

        let builtin_context = BaseContext::new(context_args.clone())?;
        let home_path = PathBuf::from(env!("SQL_FUN_HOME"));
        let builtin = home_path.join("postgres/17/schema.sql");
        let builtin = PostgresDialect::parse_schema_file(&builtin, builtin_context)?;
        analyze_context.extend(Box::new(builtin));

        let tablefunc_ext = home_path.join("postgres/17/extension/tablefunc--1.0.sql");
        let input_context = BaseContext::new(context_args.clone())?;
        let tablefunc_ext = PostgresDialect::parse_schema_file(&tablefunc_ext, input_context)?;

        analyze_context.extend(Box::new(tablefunc_ext));
        let file = PathBuf::from("../examples/adventure-works/schema.develop.sql");
        let _dump = PostgresDialect::parse_schema_file(&file, analyze_context)?;
        Ok(())
    }
}
