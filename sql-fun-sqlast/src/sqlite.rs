//! SQLite parser and lexer integration for this crate.
//!
//! This module normalizes `turso_parser` tokens into crate-owned lexical tokens
//! with stable text/span access similar to the PostgreSQL path, and provides the
//! entry point from parser output into this crate's SQLite syntax and semantic
//! analysis layers.

use std::{ops::Range, path::Path};

use ::sql_fun_core::IVec;
use ::turso_parser::{error::Error as TursoError, lexer::Token, parser::Parser as TursoParser};

use crate::syn::sqlite::Cmd;

use crate::{
    AstAndContextPair, ParseDumpFileError, ParsedSchemaDump, StringSpan,
    sem::{AnalysisError, FrozenBaseContext, ParseContext, SchemaFileContext, SemAst},
};

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
    ) -> Result<ParsedSchemaDump, ParseDumpFileError> {
        *content_hash = None;
        let sql_statements = std::fs::read_to_string(dump_file)?;
        let mut hasher = ::sql_fun_core::Hasher::new();
        hasher.append_str(&sql_statements);
        *content_hash = Some(hasher.finish());

        let mut parser = TursoParser::new(sql_statements.as_bytes());
        let mut context = SchemaFileContext::new(dump_file, input_context);
        let mut sem_results = Vec::new();
        while let Some(statement) = Self::next_statement(&mut parser)? {
            let statement_span = StringSpan::new(statement.offset, statement.len);
            let Some(statement_text) = statement_span.get_from(&sql_statements) else {
                AnalysisError::raise_unexpected_none("statement_span not in sql")?;
                AnalysisError::never()
            };
            context.set_statement_span(statement_span);
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
    use crate::SqliteDialect;

    #[rstest::rstest]
    #[case::success("CREATE TABLE hash_test (id);", true)]
    #[case::failure("CREATE TABLE broken (id", false)]
    fn preserves_original_hash_even_on_syntax_failure(#[case] sql: &str, #[case] succeeds: bool) {
        let file = test_helpers::schema_file(sql);
        let base = test_helpers::base_with_public_schema();
        let mut hash = None;
        let result = SqliteDialect::parse_schema_file(file.path(), &base, &mut hash);
        assert_eq!(result.is_ok(), succeeds, "{result:?}");
        assert_eq!(
            hash,
            Some(::sql_fun_core::Hasher::hash_file(file.path()).unwrap())
        );
    }

    #[test]
    fn read_failure_clears_output_hash() {
        let directory = ::tempfile::tempdir().unwrap();
        let base = test_helpers::base_with_public_schema();
        let mut hash = Some("previous".to_owned());
        let error = SqliteDialect::parse_schema_file(
            &directory.path().join("missing.sql"),
            &base,
            &mut hash,
        )
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

#[cfg(test)]
mod test_parser_upgrade_boundary {
    use super::{SqliteDialect, SqliteLexer};
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
    use super::SqliteDialect;
    use ::turso_parser::MAX_EXPR_DEPTH;

    #[test]
    fn propagates_tree_height_limit_for_left_associative_expressions() {
        assert_eq!(MAX_EXPR_DEPTH, 100);
        let accepted = format!("SELECT {}", vec!["1"; MAX_EXPR_DEPTH].join(" + "));
        assert!(SqliteDialect::parse(&accepted).unwrap().is_some());
        let rejected = format!("{accepted} + 1");
        let error = SqliteDialect::parse(&rejected).unwrap_err();
        assert!(error.to_string().contains("maximum depth 100"));
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
        let error = SqliteDialect::parse(&rejected).unwrap_err();
        assert!(error.to_string().contains("maximum depth 100"));
    }
}
