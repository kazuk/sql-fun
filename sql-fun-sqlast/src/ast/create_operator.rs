//! Unresolved PostgreSQL operator declarations.

use crate::{
    StringSpan,
    ast::{
        DropBehavior, Literal, Name, ObjectArgument, TryFromWithTokens, TypeName,
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
        let options = OperatorOption::convert_list(value.get_definition(), tokens)?;
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

impl OperatorOption {
    fn convert_list(
        options: syn::NodeListOpt,
        tokens: &IVec<ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        options
            .as_inner()
            .unwrap_or_default()
            .iter()
            .map(|node| Self::try_from_with_tokens(node, tokens))
            .collect()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AlterOperator {
    signature: OperatorSignature,
    options: Vec<OperatorOption>,
}

impl AlterOperator {
    pub(crate) const fn name(&self) -> &Name {
        &self.signature.name
    }

    pub(crate) const fn operands(&self) -> &[ObjectArgument; 2] {
        &self.signature.operands
    }

    pub(crate) fn options(&self) -> &[OperatorOption] {
        &self.options
    }
}

impl TryFromWithTokens<&syn::AlterOperatorStmt> for AlterOperator {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::AlterOperatorStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, Self::Error> {
        let target = required(value.get_opername().as_inner(), "ALTER OPERATOR target")?;
        Ok(Self {
            signature: OperatorSignature::try_from_with_tokens(&target, tokens)?,
            options: OperatorOption::convert_list(value.get_options(), tokens)?,
        })
    }
}

#[cfg(test)]
mod test_alter_operator_conversion {
    use crate::{
        ast::{Dialect, StatementKind, StatementNode},
        parse_statements,
    };

    #[test]
    fn converts_audited_bundle_declarations() {
        // These declarations are identical in the 17.6 and 18.1 bundles.
        // Sources: cube--1.2--1.3, seg--1.1--1.2, and pg_trgm--1.4--1.5.
        let fixtures: [(_, _, &[_]); 3] = [
            (
                "cube",
                r#"ALTER OPERATOR <= (cube, cube) SET (
	RESTRICT = scalarlesel, JOIN = scalarlejoinsel
);
ALTER OPERATOR >= (cube, cube) SET (
	RESTRICT = scalargesel, JOIN = scalargejoinsel
);"#,
                &[
                    ("<=", "scalarlesel", "scalarlejoinsel"),
                    (">=", "scalargesel", "scalargejoinsel"),
                ],
            ),
            (
                "seg",
                r#"ALTER OPERATOR <= (seg, seg) SET (
	RESTRICT = scalarlesel,
	JOIN = scalarlejoinsel
);
ALTER OPERATOR >= (seg, seg) SET (
	RESTRICT = scalargesel,
	JOIN = scalargejoinsel
);"#,
                &[
                    ("<=", "scalarlesel", "scalarlejoinsel"),
                    (">=", "scalargesel", "scalargejoinsel"),
                ],
            ),
            (
                "text",
                r#"ALTER OPERATOR % (text, text)
  SET (RESTRICT = matchingsel, JOIN = matchingjoinsel);
ALTER OPERATOR <% (text, text)
  SET (RESTRICT = matchingsel, JOIN = matchingjoinsel);
ALTER OPERATOR %> (text, text)
  SET (RESTRICT = matchingsel, JOIN = matchingjoinsel);
ALTER OPERATOR <<% (text, text)
  SET (RESTRICT = matchingsel, JOIN = matchingjoinsel);
ALTER OPERATOR %>> (text, text)
  SET (RESTRICT = matchingsel, JOIN = matchingjoinsel);"#,
                &[
                    ("%", "matchingsel", "matchingjoinsel"),
                    ("<%", "matchingsel", "matchingjoinsel"),
                    ("%>", "matchingsel", "matchingjoinsel"),
                    ("<<%", "matchingsel", "matchingjoinsel"),
                    ("%>>", "matchingsel", "matchingjoinsel"),
                ],
            ),
        ];
        for (operand, sql, expected) in fixtures {
            let statements = parse_statements(sql, Dialect::postgres())
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(statements.len(), expected.len());
            for (statement, &(name, restrict, join)) in statements.iter().zip(expected) {
                assert_eq!(statement.kind(), StatementKind::AlterOperator);
                let StatementNode::AlterOperator(operator) = statement.node() else {
                    panic!("expected ALTER OPERATOR");
                };
                assert_eq!(operator.name().0, [name]);
                for argument in operator.operands() {
                    assert_eq!(argument.type_name.as_ref().unwrap().name.0, [operand]);
                }
                let statement_sql = statement.span().get_from(sql).unwrap();
                assert_eq!(operator.options.len(), 2);
                for (option, (key, estimator, token)) in operator
                    .options
                    .iter()
                    .zip([("restrict", restrict, "RESTRICT"), ("join", join, "JOIN")])
                {
                    assert_eq!(option.key(), key);
                    assert_eq!(option.name().unwrap().0, [estimator]);
                    assert_eq!(
                        option.location().unwrap().get_from(statement_sql),
                        Some(token)
                    );
                }
            }
        }
    }

    #[test]
    fn retains_qualified_prefix_operator_and_explicit_reset() {
        let sql = r#"ALTER OPERATOR "App".- (NONE, pg_catalog.int4) SET (RESTRICT = NONE);"#;
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let StatementNode::AlterOperator(operator) = statement.node() else {
            panic!("expected ALTER OPERATOR");
        };
        assert_eq!(operator.name().0, ["App", "-"]);
        assert!(operator.operands()[0].type_name.is_none());
        assert_eq!(
            operator.operands()[1].type_name.as_ref().unwrap().name.0,
            ["pg_catalog", "int4"],
        );
        assert_eq!(operator.options.len(), 1);
        assert_eq!(operator.options[0].key(), "restrict");
        assert!(operator.options[0].value().is_none());
    }
}

#[derive(Clone, Debug)]
pub(crate) struct OperatorSignature {
    name: Name,
    operands: [ObjectArgument; 2],
}

impl OperatorSignature {
    pub(crate) const fn name(&self) -> &Name {
        &self.name
    }

    pub(crate) const fn operands(&self) -> &[ObjectArgument; 2] {
        &self.operands
    }

    fn try_from_drop_target(
        node: &syn::Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let target = required(
            node.as_object_with_args().as_inner(),
            "DROP OPERATOR target",
        )?;
        Self::try_from_with_tokens(&target, tokens)
    }
}

impl TryFromWithTokens<&syn::ObjectWithArgs> for OperatorSignature {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::ObjectWithArgs,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, Self::Error> {
        let name = Name::try_from(value.get_objname())?;
        let operands = required(value.get_objargs().as_inner(), "operator operands")?;
        let [left, right] = &operands[..] else {
            return AnalysisError::raise_unexpected_input("operator operand slots");
        };
        Ok(Self {
            name,
            operands: [
                ObjectArgument::try_from_node(left, tokens)?,
                ObjectArgument::try_from_node(right, tokens)?,
            ],
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DropOperator {
    targets: Vec<OperatorSignature>,
    if_exists: bool,
    behavior: DropBehavior,
}

impl DropOperator {
    pub(crate) fn targets(&self) -> &[OperatorSignature] {
        &self.targets
    }

    pub(crate) const fn if_exists(&self) -> bool {
        self.if_exists
    }

    pub(crate) const fn behavior(&self) -> DropBehavior {
        self.behavior
    }
}

impl TryFromWithTokens<&syn::DropStmt> for DropOperator {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::DropStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, Self::Error> {
        let objects = required(value.get_objects().as_inner(), "DROP OPERATOR targets")?;
        let targets = objects
            .iter()
            .map(|node| OperatorSignature::try_from_drop_target(node, tokens))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            targets,
            if_exists: value.get_missing_ok(),
            behavior: DropBehavior::try_from(value.get_behavior().as_inner())?,
        })
    }
}

#[cfg(test)]
mod test_drop_operator_conversion {
    use crate::{
        ast::{Dialect, DropBehavior, StatementKind, StatementNode},
        parse_statements,
    };

    #[test]
    fn retains_binary_signatures_and_source_locations() {
        // Both bundled intarray--1.4--1.5.sql scripts use these declarations.
        let sql = r#"DROP OPERATOR @ (_int4, _int4);
DROP OPERATOR ~ (_int4, _int4);"#;
        let statements = parse_statements(sql, Dialect::postgres())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(statements.len(), 2);
        for (statement, name) in statements.iter().zip(["@", "~"]) {
            assert_eq!(statement.kind(), StatementKind::DropOperator);
            let StatementNode::DropOperator(drop) = statement.node() else {
                panic!("expected DROP OPERATOR");
            };
            assert!(!drop.if_exists);
            assert_eq!(drop.behavior, DropBehavior::Restrict);
            let [target] = drop.targets.as_slice() else {
                panic!("expected a single operator signature");
            };
            assert_eq!(target.name.0, [name]);
            let statement_sql = statement.span().get_from(sql).unwrap();
            assert!(
                statement_sql
                    .trim_start()
                    .starts_with(&format!("DROP OPERATOR {name}"))
            );
            for argument in &target.operands {
                let ty = argument.type_name.as_ref().unwrap();
                assert_eq!(ty.name.0, ["_int4"]);
                assert_eq!(ty.location.unwrap().get_from(statement_sql), Some("_int4"));
            }
        }
    }

    #[rstest::rstest]
    #[case::cascade("CASCADE", DropBehavior::Cascade)]
    #[case::restrict("RESTRICT", DropBehavior::Restrict)]
    fn retains_ordered_signatures_and_options(
        #[case] option: &str,
        #[case] behavior: DropBehavior,
    ) {
        let sql = format!(
            r#"DROP OPERATOR IF EXISTS "App".- (NONE, pg_catalog.int4), "App".- (text, int4) {option};"#
        );
        let statement = parse_statements(&sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let StatementNode::DropOperator(drop) = statement.node() else {
            panic!("expected DROP OPERATOR");
        };
        assert!(drop.if_exists);
        assert_eq!(drop.behavior, behavior);
        let [unary, binary] = drop.targets.as_slice() else {
            panic!("expected two operator signatures");
        };
        assert_eq!(unary.name.0, ["App", "-"]);
        assert_eq!(binary.name.0, ["App", "-"]);
        assert!(unary.operands[0].type_name.is_none());
        assert_eq!(
            unary.operands[1].type_name.as_ref().unwrap().name.0,
            ["pg_catalog", "int4"],
        );
        assert_eq!(
            binary.operands[0].type_name.as_ref().unwrap().name.0,
            ["text"]
        );
        assert_eq!(
            binary.operands[1].type_name.as_ref().unwrap().name.0,
            ["int4"]
        );
    }
}
