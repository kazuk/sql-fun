//! SQLite parser and lexer integration for this crate.
//!
//! This module normalizes `turso_parser` tokens into crate-owned lexical tokens
//! with stable text/span access similar to the PostgreSQL path, and provides the
//! entry point from parser output into this crate's SQLite syntax and semantic
//! analysis layers.

use crate::{SqlFileSnapshot, ast::Dialect as AstDialect};

use crate::{
    AstAndContextPair, ParseDumpFileError, ParsedSchemaDump, StringSpan,
    context::{FrozenBaseContext, ParseContext, SchemaFileContext},
    sem::{AnalysisError, SemAst},
    syn::sqlite::Cmd,
};
use ::sql_fun_core::IVec;
use ::turso_parser::{error::Error as TursoError, lexer::Token, parser::Parser as TursoParser};
use std::{ops::Range, path::Path};

/// dialect for SQLite
pub struct SqliteDialect;

#[derive(Debug)]
/// SQLite token
pub struct SqliteToken<'a> {
    token: Token<'a>,
    span: std::ops::Range<usize>,
}

impl<'a> SqliteToken<'a> {
    /// new instance
    pub const fn new(token: Token<'a>, span: std::ops::Range<usize>) -> Self {
        Self { token, span }
    }

    /// get token text as UTF-8
    pub fn text(&self) -> String {
        self.token.to_utf8()
    }

    /// token type
    pub const fn token_type(&self) -> turso_parser::token::TokenType {
        self.token.token_type
    }

    /// token span
    pub const fn span(&self) -> &Range<usize> {
        &self.span
    }
}

/// SQLite syntax lexer
pub struct SqliteLexer<'a> {
    lexer: turso_parser::lexer::Lexer<'a>,
    input: &'a str,
}

#[cfg(feature = "sqlite")]
impl<'a> SqliteLexer<'a> {
    /// initlialize lexer with input string
    pub const fn new(input: &'a str) -> Self {
        Self {
            lexer: turso_parser::lexer::Lexer::new(input.as_bytes()),
            input,
        }
    }

    fn offset(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        let base = haystack.as_ptr() as usize;
        let ptr = needle.as_ptr() as usize;

        if ptr >= base && ptr <= base + haystack.len() {
            Some(ptr - base)
        } else {
            None // needleがhaystack由来ではない
        }
    }

    /// get a token
    ///
    /// # Error
    ///
    /// [`TursoError`] for lexical error
    #[expect(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<SqliteToken<'a>>, TursoError> {
        let Some(token) = self.lexer.next().transpose()? else {
            return Ok(None);
        };

        let input_view = self.input.as_bytes();
        let Some(start_offset) = Self::offset(input_view, token.value) else {
            return Err(TursoError::Custom(String::from(
                "unexpected: token not in input",
            )));
        };
        let end_offset = start_offset + token.value.len();
        Ok(Some(SqliteToken::new(token, start_offset..end_offset)))
    }
}

pub struct SqliteStatement {
    cmd: Cmd,
    offset: usize,
    len: usize,
}

impl SqliteStatement {
    pub const fn new(cmd: Cmd, offset: usize, len: usize) -> Self {
        Self { cmd, offset, len }
    }

    pub(crate) const fn span(&self) -> StringSpan {
        StringSpan::new(self.offset, self.len)
    }

    pub(crate) fn command(self) -> Cmd {
        self.cmd
    }
}

impl SqliteDialect {
    /// parse SQL Statement
    pub fn parse(sql: &str) -> Result<Option<Cmd>, TursoError> {
        let mut parser = TursoParser::new(sql.as_bytes());
        Ok(parser.next_cmd()?.map(Cmd::from))
    }

