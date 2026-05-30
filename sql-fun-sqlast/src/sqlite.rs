//! SQLite parser and lexer integration for this crate.
//!
//! This module normalizes `turso_parser` tokens into crate-owned lexical tokens
//! with stable text/span access similar to the PostgreSQL path, and provides the
//! entry point from parser output into this crate's SQLite syntax and semantic
//! analysis layers.

use std::{ops::Range, path::Path};

use sql_fun_core::IVec;
use turso_parser::{
    ast::Cmd, error::Error as TursoError, lexer::Token, parser::Parser as TursoParser,
};

use crate::{
    AstAndContextPair, ParseDumpFileError, ParsedSchemaDump, StringSpan,
    sem::{AnalysisError, BaseParseContext, SchemaFileContext, SemAst},
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
    pub fn new(token: Token<'a>, span: std::ops::Range<usize>) -> Self {
        Self { token, span }
    }

    /// get token text as UTF-8
    pub fn text(&self) -> String {
        self.token.to_utf8()
    }

    /// token type
    pub fn token_type(&self) -> turso_parser::token::TokenType {
        self.token.token_type
    }

    /// token span
    pub fn span(&self) -> &Range<usize> {
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
    pub fn new(input: &'a str) -> Self {
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
    cmd: turso_parser::ast::Cmd,
    offset: usize,
    len: usize,
}

impl SqliteStatement {
    pub fn new(cmd: turso_parser::ast::Cmd, offset: usize, len: usize) -> Self {
        Self { cmd, offset, len }
    }
}

impl SqliteDialect {
    /// parse SQL Statement
    pub fn parse(sql: &str) -> Result<Option<Cmd>, TursoError> {
        let mut parser = TursoParser::new(sql.as_bytes());
        parser.next_cmd()
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

    fn next_statement(parser: &mut TursoParser<'_>) -> Result<Option<SqliteStatement>, TursoError> {
        let start_offset = parser.offset();
        let Some(cmd) = parser.next().transpose()? else {
            return Ok(None);
        };
        let len = parser.offset() - start_offset;
        Ok(Some(SqliteStatement::new(cmd, start_offset, len)))
    }

    /// parse schema dump file
    pub fn parse_schema_file<TContext>(
        dump_file: &Path,
        input_context: TContext,
    ) -> Result<ParsedSchemaDump<TContext>, ParseDumpFileError>
    where
        TContext: BaseParseContext + std::fmt::Debug,
    {
        let sql_statements = std::fs::read_to_string(dump_file)?;

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

        Ok(ParsedSchemaDump::new(sem_results, context))
    }
}
