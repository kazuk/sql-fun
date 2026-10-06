use crate::ast::{Dialect, Statement, StatementNode, TryFromWithTokens};
use crate::parse_statements::ParseStatementError;
use crate::sem::AnalysisError;
use crate::syn::{ListOpt, Opt};
use crate::{PostgresDialect, StringSpan};

use std::vec::IntoIter;

pub(crate) fn split_postgres_statements(sql: &str) -> Result<Vec<&str>, ::pg_query::Error> {
    ::pg_query::split_with_parser(sql)
}

pub(super) fn parse(
    sql: &str,
) -> impl Iterator<Item = Result<Statement, ParseStatementError>> + '_ {
    let (statements, error) = match split_postgres_statements(sql) {
        Ok(statements) => (statements, None),
        Err(error) => (
            Vec::new(),
            Some(ParseStatementError::syntax(
                StringSpan::new(0, sql.len()),
                &error.to_string(),
            )),
        ),
    };
    PostgresStatements {
        sql,
        statements: statements.into_iter(),
        error,
    }
}

struct PostgresStatements<'a> {
    sql: &'a str,
    statements: IntoIter<&'a str>,
    error: Option<ParseStatementError>,
}

impl Iterator for PostgresStatements<'_> {
    type Item = Result<Statement, ParseStatementError>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(error) = self.error.take() {
            return Some(Err(error));
        }
        let sql = self.statements.next()?;
        // The splitter returns slices of the original input, including leading trivia.
        let offset = sql.as_ptr() as usize - self.sql.as_ptr() as usize;
        let statement = parse_statement(sql, StringSpan::new(offset, sql.len()));
        if let Err(error) = &statement
            && error.is_syntax()
        {
            self.statements = Vec::new().into_iter();
        }
        Some(statement)
    }
}

#[cfg(test)]
mod test_postgres_statements {
    #[rstest::rstest]
    #[case::empty("")]
    #[case::whitespace(" \t\n")]
    #[case::line_comment("-- no statement")]
    #[case::block_comment("/* no statement */")]
    #[case::empty_statements(";;; -- no statement")]
    fn skips_input_without_statements(#[case] sql: &str) {
        assert_eq!(super::parse(sql).count(), 0);
    }

    #[rstest::rstest]
    #[case::string("SELECT 'first;second'; SELECT 2;")]
    #[case::quoted_identifier("SELECT \"semi;colon\"; SELECT 2;")]
    #[case::dollar_quote("SELECT $body$first;second$body$; SELECT 2;")]
    #[case::comment("SELECT 1 /* ; */; -- ;\nSELECT 2;")]
    fn preserves_quoted_and_commented_semicolons(#[case] sql: &str) {
        let mut statements = super::parse(sql);
        assert!(
            statements
                .next()
                .expect("first statement")
                .unwrap()
                .is_query()
        );
        assert!(
            statements
                .next()
                .expect("second statement")
                .unwrap()
                .is_query()
        );
        assert!(statements.next().is_none());
    }

    #[test]
    fn preserves_dollar_quoted_function_body() {
        let sql =
            "CREATE FUNCTION sample() RETURNS integer AS $$ SELECT 1; $$ LANGUAGE SQL; SELECT 2;";
        let mut statements = super::parse(sql);
        assert!(statements.next().expect("function statement").is_ok());
        assert!(
            statements
                .next()
                .expect("query statement")
                .unwrap()
                .is_query()
        );
        assert!(statements.next().is_none());
    }

    #[rstest::rstest]
    #[case::invalid_statement("SELECT 1; SELECT FROM; SELECT 3;")]
    #[case::unterminated_string("SELECT 1; SELECT 'unfinished")]
    #[case::nul_byte("SELECT 1; SELECT '\0'; SELECT 3;")]
    fn yields_a_single_error_when_syntax_prevents_splitting(#[case] sql: &str) {
        let mut statements = super::parse(sql);
        let error = statements.next().expect("syntax error").unwrap_err();
        assert!(error.is_syntax());
        assert_eq!(error.span().get_from(sql), Some(sql));
        assert!(statements.next().is_none());
        assert!(statements.next().is_none());
    }

    #[test]
    fn statements_outlive_the_input_and_iterator() {
        let statement = {
            let sql = String::from("SELECT 'owned text'");
            super::parse(&sql).next().expect("statement").unwrap()
        };
        assert!(statement.is_query());
    }
}

fn parse_statement(sql: &str, span: StringSpan) -> Result<Statement, ParseStatementError> {
    let parsed = PostgresDialect::parse(sql)
        .map_err(|error| ParseStatementError::syntax(span, &error.to_string()))?;
    let tokens = PostgresDialect::scan(sql)
        .map_err(|error| ParseStatementError::syntax(span, &error.to_string()))?;
    let Some(node) = parsed.get_stmts().get(0).get_stmt().as_inner() else {
        return AnalysisError::raise_unexpected_none("single PostgreSQL statement")
            .map_err(|error| ParseStatementError::conversion(span, &error));
    };
    let statement = StatementNode::try_from_with_tokens(&node, &tokens)
        .map_err(|error| ParseStatementError::conversion(span, &error))?;
    Ok(Statement::from_node(statement, Dialect::postgres()))
}
