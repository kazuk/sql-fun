//! Unresolved PostgreSQL access method declarations.

use sql_fun_core::IVec;

use crate::{
    StringSpan,
    ast::{Name, TryFromWithTokens},
    error::AnalysisError,
    syn::{self, Opt, ScanToken, Token},
};

#[allow(
    dead_code,
    reason = "retained for access method semantic analysis (#1223)"
)]
#[derive(Clone, Debug)]
pub(crate) struct CreateAccessMethod {
    name: String,
    kind: AccessMethodKind,
    handler: Name,
    name_span: Option<StringSpan>,
    handler_span: Option<StringSpan>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccessMethodKind {
    Table,
    Index,
}

impl TryFrom<&str> for AccessMethodKind {
    type Error = AnalysisError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        // PostgreSQL pg_am.h AMTYPE_TABLE and AMTYPE_INDEX.
        match value {
            "t" => Ok(Self::Table),
            "i" => Ok(Self::Index),
            _ => AnalysisError::raise_unexpected_input("access method kind"),
        }
    }
}

impl TryFromWithTokens<&syn::CreateAmStmt> for CreateAccessMethod {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::CreateAmStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, Self::Error> {
        // CreateAmStmt has no location fields. The scanner receives exactly this
        // statement, whose TYPE clause separates the two names.
        let tokens: Vec<_> = tokens
            .iter()
            .filter(|token| {
                !matches!(
                    token.get_token().as_inner(),
                    Some(Token::SqlComment | Token::CComment | Token::Ascii59)
                )
            })
            .collect();
        let type_clause = tokens
            .iter()
            .enumerate()
            .skip(4)
            .find(|(_, token)| matches!(token.get_token().as_inner(), Some(Token::TypeP)))
            .map(|(index, _)| index);
        let (name_span, handler_span) = if let Some(index) = type_clause
            && let Some(handler_tokens) = tokens.get(index + 3..)
        {
            (token_range(&tokens[3..index]), token_range(handler_tokens))
        } else {
            (None, None)
        };

        Ok(Self {
            name: value.get_amname(),
            kind: AccessMethodKind::try_from(value.get_amtype().as_str())?,
            handler: Name::try_from(value.get_handler_name())?,
            name_span,
            handler_span,
        })
    }
}

#[cfg(test)]
mod test_create_access_method_conversion {
    use crate::{
        ast::{Dialect, StatementKind, StatementNode, create_access_method::AccessMethodKind},
        parse_statements,
    };

    #[rstest::rstest]
    // bloom--1.0.sql, line 12, in both PostgreSQL 17.6 and 18.1.
    #[case(
        "CREATE ACCESS METHOD bloom TYPE INDEX HANDLER blhandler;",
        AccessMethodKind::Index,
        "bloom",
        &["blhandler"],
        "bloom",
        "blhandler",
    )]
    #[case(
        r#"CREATE ACCESS METHOD /* method */ "App Method"
TYPE TABLE HANDLER "App" /* schema */ . "Table Handler";"#,
        AccessMethodKind::Table,
        "App Method",
        &["App", "Table Handler"],
        r#""App Method""#,
        r#""App" /* schema */ . "Table Handler""#,
    )]
    fn preserves_access_method_declaration(
        #[case] declaration: &str,
        #[case] kind: AccessMethodKind,
        #[case] name: &str,
        #[case] handler: &[&str],
        #[case] written_name: &str,
        #[case] written_handler: &str,
    ) {
        let sql = format!("SELECT 1;\n{declaration}");
        let statement = parse_statements(&sql, Dialect::postgres())
            .nth(1)
            .unwrap()
            .unwrap();
        assert_eq!(statement.kind(), StatementKind::CreateAccessMethod);
        let StatementNode::CreateAccessMethod(method) = statement.node() else {
            panic!("expected CREATE ACCESS METHOD");
        };
        assert_eq!(method.kind, kind);
        assert_eq!(method.name, name);
        assert_eq!(method.handler.0, handler);

        let statement_sql = statement.span().get_from(&sql).unwrap();
        assert!(
            statement_sql
                .trim_start()
                .starts_with("CREATE ACCESS METHOD")
        );
        assert_eq!(
            method.name_span.unwrap().get_from(statement_sql),
            Some(written_name)
        );
        assert_eq!(
            method.handler_span.unwrap().get_from(statement_sql),
            Some(written_handler)
        );
    }
}

fn token_range(tokens: &[&ScanToken]) -> Option<StringSpan> {
    let start = tokens.first()?.get_start() as usize;
    let end = tokens.last()?.get_end() as usize;
    Some(StringSpan::new(start, end - start))
}
