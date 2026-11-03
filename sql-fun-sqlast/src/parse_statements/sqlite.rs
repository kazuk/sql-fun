use crate::StringSpan;
use crate::ast::{Dialect, Statement, StatementNode};
use crate::parse_statements::ParseStatementError;
use crate::sqlite::SqliteStatement;
#[cfg(feature = "workaround_turso_9791")]
use crate::sqlite_parser::Parser;
use crate::syn::sqlite::Cmd;
#[cfg(not(feature = "workaround_turso_9791"))]
use ::turso_parser::parser::Parser;

pub(super) fn parse(
    sql: &str,
) -> impl Iterator<Item = Result<Statement, ParseStatementError>> + '_ {
    SqliteStatements {
        #[cfg(feature = "workaround_turso_9791")]
        parser: Parser::with_type_spans(sql.as_bytes()),
        #[cfg(not(feature = "workaround_turso_9791"))]
        parser: Parser::new(sql.as_bytes()),
        input: sql,
        finished: false,
    }
}

#[cfg(all(test, feature = "workaround_turso_9791"))]
mod diagnostic_tests {
    #[test]
    fn unsupported_join_reports_the_sql_word_without_result_debug() {
        let sql = "SELECT 1 FROM t LEFT surprising JOIN u";
        let error = super::parse(sql).next().unwrap().unwrap_err();
        assert_eq!(error.to_string(), "unsupported JOIN type: surprising");
        assert!(error.is_syntax());
        assert!(error.is_boundary_discovery());
        assert!(!error.is_recoverable());
        assert_eq!(error.span().offset(), 0);
        assert_eq!(error.span().len(), sql.len());
    }
}

struct SqliteStatements<'a> {
    parser: Parser<'a>,
    input: &'a str,
    finished: bool,
}

impl SqliteStatements<'_> {
    fn next_statement(&mut self) -> Result<Option<SqliteStatement>, ::turso_parser::error::Error> {
        let start = self.parser.offset();
        let Some(command) = self.parser.next().transpose()? else {
            return Ok(None);
        };
        Ok(Some(SqliteStatement::new(
            Cmd::from(command),
            start,
            self.parser.offset() - start,
        )))
    }
}

impl Iterator for SqliteStatements<'_> {
    type Item = Result<Statement, ParseStatementError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        let start = self.parser.offset();
        let parsed = match self.next_statement() {
            Ok(Some(statement)) => statement,
            Ok(None) => {
                self.finished = true;
                return None;
            }
            Err(error) => {
                self.finished = true;
                // Turso restores its cursor on a parser error. This span identifies
                // the attempted input, not a fabricated location for the bad token.
                let mut failure = ParseStatementError::boundary_discovery(
                    StringSpan::new(start, self.input.len() - start),
                    &error.to_string(),
                );
                if let Some(mut labels) = ::miette::Diagnostic::labels(&error)
                    && let Some(label) = labels.next()
                {
                    failure = failure.with_position(StringSpan::new(label.offset(), label.len()));
                }
                return Some(Err(failure));
            }
        };
        // Drain per-statement capture even if its common conversion fails or
        // EXPLAIN is rejected, so following statements cannot inherit events.
        #[cfg(feature = "workaround_turso_9791")]
        let type_spans = self.parser.take_type_spans();
        let span = parsed.span();
        let Some(statement) = parsed.command().into_plain_statement() else {
            return Some(Err(ParseStatementError::unsupported(
                span,
                "EXPLAIN is not supported by the common AST",
            )));
        };
        let node = match StatementNode::try_from(&statement) {
            Ok(node) => node,
            Err(error) => return Some(Err(ParseStatementError::conversion(span, &error))),
        };
        let statement = Statement::from_node(node, Dialect::sqlite(), span);
        #[cfg(feature = "workaround_turso_9791")]
        let statement = {
            let mut statement = statement;
            if let Err(error) = statement.retain_sqlite_type_syntax(self.input, &type_spans) {
                return Some(Err(ParseStatementError::conversion(span, &error)));
            }
            statement
        };
        Some(Ok(statement))
    }
}

impl std::iter::FusedIterator for SqliteStatements<'_> {}
