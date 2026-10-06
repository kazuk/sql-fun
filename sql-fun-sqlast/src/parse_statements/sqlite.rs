use crate::StringSpan;
use crate::ast::{Dialect, Statement, StatementNode};
use crate::parse_statements::ParseStatementError;
use crate::sqlite::SqliteDialect;

use ::turso_parser::parser::Parser;

pub(super) fn parse(
    sql: &str,
) -> impl Iterator<Item = Result<Statement, ParseStatementError>> + '_ {
    SqliteStatements {
        parser: Parser::new(sql.as_bytes()),
        input_len: sql.len(),
        finished: false,
    }
}

struct SqliteStatements<'a> {
    parser: Parser<'a>,
    input_len: usize,
    finished: bool,
}

impl Iterator for SqliteStatements<'_> {
    type Item = Result<Statement, ParseStatementError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        let start = self.parser.offset();
        let parsed = match SqliteDialect::next_statement(&mut self.parser) {
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
                    StringSpan::new(start, self.input_len - start),
                    &error.to_string(),
                );
                if let Some(label) =
                    ::miette::Diagnostic::labels(&error).and_then(|mut labels| labels.next())
                {
                    failure = failure.with_position(StringSpan::new(label.offset(), label.len()));
                }
                return Some(Err(failure));
            }
        };
        let span = parsed.span();
        let Some(statement) = parsed.command().into_plain_statement() else {
            return Some(Err(ParseStatementError::unsupported(
                span,
                "EXPLAIN is not supported by the common AST",
            )));
        };
        Some(
            StatementNode::try_from(&statement)
                .map(|node| Statement::from_node(node, Dialect::sqlite()))
                .map_err(|error| ParseStatementError::conversion(span, &error)),
        )
    }
}

impl std::iter::FusedIterator for SqliteStatements<'_> {}
