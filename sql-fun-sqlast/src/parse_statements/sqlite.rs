use crate::StringSpan;
use crate::ast::{Dialect, Statement, StatementNode};
use crate::parse_statements::ParseStatementError;
use crate::sqlite::SqliteStatement;
use crate::sqlite_parser::Parser;
use crate::syn::sqlite::Cmd;

pub(super) fn parse(
    sql: &str,
) -> impl Iterator<Item = Result<Statement, ParseStatementError>> + '_ {
    SqliteStatements {
        parser: Parser::with_type_spans(sql.as_bytes()),
        input: sql,
        finished: false,
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
                if let Some(label) =
                    ::miette::Diagnostic::labels(&error).and_then(|mut labels| labels.next())
                {
                    failure = failure.with_position(StringSpan::new(label.offset(), label.len()));
                }
                return Some(Err(failure));
            }
        };
        // Drain per-statement capture even if its common conversion fails or
        // EXPLAIN is rejected, so following statements cannot inherit events.
        let type_spans = self.parser.take_type_spans();
        let span = parsed.span();
        let Some(statement) = parsed.command().into_plain_statement() else {
            return Some(Err(ParseStatementError::unsupported(
                span,
                "EXPLAIN is not supported by the common AST",
            )));
        };
        Some(
            StatementNode::try_from(&statement)
                .and_then(|node| {
                    let mut statement = Statement::from_node(node, Dialect::sqlite(), span);
                    statement.retain_sqlite_type_syntax(self.input, &type_spans)?;
                    Ok(statement)
                })
                .map_err(|error| ParseStatementError::conversion(span, &error)),
        )
    }
}

impl std::iter::FusedIterator for SqliteStatements<'_> {}