    /// scan a SQL Statement into lex tokens
    pub fn scan<'a>(sql: &'a str) -> Result<IVec<SqliteToken<'a>>, TursoError> {
        let mut lexer = SqliteLexer::new(sql);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next()? {
            tokens.push(token)
        }
        Ok(tokens.into())
    }

    pub(crate) fn next_statement(
        parser: &mut TursoParser<'_>,
    ) -> Result<Option<SqliteStatement>, TursoError> {
        let start_offset = parser.offset();
        let Some(cmd) = parser.next().transpose()? else {
            return Ok(None);
        };
        let len = parser.offset() - start_offset;
        Ok(Some(SqliteStatement::new(
            Cmd::from(cmd),
            start_offset,
            len,
        )))
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
        let sql_statements = snapshot.text();
        let dump_file = snapshot.path();

        let mut parser = TursoParser::new(sql_statements.as_bytes());
        let mut context = SchemaFileContext::new(dump_file, input_context, args, metadata);
        AnalysisError::check_dialect(AstDialect::sqlite(), context.dialect())?;
        let mut sem_results = Vec::new();
        while let Some(statement) = Self::next_statement(&mut parser)? {
            let statement_span = StringSpan::new(statement.offset, statement.len);
            let Some(statement_text) = statement_span.get_from(sql_statements) else {
                AnalysisError::raise_unexpected_none("statement_span not in sql")?;
                AnalysisError::never()
            };
            context.set_statement_span(statement_span);
            context.set_diagnostic_source(statement_text);
            let syn = statement.cmd;
            let tokens = Self::scan(statement_text)?;
            let AstAndContextPair(sem, rcontext) =
                SemAst::semantic_analysis_sqlite(context, syn, &tokens)?;
            sem_results.push(sem);
            context = rcontext;
        }

        Ok(context.into_parsed(sem_results))
    }
}

#[cfg(test)]
mod test_schema_content_hash {
    #[cfg(feature = "sqlite")]
    use crate::SqliteDialect;
    use crate::ast::Dialect;
    use crate::test_helpers::schema_content_hash as test_helpers;
    use crate::{
        SqlFileSnapshot, ast::Dialect as AstDialect,
        context::BaseParseContext as ContextBaseParseContext,
        test_helpers::TestParseContext as TestHelpersTestParseContext,
    };

    #[rstest::rstest]
    #[case::success("CREATE TABLE hash_test (id);", true)]
    #[case::failure("CREATE TABLE broken (id", false)]
    fn preserves_original_hash_even_on_syntax_failure(#[case] sql: &str, #[case] succeeds: bool) {
        let file = test_helpers::schema_file(sql);
        let base = test_helpers::base_with_public_schema(Dialect::sqlite());
        let mut hash = None;
        let result = {
            let analysis_base = &base;
            let environment =
                TestHelpersTestParseContext::default().with_dialect(AstDialect::sqlite());
            SqliteDialect::parse_schema_file(
                file.path(),
                analysis_base,
                &mut hash,
                ContextBaseParseContext::get_args(&environment),
                ContextBaseParseContext::metadata(&environment),
            )
        };
        assert_eq!(result.is_ok(), succeeds, "{result:?}");
        assert_eq!(
            hash,
            Some(::sql_fun_core::Hasher::hash_file(file.path()).unwrap())
        );
    }

    #[test]
    fn analyzes_snapshot_after_source_is_deleted() {
        let file = test_helpers::schema_file("CREATE TABLE retained (id);");
        let snapshot = SqlFileSnapshot::read(file.path()).unwrap();
        let base = test_helpers::base_with_public_schema(Dialect::sqlite());
        std::fs::remove_file(file.path()).unwrap();
        assert!(
            {
                let analysis_base = &base;
                let environment = crate::test_helpers::TestParseContext::default()
                    .with_dialect(crate::ast::Dialect::sqlite());
                SqliteDialect::parse_schema_snapshot(
                    &snapshot,
                    analysis_base,
                    crate::context::BaseParseContext::get_args(&environment),
                    crate::context::BaseParseContext::metadata(&environment),
                )
            }
            .is_ok()
        );
    }

