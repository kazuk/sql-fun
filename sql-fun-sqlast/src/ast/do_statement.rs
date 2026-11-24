//! Preserve PostgreSQL anonymous blocks and their operand locations.

use sql_fun_core::IVec;

use crate::{
    StringSpan,
    ast::{TryFromWithTokens, postgres::required},
    error::AnalysisError,
    syn::{self, ListOpt, Opt, ScanToken, Token},
};

#[derive(Clone, Debug)]
pub(crate) struct DoStatement {
    pub(crate) keyword_span: Option<StringSpan>,
    #[cfg_attr(not(test), allow(dead_code, reason = "retained opaque DO body"))]
    body: Operand,
    #[cfg_attr(not(test), allow(dead_code, reason = "retained DO language clause"))]
    language: Option<Operand>,
}

#[derive(Clone, Debug)]
#[cfg_attr(not(test), allow(dead_code, reason = "retained opaque DO operand"))]
struct Operand {
    text: String,
    span: Option<StringSpan>,
}

impl TryFromWithTokens<&syn::DoStmt> for DoStatement {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        statement: &syn::DoStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, Self::Error> {
        let options = statement
            .get_args()
            .as_inner()
            .unwrap_or_default()
            .iter()
            .map(|node| required(node.as_def_elem().as_inner(), "DO option"))
            .collect::<Result<Vec<_>, _>>()?;
        let mut body = None;
        let mut language = None;
        for (index, option) in options.iter().enumerate() {
            let (target, skip) = match option.get_defname().as_str() {
                "as" => (&mut body, 0),
                "language" => (&mut language, 1),
                _ => return AnalysisError::raise_unexpected_input("DO option"),
            };
            if target.is_some() {
                return AnalysisError::raise_unexpected_input("duplicate DO option");
            }
            let text = required(option.get_arg().as_string().get_sval(), "DO option text")?;
            let end = options.get(index + 1).map(syn::DefElem::get_location);
            *target = Some(Operand {
                text,
                span: operand_span(tokens, option.get_location(), end, skip),
            });
        }
        let keyword_span = tokens
            .iter()
            .find(|token| matches!(token.get_token().as_inner(), Some(Token::Do)))
            .map(|token| {
                StringSpan::new(
                    token.get_start() as usize,
                    (token.get_end() - token.get_start()) as usize,
                )
            });
        Ok(Self {
            keyword_span,
            body: required(body, "DO body")?,
            language,
        })
    }
}

fn operand_span(
    tokens: &IVec<ScanToken>,
    start: i32,
    end: Option<i32>,
    skip: usize,
) -> Option<StringSpan> {
    // DefElem locations start at the body literal or at the LANGUAGE
    // keyword. The next option bounds continued and Unicode literals.
    let mut operand = tokens
        .iter()
        .skip_while(|token| token.get_start() < start)
        .take_while(|token| end.is_none_or(|end| token.get_start() < end))
        .filter(|token| {
            !matches!(
                token.get_token().as_inner(),
                Some(Token::SqlComment | Token::CComment | Token::Ascii59)
            )
        })
        .skip(skip);
    let first = operand.next()?;
    let last = operand.last().unwrap_or(first);
    Some(StringSpan::new(
        first.get_start() as usize,
        (last.get_end() - first.get_start()) as usize,
    ))
}

#[cfg(test)]
mod test_conversion {
    use crate::{
        ast::{Dialect, StatementKind, StatementNode},
        parse_statements,
    };

    #[rstest::rstest]
    #[case(17, "cube--1.3--1.4")]
    #[case(17, "seg--1.2--1.3")]
    #[case(18, "cube--1.3--1.4")]
    #[case(18, "seg--1.2--1.3")]
    fn converts_bundled_block(#[case] version: u8, #[case] script: &str) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../sql-fun-home/postgres/{version}/extension/{script}.sql"
        ));
        let script = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let (_, block) = script.split_once("\nDO LANGUAGE plpgsql\n").unwrap();
        let (body_literal, _) = block.split_once("$$;").unwrap();
        let body_literal = format!("{body_literal}$$");
        let sql = format!("DO LANGUAGE plpgsql\n{body_literal}");
        assert_conversion(
            &sql,
            body_literal
                .strip_prefix("$$")
                .unwrap()
                .strip_suffix("$$")
                .unwrap(),
            &body_literal,
            Some(("plpgsql", "plpgsql")),
        );
    }

    #[rstest::rstest]
    #[case("DO 'BEGIN NULL; END'", "BEGIN NULL; END", "'BEGIN NULL; END'", None)]
    #[case(
        "DO /* block */ $code$BEGIN NULL; END$code$ LANGUAGE /* name */ \"plpython3u\"",
        "BEGIN NULL; END",
        "$code$BEGIN NULL; END$code$",
        Some(("plpython3u", "\"plpython3u\"")),
    )]
    #[case(
        r"DO LANGUAGE 'plpgsql' E'BEGIN\nNULL; END'",
        "BEGIN\nNULL; END",
        r"E'BEGIN\nNULL; END'",
        Some(("plpgsql", "'plpgsql'")),
    )]
    #[case(
        "DO 'BEGIN '\n'NULL; END'",
        "BEGIN NULL; END",
        "'BEGIN '\n'NULL; END'",
        None
    )]
    #[case(
        "DO U&'B!0045GIN NULL; END' UESCAPE '!'",
        "BEGIN NULL; END",
        "U&'B!0045GIN NULL; END' UESCAPE '!'",
        None
    )]
    fn converts_legal_forms(
        #[case] sql: &str,
        #[case] decoded: &str,
        #[case] literal: &str,
        #[case] language: Option<(&str, &str)>,
    ) {
        assert_conversion(sql, decoded, literal, language);
    }

    fn assert_conversion(
        declaration: &str,
        decoded: &str,
        literal: &str,
        language: Option<(&str, &str)>,
    ) {
        let sql = format!("SELECT 'é';\n{declaration}; SELECT 2");
        let mut statements = parse_statements(&sql, Dialect::postgres());
        let statement = statements.nth(1).unwrap().unwrap();
        assert!(statements.next().unwrap().unwrap().is_query());
        assert_eq!(statement.kind(), StatementKind::Do);
        let statement_sql = statement.span().get_from(&sql).unwrap();
        assert_eq!(statement_sql.trim().trim_end_matches(';'), declaration);
        let StatementNode::Do(block) = statement.node() else {
            panic!("expected DO");
        };
        assert_eq!(
            block.keyword_span.unwrap().get_from(statement_sql),
            Some("DO")
        );
        assert_eq!(block.body.text, decoded);
        assert_eq!(
            block.body.span.unwrap().get_from(statement_sql),
            Some(literal)
        );
        assert_eq!(
            block.language.as_ref().map(|language| (
                language.text.as_str(),
                language.span.unwrap().get_from(statement_sql).unwrap(),
            )),
            language
        );
    }
}
