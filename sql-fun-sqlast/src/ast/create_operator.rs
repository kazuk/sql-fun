//! Unresolved PostgreSQL operator declarations.

use crate::{
    StringSpan,
    ast::{
        Literal, Name, TryFromWithTokens, TypeName,
        postgres::{anchored_token_span, required},
    },
    error::AnalysisError,
    syn::{self, ListOpt, Opt, ScanToken},
};
use ::sql_fun_core::IVec;

#[derive(Clone, Debug)]
pub(crate) struct CreateOperator {
    name: Name,
    options: Vec<OperatorOption>,
}

#[derive(Clone, Debug)]
pub(crate) struct OperatorOption {
    key: String,
    value: Option<OperatorOptionValue>,
    // Relative to the statement being converted.
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) enum OperatorOptionValue {
    Type(TypeName),
    Name(Name),
    Literal(Literal),
}

impl CreateOperator {
    pub(crate) const fn name(&self) -> &Name {
        &self.name
    }

    pub(crate) fn options(&self) -> &[OperatorOption] {
        &self.options
    }
}

impl OperatorOption {
    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    pub(crate) const fn value(&self) -> Option<&OperatorOptionValue> {
        self.value.as_ref()
    }

    pub(crate) const fn location(&self) -> Option<StringSpan> {
        self.location
    }
}

// Quoted relationship targets bypass the scanner's operator-token rules.
pub(crate) fn valid_operator_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() < 64
        && token != "!="
        && token
            .bytes()
            .all(|byte| b"+-*/<>=~!@#%^&|`?".contains(&byte))
        && !token.contains("/*")
        && !token.contains("--")
        && (token.len() == 1
            || !token.ends_with(['+', '-'])
            || token.bytes().any(|byte| b"~!@#%^&|`?".contains(&byte)))
}

impl OperatorOption {
    pub(crate) fn name(&self) -> Option<Name> {
        match &self.value {
            Some(OperatorOptionValue::Name(name)) => Some(name.clone()),
            Some(OperatorOptionValue::Literal(Literal::String(value))) => Some(Name::local(value)),
            _ => None,
        }
    }
}

impl TryFromWithTokens<&syn::DefineStmt> for CreateOperator {
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
            .map(|node| OperatorOption::try_from_with_tokens(node, tokens))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { name, options })
    }
}

#[cfg(test)]
mod test_create_operator_conversion {
    use crate::{
        ast::{Dialect, StatementKind, StatementNode},
        parse_statements,
    };

    #[test]
    fn converts_bundled_operator_declarations() {
        // The PostgreSQL 17.6 and 18.1 bundles contain the same declarations.
        // The last declaration is from earthdistance; the others are from pg_trgm.
        for (sql, expected) in [
            (
                r#"CREATE OPERATOR % (
        LEFTARG = text,
        RIGHTARG = text,
        PROCEDURE = similarity_op,
        COMMUTATOR = '%',
        RESTRICT = contsel,
        JOIN = contjoinsel
);"#,
                "%",
            ),
            (
                r#"CREATE OPERATOR <% (
        LEFTARG = text,
        RIGHTARG = text,
        PROCEDURE = word_similarity_op,
        COMMUTATOR = '%>',
        RESTRICT = contsel,
        JOIN = contjoinsel
);"#,
                "<%",
            ),
            (
                r#"CREATE OPERATOR %> (
        LEFTARG = text,
        RIGHTARG = text,
        PROCEDURE = word_similarity_commutator_op,
        COMMUTATOR = '<%',
        RESTRICT = contsel,
        JOIN = contjoinsel
);"#,
                "%>",
            ),
            (
                r#"CREATE OPERATOR <-> (
        LEFTARG = text,
        RIGHTARG = text,
        PROCEDURE = similarity_dist,
        COMMUTATOR = '<->'
);"#,
                "<->",
            ),
            (
                r#"CREATE OPERATOR <<-> (
        LEFTARG = text,
        RIGHTARG = text,
        PROCEDURE = word_similarity_dist_op,
        COMMUTATOR = '<->>'
);"#,
                "<<->",
            ),
            (
                r#"CREATE OPERATOR <->> (
        LEFTARG = text,
        RIGHTARG = text,
        PROCEDURE = word_similarity_dist_commutator_op,
        COMMUTATOR = '<<->'
);"#,
                "<->>",
            ),
            (
                r#"CREATE OPERATOR <@> (
  LEFTARG = point,
  RIGHTARG = point,
  PROCEDURE = geo_distance,
  COMMUTATOR = <@>
);"#,
                "<@>",
            ),
        ] {
            let statement = parse_statements(sql, Dialect::postgres())
                .next()
                .unwrap()
                .unwrap();
            assert_eq!(statement.kind(), StatementKind::CreateOperator);
            let StatementNode::CreateOperator(operator) = statement.node() else {
                panic!("expected CREATE OPERATOR");
            };
            assert_eq!(operator.name.0, [expected]);
        }
    }

    #[test]
    fn retains_qualified_name_and_option_order_for_prefix_operator() {
        let sql =
            r#"CREATE OPERATOR "App".!! (RIGHTARG = integer, PROCEDURE = pg_catalog.int4um);"#;
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let StatementNode::CreateOperator(operator) = statement.node() else {
            panic!("expected CREATE OPERATOR");
        };
        assert_eq!(operator.name.0, ["App", "!!"]);
        assert_eq!(
            operator
                .options
                .iter()
                .map(|option| option.key.as_str())
                .collect::<Vec<_>>(),
            ["rightarg", "procedure"]
        );
    }
}

