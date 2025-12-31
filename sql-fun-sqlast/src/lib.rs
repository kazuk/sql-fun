#![deny(missing_docs)]
#![doc = include_str!("lib.md")]

pub mod sem;
pub mod syn;

mod schema_dump_file;
mod sql_file;
mod trie_map;

use self::sql_file::read_sql_file;
pub use self::trie_map::TrieMap;

use std::string::String as StdString;

use crate::{
    sem::{ParseContext, SemAst},
    syn::ParseResult,
};

/// Semantic AST and parse context result
pub struct AstAndContextPair<TParseContext>(SemAst, TParseContext)
where
    TParseContext: ParseContext;

impl<TParseContext> AstAndContextPair<TParseContext>
where
    TParseContext: ParseContext,
{
    /// create instance
    pub fn new(ast: SemAst, context: TParseContext) -> Self {
        Self(ast, context)
    }
}

/// parse SQL statement
pub fn parse(sql: &str) -> Result<ParseResult, pg_query::Error> {
    let parse_result = ::pg_query::parse(sql)?;
    Ok(ParseResult::from(parse_result.protobuf))
}

fn offset_in_string(haystack: &StdString, needle: &str) -> Option<usize> {
    let base = haystack.as_ptr() as usize;
    let ptr = needle.as_ptr() as usize;

    if ptr >= base && ptr <= base + haystack.len() {
        Some(ptr - base)
    } else {
        None // needleがhaystack由来ではない
    }
}

///  offset and length for SQL statement
#[expect(dead_code)]
#[derive(Debug, Clone, Copy, Default)]
pub struct StringSpan {
    offset: usize,
    len: usize,
}

impl StringSpan {
    /// get content span
    pub fn from_str_in_str(container: &StdString, content: &str) -> Self {
        let offset = offset_in_string(container, content).expect("content in container");
        let len = content.len();
        Self { offset, len }
    }
}

pub use self::schema_dump_file::{ParseDumpFileError, ParsedSchemaDump, parse_schema_file};

#[cfg(test)]
pub mod test_helpers;

#[cfg(test)]
mod tests {

    use std::path::PathBuf;

    use crate::{
        sem::{BaseContext, BaseParseContext},
        syn::{ListOpt, Opt},
    };

    use super::{parse, parse_schema_file};
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
            "subcmd",
        ])
        .unwrap()
    }

    #[ctor::ctor]
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
                    use core::fmt::Write as _;
                    write!(writer, "{value:?}")
                } else {
                    Ok(())
                }
            }))
            .try_init();
    }

    #[test]
    fn test_parse_simple_query() -> TestResult {
        let result = parse("select * from users where id=0")?;
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
    fn test_create_composit_type() -> TestResult {
        let result = parse("CREATE TYPE compfoo AS (f1 int, f2 text);")?;
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
        let result = parse(
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
        let result = parse(
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
        let builtin = parse_schema_file(&builtin, builtin_context)?;
        analyze_context.extend(Box::new(builtin));

        let tablefunc_ext = home_path.join("postgres/17/extension/tablefunc--1.0.sql");
        let input_context = BaseContext::new(context_args.clone())?;
        let tablefunc_ext = parse_schema_file(&tablefunc_ext, input_context)?;

        analyze_context.extend(Box::new(tablefunc_ext));
        let file = PathBuf::from("../examples/adventure-works/schema.develop.sql");
        let _dump = parse_schema_file(&file, analyze_context)?;
        Ok(())
    }
}