    #[test]
    fn read_failure_clears_output_hash() {
        let directory = ::tempfile::tempdir().unwrap();
        let base = test_helpers::base_with_public_schema(Dialect::sqlite());
        let mut hash = Some("previous".to_owned());
        let error = {
            let analysis_base = &base;
            let environment =
                TestHelpersTestParseContext::default().with_dialect(AstDialect::sqlite());
            SqliteDialect::parse_schema_file(
                &directory.path().join("missing.sql"),
                analysis_base,
                &mut hash,
                ContextBaseParseContext::get_args(&environment),
                ContextBaseParseContext::metadata(&environment),
            )
        }
        .unwrap_err();
        assert!(error.is_io());
        assert!(hash.is_none());
    }
}

#[cfg(test)]
mod test_parser_upgrade_boundary {
    #[cfg(feature = "sqlite")]
    use crate::sqlite::{SqliteDialect, SqliteLexer};
    use ::turso_parser::parser::Parser;

    #[rstest::rstest]
    #[case::update_order_by("UPDATE items SET value = 1 ORDER BY value")]
    #[case::update_limit("UPDATE items SET value = 1 LIMIT 1")]
    #[case::update_order_by_limit("UPDATE items SET value = 1 ORDER BY value LIMIT 1")]
    #[case::delete_order_by("DELETE FROM items ORDER BY value")]
    #[case::delete_limit("DELETE FROM items LIMIT 1")]
    #[case::delete_order_by_limit("DELETE FROM items ORDER BY value LIMIT 1")]
    #[case::generated_default("CREATE TABLE items (base INT, derived INT AS (base) DEFAULT 1)")]
    #[case::only_generated_column("CREATE TABLE items (derived INT AS (1))")]
    #[case::compound_order_by("SELECT 1 ORDER BY 1 UNION SELECT 2")]
    #[case::compound_limit("SELECT 1 LIMIT 1 UNION SELECT 2")]
    fn propagates_upstream_rejections_without_replacing_diagnostics(#[case] sql: &str) {
        let expected = Parser::new(sql.as_bytes())
            .next_cmd()
            .expect_err("upstream rejection");
        let actual = SqliteDialect::parse(sql).expect_err("inherited rejection");
        assert_eq!(actual.to_string(), expected.to_string(), "{sql}");
    }

    #[rstest::rstest]
    fn blob_tokens_retain_complete_text_and_source_span(
        #[values("x'ab'", "X'AB'", "x'aBcD'", "x''")] literal: &str,
    ) {
        let sql = format!("  {literal}");
        let mut lexer = SqliteLexer::new(&sql);
        let whitespace = lexer.next().unwrap().unwrap();
        assert_eq!(whitespace.text(), "  ");
        assert_eq!(*whitespace.span(), 0..2);
        let token = lexer.next().unwrap().unwrap();
        assert_eq!(token.text(), literal);
        assert_eq!(*token.span(), 2..sql.len());
        assert!(lexer.next().unwrap().is_none());
    }
}

#[cfg(test)]
mod test_expression_depth {
    #[cfg(feature = "sqlite")]
    use crate::sqlite::SqliteDialect;
    use ::turso_parser::MAX_EXPR_DEPTH;

    #[test]
    fn propagates_tree_height_limit_for_left_associative_expressions() {
        let accepted = format!("SELECT {}", vec!["1"; MAX_EXPR_DEPTH].join(" + "));
        assert!(SqliteDialect::parse(&accepted).unwrap().is_some());
        let rejected = format!("{accepted} + 1");
        assert!(SqliteDialect::parse(&rejected).is_err());
    }

    #[test]
    fn propagates_nested_expression_limit() {
        // Combine nesting and a long tree without exhausting the debug test thread's stack.
        let accepted = format!(
            "SELECT abs(abs({}))",
            vec!["1"; MAX_EXPR_DEPTH - 2].join(" + ")
        );
        assert!(SqliteDialect::parse(&accepted).unwrap().is_some());
        let rejected = format!(
            "SELECT abs(abs({}))",
            vec!["1"; MAX_EXPR_DEPTH - 1].join(" + ")
        );
        assert!(SqliteDialect::parse(&rejected).is_err());
    }
}