impl TryFromWithTokens<&syn::Node> for OperatorOption {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        node: &syn::Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, Self::Error> {
        let option = required(node.as_def_elem().as_inner(), "operator option")?;
        let key = option.get_defname();
        let value = option
            .get_arg()
            .as_inner()
            .map(|node| OperatorOptionValue::from_argument(&key, &node, tokens))
            .transpose()?;
        Ok(Self {
            key,
            value,
            location: anchored_token_span(option.get_location(), tokens),
        })
    }
}

#[cfg(test)]
mod test_operator_option_conversion {
    use crate::{
        ast::{Dialect, StatementNode},
        parse_statements,
    };

    #[test]
    fn retains_flag_keys_and_locations_without_values() {
        let sql = "CREATE OPERATOR === (LEFTARG = integer, RIGHTARG = integer, FUNCTION = int4eq, HASHES, MERGES);";
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let StatementNode::CreateOperator(operator) = statement.node() else {
            panic!("expected CREATE OPERATOR");
        };
        let statement_sql = statement.span().get_from(sql).unwrap();
        for (option, (key, token)) in operator.options[3..]
            .iter()
            .zip([("hashes", "HASHES"), ("merges", "MERGES")])
        {
            assert_eq!(option.key, key);
            assert!(option.value.is_none());
            assert_eq!(
                option.location.as_ref().unwrap().get_from(statement_sql),
                Some(token)
            );
        }
        assert_eq!(operator.options.len(), 5);
    }
}

impl OperatorOptionValue {
    fn from_argument(
        key: &str,
        node: &syn::Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, AnalysisError> {
        // PostgreSQL represents bare function names as TypeName nodes and
        // unquoted operator references as lists. Quoted names are strings.
        let takes_name = matches!(
            key,
            "function" | "procedure" | "commutator" | "negator" | "restrict" | "join"
        );
        if let Some(typ) = node.as_type_name().as_inner() {
            return if takes_name {
                Name::try_from(typ.get_names()).map(Self::Name)
            } else {
                TypeName::try_from_with_tokens(&typ, tokens).map(Self::Type)
            };
        }
        if let Some(list) = node.as_list().as_inner() {
            return Name::try_from(list.get_items()).map(Self::Name);
        }
        if takes_name && let Some(name) = node.as_string().get_sval() {
            return Ok(Self::Name(Name(vec![name])));
        }
        required(Option::<Literal>::from(node), "operator option value").map(Self::Literal)
    }
}

#[cfg(test)]
mod test_operator_option_value_conversion {
    use crate::{
        ast::{Dialect, StatementNode, create_operator::OperatorOptionValue},
        parse_statements,
    };

    #[test]
    fn retains_operand_type_and_qualified_or_quoted_names() {
        let sql = r#"CREATE OPERATOR "App".=== (
    LEFTARG = integer, RIGHTARG = integer, FUNCTION = pg_catalog.int4eq,
    COMMUTATOR = '===', NEGATOR = OPERATOR("App".!==),
    RESTRICT = pg_catalog.eqsel, JOIN = pg_catalog.eqjoinsel
);"#;
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let StatementNode::CreateOperator(operator) = statement.node() else {
            panic!("expected CREATE OPERATOR");
        };
        let options = &operator.options;
        for option in &options[..2] {
            assert!(matches!(
                &option.value,
                Some(OperatorOptionValue::Type(typ)) if typ.name.0 == ["pg_catalog", "int4"]
            ));
        }
        for (option, expected) in options[2..].iter().zip([
            &["pg_catalog", "int4eq"][..],
            &["==="],
            &["App", "!=="],
            &["pg_catalog", "eqsel"],
            &["pg_catalog", "eqjoinsel"],
        ]) {
            assert!(matches!(
                &option.value,
                Some(OperatorOptionValue::Name(name)) if name.0 == expected
            ));
        }
        assert_eq!(options.len(), 7);
    }
}
