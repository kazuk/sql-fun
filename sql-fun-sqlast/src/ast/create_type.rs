//! Unresolved PostgreSQL shell and base type declarations and settings changes.

use crate::{
    StringSpan,
    ast::{
        Name, StatementKind, SyntaxOption, TryFromWithTokens,
        postgres::{anchored_token_span, required},
    },
    error::AnalysisError,
    syn::{self, ListOpt, Opt, ScanToken},
};
use ::sql_fun_core::IVec;

#[derive(Clone, Debug)]
pub(crate) struct CreateType {
    name: Name,
    definition: Option<Vec<BaseTypeOption>>,
}

#[derive(Clone, Debug)]
pub(crate) struct BaseTypeOption {
    syntax: SyntaxOption,
    // Token span relative to the statement being converted.
    location: Option<StringSpan>,
}

impl BaseTypeOption {
    pub(crate) const fn syntax(&self) -> &SyntaxOption {
        &self.syntax
    }
}

impl CreateType {
    pub(crate) const fn name(&self) -> &Name {
        &self.name
    }

    pub(crate) fn definition(&self) -> Option<&[BaseTypeOption]> {
        self.definition.as_deref()
    }

    pub(super) const fn kind(&self) -> StatementKind {
        if self.definition.is_some() {
            StatementKind::BaseType
        } else {
            StatementKind::ShellType
        }
    }
}

impl TryFromWithTokens<&syn::Node> for BaseTypeOption {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        node: &syn::Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, Self::Error> {
        let option = required(node.as_def_elem().as_inner(), "base type option")?;
        Ok(Self {
            syntax: SyntaxOption::try_from_with_tokens(&option, tokens)?,
            location: anchored_token_span(option.get_location(), tokens),
        })
    }
}

impl TryFromWithTokens<&syn::DefineStmt> for CreateType {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::DefineStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, Self::Error> {
        let name = Name::try_from(value.get_defnames())?;
        let definition = value
            .get_definition()
            .as_inner()
            .filter(|nodes| !nodes.is_empty())
            .map(|nodes| {
                nodes
                    .iter()
                    .map(|node| BaseTypeOption::try_from_with_tokens(node, tokens))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        Ok(Self { name, definition })
    }
}

#[cfg(test)]
mod test_create_type_conversion {
    use crate::{
        ast::{Dialect, Expr, Literal, StatementKind, StatementNode, SyntaxValue},
        parse_statements,
    };

    #[test]
    fn retains_unresolved_shell_and_base_type_declarations() {
        let sql = r#"CREATE TYPE "App"."Amount";
CREATE TYPE "App"."Amount" (
    INPUT = "App"."Read", OUTPUT = "App"."Write",
    INTERNALLENGTH = VARIABLE, DEFAULT = '0', PASSEDBYVALUE
);"#;

        let mut statements = parse_statements(sql, Dialect::postgres());
        let shell = statements.next().unwrap().unwrap();
        assert_eq!(shell.kind(), StatementKind::ShellType);
        let StatementNode::CreateType(shell_type) = shell.node() else {
            panic!("expected a shell type");
        };
        assert_eq!(shell_type.name.0, ["App", "Amount"]);
        assert!(shell_type.definition.is_none());

        let base = statements.next().unwrap().unwrap();
        assert_eq!(base.kind(), StatementKind::BaseType);
        let StatementNode::CreateType(base_type) = base.node() else {
            panic!("expected a base type");
        };
        assert_eq!(base_type.name.0, ["App", "Amount"]);
        let options = base_type.definition.as_deref().unwrap();
        assert_eq!(
            options
                .iter()
                .map(|opt| opt.syntax.name.as_str())
                .collect::<Vec<_>>(),
            [
                "input",
                "output",
                "internallength",
                "default",
                "passedbyvalue"
            ]
        );
        let Some(SyntaxValue::Type(input)) = &options[0].syntax.value else {
            panic!("expected a qualified input function name");
        };
        assert_eq!(input.name.0, ["App", "Read"]);
        assert!(options[2].syntax.value.is_some());
        assert!(matches!(
            &options[3].syntax.value,
            Some(SyntaxValue::Expression(Expr::Literal(Literal::String(value)))) if value == "0"
        ));
        assert!(options[4].syntax.value.is_none());

        let statement_sql = base.span().get_from(sql).unwrap();
        assert_eq!(
            options[4]
                .location
                .as_ref()
                .unwrap()
                .get_from(statement_sql),
            Some("PASSEDBYVALUE")
        );
        assert!(statements.next().is_none());
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AlterTypeSettings {
    name: Name,
    options: Vec<BaseTypeOption>,
}

impl TryFromWithTokens<&syn::AlterTypeStmt> for AlterTypeSettings {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        statement: &syn::AlterTypeStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, Self::Error> {
        let name = Name::try_from(statement.get_type_name())?;
        let options = required(statement.get_options().as_inner(), "base type settings")?
            .iter()
            .map(|node| BaseTypeOption::try_from_with_tokens(node, tokens))
            .collect::<Result<_, _>>()?;
        Ok(Self { name, options })
    }
}

#[cfg(test)]
mod test_alter_type_settings_conversion {
    use crate::{
        ast::{Dialect, StatementKind, StatementNode, SyntaxValue},
        parse_statements,
    };

    #[test]
    fn retains_unresolved_base_type_settings() {
        for (sql, target, receive, send) in [
            (
                "ALTER TYPE ltree SET ( RECEIVE = ltree_recv, SEND = ltree_send );",
                &["ltree"][..],
                &["ltree_recv"][..],
                &["ltree_send"][..],
            ),
            (
                r#"ALTER TYPE "App"."Tree" SET (RECEIVE = "App"."Read", SEND = "App"."Write");"#,
                &["App", "Tree"][..],
                &["App", "Read"][..],
                &["App", "Write"][..],
            ),
        ] {
            let script = format!("SELECT 1;\n{sql}");
            let statement = parse_statements(&script, Dialect::postgres())
                .nth(1)
                .unwrap()
                .unwrap();
            assert_eq!(statement.kind(), StatementKind::AlterTypeSettings);
            let StatementNode::AlterTypeSettings(settings) = statement.node() else {
                panic!("expected base type settings");
            };
            assert_eq!(settings.name.0, target);
            let statement_sql = statement.span().get_from(&script).unwrap();
            assert_eq!(settings.options.len(), 2);
            for (option, (name, function)) in settings
                .options
                .iter()
                .zip([("RECEIVE", receive), ("SEND", send)])
            {
                assert_eq!(option.syntax.name, name.to_ascii_lowercase());
                let Some(SyntaxValue::Type(value)) = &option.syntax.value else {
                    panic!("expected an unresolved function name");
                };
                assert_eq!(value.name.0, function);
                assert_eq!(
                    option.location.as_ref().unwrap().get_from(statement_sql),
                    Some(name)
                );
            }
        }
    }
}
