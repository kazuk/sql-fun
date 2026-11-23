//! Converts PostgreSQL text search template declarations before catalog resolution.

use crate::{
    StringSpan,
    ast::{
        Literal, Name, TryFromWithTokens,
        postgres::{anchored_token_span, required},
    },
    error::AnalysisError,
    syn::{self, ListOpt, Opt, ScanToken},
};
use ::sql_fun_core::IVec;

#[derive(Clone, Debug)]
pub(crate) struct CreateTextSearchTemplate {
    pub(crate) name: Name,
    pub(crate) options: Vec<TemplateOption>,
}

#[derive(Clone, Debug)]
pub(crate) struct TemplateOption {
    pub(crate) key: String,
    pub(crate) value: Option<TemplateOptionValue>,
    // Relative to the statement being converted.
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) enum TemplateOptionValue {
    Name(Name),
    Literal(Literal),
}

impl TryFromWithTokens<&syn::DefineStmt> for CreateTextSearchTemplate {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::DefineStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, Self::Error> {
        let name = Name::try_from(value.get_defnames())?;
        let options = value
            .get_definition()
            .as_inner()
            .unwrap_or_default()
            .iter()
            .map(|node| TemplateOption::try_from_with_tokens(node, tokens))
            .collect::<Result<_, _>>()?;
        Ok(Self { name, options })
    }
}

#[cfg(test)]
mod test_template_conversion {
    use crate::{
        ast::{
            Dialect, Literal, StatementKind, StatementNode,
            create_text_search_template::TemplateOptionValue,
        },
        parse_statements,
    };

    #[rstest::rstest]
    // These three declarations are identical across the audited 17.6 and 18.1 bundles.
    #[case(
        "intdict_template (LEXIZE = dintdict_lexize, INIT = dintdict_init)",
        &["intdict_template"],
        &[("LEXIZE", &["dintdict_lexize"][..]), ("INIT", &["dintdict_init"][..])]
    )]
    #[case(
        "xsyn_template (LEXIZE = dxsyn_lexize, INIT = dxsyn_init)",
        &["xsyn_template"],
        &[("LEXIZE", &["dxsyn_lexize"][..]), ("INIT", &["dxsyn_init"][..])]
    )]
    #[case(
        "unaccent (INIT = unaccent_init, LEXIZE = unaccent_lexize)",
        &["unaccent"],
        &[("INIT", &["unaccent_init"][..]), ("LEXIZE", &["unaccent_lexize"][..])]
    )]
    #[case(
        r#""App"."Template" (LEXIZE = "App"."Lexize")"#,
        &["App", "Template"],
        &[("LEXIZE", &["App", "Lexize"][..])]
    )]
    #[case(
        "t (LEXIZE = 'Lexize', INIT = app.init, LEXIZE = app.lexize)",
        &["t"],
        &[("LEXIZE", &["Lexize"][..]), ("INIT", &["app", "init"][..]), ("LEXIZE", &["app", "lexize"][..])]
    )]
    fn retains_written_declaration(
        #[case] declaration: &str,
        #[case] expected_name: &[&str],
        #[case] expected_options: &[(&str, &[&str])],
    ) {
        let sql = format!("SELECT 1; CREATE TEXT SEARCH TEMPLATE {declaration};");
        let statement = parse_statements(&sql, Dialect::postgres())
            .nth(1)
            .unwrap()
            .unwrap();
        assert_eq!(statement.kind(), StatementKind::CreateTextSearchTemplate);
        let StatementNode::CreateTextSearchTemplate(template) = statement.node() else {
            panic!("expected CREATE TEXT SEARCH TEMPLATE");
        };
        let statement_sql = statement.span().get_from(&sql).unwrap();
        assert!(
            statement_sql
                .trim_start()
                .starts_with("CREATE TEXT SEARCH TEMPLATE ")
        );
        assert_eq!(template.name.0, expected_name);
        assert_eq!(template.options.len(), expected_options.len());

        let mut remaining_sql = statement_sql;
        for (option, &(token, expected_value)) in template.options.iter().zip(expected_options) {
            assert_eq!(option.key, token.to_ascii_lowercase());
            let (prefix, suffix) = remaining_sql.split_once(token).unwrap();
            let offset = statement_sql.len() - remaining_sql.len() + prefix.len();
            let location = option.location.as_ref().unwrap();
            assert_eq!(location.get_from(statement_sql), Some(token));
            assert_eq!(location, &crate::StringSpan::new(offset, token.len()));
            remaining_sql = suffix;
            match option.value.as_ref().unwrap() {
                TemplateOptionValue::Name(name) => assert_eq!(name.0, expected_value),
                TemplateOptionValue::Literal(Literal::String(name)) => {
                    assert_eq!([name.as_str()], expected_value);
                }
                value => panic!("unexpected template option value: {value:?}"),
            }
        }
    }
}

impl TryFromWithTokens<&syn::Node> for TemplateOption {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        node: &syn::Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, Self::Error> {
        let option = required(node.as_def_elem().as_inner(), "text search template option")?;
        let value = option
            .get_arg()
            .as_inner()
            .map(|node| {
                if let Some(typ) = node.as_type_name().as_inner() {
                    Name::try_from(typ.get_names()).map(TemplateOptionValue::Name)
                } else if let Some(list) = node.as_list().as_inner() {
                    Name::try_from(list.get_items()).map(TemplateOptionValue::Name)
                } else {
                    required(
                        Option::<Literal>::from(&node),
                        "text search template option value",
                    )
                    .map(TemplateOptionValue::Literal)
                }
            })
            .transpose()?;
        Ok(Self {
            key: option.get_defname(),
            value,
            location: anchored_token_span(option.get_location(), tokens),
        })
    }
}
