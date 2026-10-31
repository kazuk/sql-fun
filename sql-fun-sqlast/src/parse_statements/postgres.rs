mod boundaries;

use crate::ast::{Dialect, Statement, StatementNode, TryFromWithTokens};
use crate::parse_statements::ParseStatementError;
use crate::sem::AnalysisError;
use crate::syn::{ListOpt, Opt, ParseResult};
use crate::{PostgresDialect, StringSpan};

use std::vec::IntoIter;

pub(crate) fn split_postgres_statements(sql: &str) -> Result<Vec<&str>, ::pg_query::Error> {
    ::pg_query::split_with_parser(sql)
}

pub(super) fn parse(
    sql: &str,
) -> impl Iterator<Item = Result<Statement, ParseStatementError>> + '_ {
    PostgresStatements {
        sql,
        ranges: self::boundaries::discover(sql).into_iter(),
    }
}

struct PostgresStatements<'a> {
    sql: &'a str,
    ranges: IntoIter<Result<StringSpan, ParseStatementError>>,
}

impl Iterator for PostgresStatements<'_> {
    type Item = Result<Statement, ParseStatementError>;

    fn next(&mut self) -> Option<Self::Item> {
        let range = match self.ranges.next()? {
            Ok(range) => range,
            Err(error) => return Some(Err(error)),
        };
        let sql = range
            .get_from(self.sql)
            .expect("boundary range in original input");
        let statement = parse_statement(sql, range);
        if let Err(error) = &statement
            && !error.is_recoverable()
        {
            self.ranges = Vec::new().into_iter();
        }
        Some(statement)
    }
}

impl std::iter::FusedIterator for PostgresStatements<'_> {}

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
    #[case::unterminated_string("SELECT 1; SELECT 'unfinished")]
    #[case::nul_byte("SELECT 1; SELECT '\0'; SELECT 3;")]
    fn yields_a_single_error_when_syntax_prevents_splitting(#[case] sql: &str) {
        let mut statements = super::parse(sql);
        let error = statements.next().expect("syntax error").unwrap_err();
        assert!(error.is_syntax());
        assert!(error.is_boundary_discovery());
        assert!(!error.is_recoverable());
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
    let parsed = ::pg_query::parse(sql, ::pg_query::ParserOptions::default())
        .map_err(|error| ParseStatementError::syntax(span, &error.to_string()))?;
    if parsed.protobuf.stmts.len() != 1 {
        return Err(ParseStatementError::boundary_discovery(
            span,
            "PostgreSQL candidate does not contain exactly one statement",
        ));
    }
    let parsed = ParseResult::from(parsed.protobuf);
    let tokens = PostgresDialect::scan(sql)
        .map_err(|error| ParseStatementError::syntax(span, &error.to_string()))?;
    let Some(node) = parsed.get_stmts().get(0).get_stmt().as_inner() else {
        return AnalysisError::raise_unexpected_none("single PostgreSQL statement")
            .map_err(|error| ParseStatementError::conversion(span, &error));
    };
    let statement = StatementNode::try_from_with_tokens(&node, &tokens).map_err(|error| {
        let failure = ParseStatementError::conversion(span, &error);
        if node.as_select_stmt().as_inner().is_some() {
            failure.with_statement_kind(crate::ast::StatementKind::query())
        } else if node.as_insert_stmt().as_inner().is_some() {
            failure.with_statement_kind(crate::ast::StatementKind::insert())
        } else if node.as_update_stmt().as_inner().is_some() {
            failure.with_statement_kind(crate::ast::StatementKind::update())
        } else if node.as_delete_stmt().as_inner().is_some() {
            failure.with_statement_kind(crate::ast::StatementKind::delete())
        } else if node.as_merge_stmt().as_inner().is_some() {
            failure.with_statement_kind(crate::ast::StatementKind::merge())
        } else if node.as_truncate_stmt().as_inner().is_some() {
            failure.with_statement_kind(crate::ast::StatementKind::truncate())
        } else {
            failure
        }
    })?;
    Ok(Statement::from_node(statement, Dialect::postgres(), span))
}

#[cfg(test)]
mod test_parse_statement {
    #[rstest::rstest]
    #[case::multiple("SELECT 1; SELECT 2;")]
    #[case::no_statement("/* no statement */")]
    fn rejects_a_range_without_exactly_one_statement(#[case] sql: &str) {
        let error = super::parse_statement(sql, crate::StringSpan::new(0, sql.len())).unwrap_err();
        assert!(error.is_boundary_discovery());
        assert!(!error.is_recoverable());
        assert_eq!(error.span().get_from(sql), Some(sql));
    }
}
