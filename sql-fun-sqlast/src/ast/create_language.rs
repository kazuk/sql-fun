//! Unresolved handler-bearing PostgreSQL language declarations.

use crate::{
    ast::{Name, postgres::required},
    error::AnalysisError,
    syn::{CreatePLangStmt, ListOpt, NodeListOpt},
};

#[derive(Clone, Debug)]
#[allow(
    dead_code,
    reason = "CREATE LANGUAGE interpretation is deferred to #1246"
)]
pub(crate) struct CreateLanguage {
    name: String,
    replace: bool,
    trusted: bool,
    handler: Name,
    inline: Option<Name>,
    validator: Option<Name>,
}

impl TryFrom<&CreatePLangStmt> for CreateLanguage {
    type Error = AnalysisError;

    fn try_from(value: &CreatePLangStmt) -> Result<Self, Self::Error> {
        let handler = required(
            optional_name(value.get_plhandler())?,
            "CREATE LANGUAGE handler",
        )?;

        Ok(Self {
            name: value.get_plname(),
            replace: value.get_replace(),
            trusted: value.get_pltrusted(),
            handler,
            inline: optional_name(value.get_plinline())?,
            validator: optional_name(value.get_plvalidator())?,
        })
    }
}

fn optional_name(parts: NodeListOpt) -> Result<Option<Name>, AnalysisError> {
    parts
        .as_inner()
        .filter(|nodes| !nodes.is_empty())
        .map(|nodes| Name::try_from_node_parts(&nodes))
        .transpose()
}

#[cfg(test)]
mod test_conversion {
    use crate::{
        ast::{Dialect, Name, StatementKind, StatementNode},
        parse_statements,
    };

    #[test]
    fn converts_audited_declaration() {
        // The declaration is identical in the PostgreSQL 17.6 and 18.1 bundles.
        let declaration = "CREATE TRUSTED LANGUAGE plpgsql
  HANDLER plpgsql_call_handler
  INLINE plpgsql_inline_handler
  VALIDATOR plpgsql_validator";
        let sql = format!("SELECT 1; {declaration};");
        let statement = parse_statements(&sql, Dialect::postgres())
            .nth(1)
            .unwrap()
            .unwrap();
        assert_eq!(statement.kind(), StatementKind::CreateLanguage);
        assert_eq!(
            statement
                .span()
                .get_from(&sql)
                .unwrap()
                .trim()
                .trim_end_matches(';'),
            declaration
        );

        let StatementNode::CreateLanguage(language) = statement.node() else {
            panic!("expected a language declaration");
        };
        assert_eq!(language.name, "plpgsql");
        assert!(!language.replace);
        assert!(language.trusted);
        assert_eq!(language.handler.parts(), ["plpgsql_call_handler"]);
        assert_eq!(
            language.inline.as_ref().unwrap().parts(),
            ["plpgsql_inline_handler"]
        );
        assert_eq!(
            language.validator.as_ref().unwrap().parts(),
            ["plpgsql_validator"]
        );
    }

    #[rstest::rstest]
    #[case("", None, None)]
    #[case(" INLINE app.inline_handler", Some(&["app", "inline_handler"]), None)]
    #[case(" VALIDATOR app.validate", None, Some(&["app", "validate"]))]
    #[case(" INLINE app.inline_handler NO VALIDATOR", Some(&["app", "inline_handler"]), None)]
    fn preserves_optional_callbacks(
        #[case] suffix: &str,
        #[case] inline: Option<&[&str; 2]>,
        #[case] validator: Option<&[&str; 2]>,
    ) {
        let sql = format!(
            r#"CREATE OR REPLACE PROCEDURAL LANGUAGE "Mixed Language" HANDLER "App"."Call"{suffix}"#
        );
        let statement = parse_statements(&sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let StatementNode::CreateLanguage(language) = statement.node() else {
            panic!("expected a language declaration");
        };
        assert_eq!(language.name, "Mixed Language");
        assert!(language.replace);
        assert!(!language.trusted);
        assert_eq!(language.handler.parts(), ["App", "Call"]);
        let parts = |name: &Name| -> Vec<String> { name.parts().to_vec() };
        assert_eq!(
            language.inline.as_ref().map(parts),
            inline.map(|name| name.iter().map(|part| (*part).to_owned()).collect())
        );
        assert_eq!(
            language.validator.as_ref().map(parts),
            validator.map(|name| name.iter().map(|part| (*part).to_owned()).collect())
        );
    }

    #[test]
    fn handlerless_form_keeps_extension_classification() {
        let statement = parse_statements(
            "CREATE OR REPLACE TRUSTED LANGUAGE plpgsql",
            Dialect::postgres(),
        )
        .next()
        .unwrap()
        .unwrap();
        assert_eq!(statement.kind(), StatementKind::Extension);
        let StatementNode::Extension(extension) = statement.node() else {
            panic!("expected an extension declaration");
        };
        assert_eq!(extension.name(), "plpgsql");
        assert!(extension.if_not_exists());
    }
}
