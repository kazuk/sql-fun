//! Retains unresolved PostgreSQL extension membership changes.

use crate::{
    ast::{Name, ObjectKind, ObjectTarget, TryFromWithTokens, postgres::required},
    error::AnalysisError,
    syn::{self, ListOpt, ObjectType, Opt, ScanToken},
};
use ::sql_fun_core::IVec;

#[derive(Clone, Debug)]
pub(crate) struct ExtensionMembership {
    extension: String,
    action: MembershipAction,
    member: ExtensionMember,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MembershipAction {
    Add,
    Drop,
}

impl TryFrom<i32> for MembershipAction {
    type Error = AnalysisError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Add),
            -1 => Ok(Self::Drop),
            _ => AnalysisError::raise_unexpected_input("extension membership action"),
        }
    }
}

#[derive(Clone, Debug)]
struct ExtensionMember {
    kind: ObjectKind,
    target: MemberTarget,
}

#[derive(Clone, Debug)]
enum MemberTarget {
    Object(Box<ObjectTarget>),
    AccessMethodObject(AccessMethodObject),
}

#[derive(Clone, Debug)]
struct AccessMethodObject {
    name: Name,
    method: String,
}

impl TryFrom<&syn::Node> for AccessMethodObject {
    type Error = AnalysisError;

    fn try_from(value: &syn::Node) -> Result<Self, Self::Error> {
        let items = required(
            value.as_list().get_items().as_inner(),
            "operator class or family",
        )?;
        // PostgreSQL prepends the access method to the qualified name.
        let Some((method, name)) = items.split_first() else {
            return AnalysisError::raise_unexpected_input("operator class or family");
        };
        let method = required(method.as_string().get_sval(), "index access method")?;
        Ok(Self {
            name: Name::try_from_node_parts(name)?,
            method,
        })
    }
}

impl ExtensionMember {
    fn try_from_node(
        kind: ObjectType,
        object: &syn::Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            kind: ObjectKind::from(kind as u32),
            target: MemberTarget::try_from_node(kind, object, tokens)?,
        })
    }
}

impl MemberTarget {
    fn object(target: ObjectTarget) -> Self {
        Self::Object(Box::new(target))
    }

    fn try_from_node(
        kind: ObjectType,
        object: &syn::Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, AnalysisError> {
        match kind {
            ObjectType::ObjectOpclass | ObjectType::ObjectOpfamily => Ok(Self::AccessMethodObject(
                AccessMethodObject::try_from(object)?,
            )),
            ObjectType::ObjectCast => Ok(Self::object(ObjectTarget::try_from_cast_node(
                object, tokens,
            )?)),
            ObjectType::ObjectTransform => Ok(Self::object(ObjectTarget::try_from_transform_node(
                object, tokens,
            )?)),
            ObjectType::ObjectAccessMethod
            | ObjectType::ObjectAggregate
            | ObjectType::ObjectCollation
            | ObjectType::ObjectConversion
            | ObjectType::ObjectDomain
            | ObjectType::ObjectEventTrigger
            | ObjectType::ObjectFdw
            | ObjectType::ObjectForeignServer
            | ObjectType::ObjectForeignTable
            | ObjectType::ObjectFunction
            | ObjectType::ObjectLanguage
            | ObjectType::ObjectMatview
            | ObjectType::ObjectOperator
            | ObjectType::ObjectProcedure
            | ObjectType::ObjectRoutine
            | ObjectType::ObjectSchema
            | ObjectType::ObjectSequence
            | ObjectType::ObjectTable
            | ObjectType::ObjectTsconfiguration
            | ObjectType::ObjectTsdictionary
            | ObjectType::ObjectTsparser
            | ObjectType::ObjectTstemplate
            | ObjectType::ObjectType
            | ObjectType::ObjectView => {
                Ok(Self::object(ObjectTarget::try_from_node(object, tokens)?))
            }
            _ => AnalysisError::raise_unexpected_input("extension member kind"),
        }
    }
}

impl TryFromWithTokens<&syn::AlterExtensionContentsStmt> for ExtensionMembership {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::AlterExtensionContentsStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, Self::Error> {
        let kind = required(value.get_objtype().as_inner(), "extension member kind")?;
        let object = required(value.get_object().as_inner(), "extension member object")?;
        Ok(Self {
            extension: value.get_extname(),
            action: MembershipAction::try_from(value.get_action())?,
            member: ExtensionMember::try_from_node(kind, &object, tokens)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ast::{
            Dialect, ObjectKind, StatementKind, StatementNode,
            extension_membership::{MemberTarget, MembershipAction},
        },
        parse_statements::parse_statements,
        syn::ObjectType,
    };

    #[test]
    fn converts_all_documented_members_for_both_actions() {
        let members = [
            ("ACCESS METHOD heap", ObjectType::ObjectAccessMethod),
            (
                "AGGREGATE app.agg(ORDER BY int)",
                ObjectType::ObjectAggregate,
            ),
            ("CAST (int AS text)", ObjectType::ObjectCast),
            ("COLLATION app.c", ObjectType::ObjectCollation),
            ("CONVERSION app.c", ObjectType::ObjectConversion),
            ("DOMAIN app.d", ObjectType::ObjectDomain),
            ("EVENT TRIGGER trg", ObjectType::ObjectEventTrigger),
            ("FOREIGN DATA WRAPPER fdw", ObjectType::ObjectFdw),
            ("FOREIGN TABLE app.t", ObjectType::ObjectForeignTable),
            ("FUNCTION app.f", ObjectType::ObjectFunction),
            ("MATERIALIZED VIEW app.v", ObjectType::ObjectMatview),
            ("OPERATOR app.+(int, int)", ObjectType::ObjectOperator),
            (
                "OPERATOR CLASS app.ops USING btree",
                ObjectType::ObjectOpclass,
            ),
            (
                "OPERATOR FAMILY app.ops USING btree",
                ObjectType::ObjectOpfamily,
            ),
            ("PROCEDURAL LANGUAGE plpgsql", ObjectType::ObjectLanguage),
            ("PROCEDURE app.p(integer)", ObjectType::ObjectProcedure),
            ("ROUTINE app.r(text)", ObjectType::ObjectRoutine),
            ("SCHEMA app", ObjectType::ObjectSchema),
            ("SEQUENCE app.s", ObjectType::ObjectSequence),
            ("SERVER srv", ObjectType::ObjectForeignServer),
            ("TABLE app.t", ObjectType::ObjectTable),
            (
                "TEXT SEARCH CONFIGURATION app.c",
                ObjectType::ObjectTsconfiguration,
            ),
            (
                "TEXT SEARCH DICTIONARY app.d",
                ObjectType::ObjectTsdictionary,
            ),
            ("TEXT SEARCH PARSER app.p", ObjectType::ObjectTsparser),
            ("TEXT SEARCH TEMPLATE app.t", ObjectType::ObjectTstemplate),
            (
                "TRANSFORM FOR app.t LANGUAGE plpgsql",
                ObjectType::ObjectTransform,
            ),
            ("TYPE app.t", ObjectType::ObjectType),
            ("VIEW app.v", ObjectType::ObjectView),
        ];
        for (verb, action) in [
            ("ADD", MembershipAction::Add),
            ("DROP", MembershipAction::Drop),
        ] {
            for (member, kind) in members {
                let sql = format!(r#"ALTER EXTENSION "My Extension" {verb} {member};"#);
                let statement = parse_statements(&sql, Dialect::postgres())
                    .next()
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    statement.kind(),
                    StatementKind::AlterExtensionMembership,
                    "{sql}"
                );
                let StatementNode::ExtensionMembership(change) = statement.into_test_node() else {
                    panic!("expected extension membership for {sql}");
                };
                assert_eq!(change.extension, "My Extension", "{sql}");
                assert_eq!(change.action, action, "{sql}");
                assert_eq!(change.member.kind, ObjectKind::from(kind as u32), "{sql}");
            }
        }
    }

    #[rstest::rstest]
    #[case::bundled(
        "ALTER EXTENSION pg_stat_statements DROP FUNCTION pg_stat_statements_reset();",
        "pg_stat_statements", MembershipAction::Drop, &["pg_stat_statements_reset"], Some(&[][..]),
    )]
    #[case::omitted(
        r#"ALTER EXTENSION ext ADD FUNCTION "App"."F";"#,
        "ext", MembershipAction::Add, &["App", "F"], None,
    )]
    #[case::typed(
        r#"ALTER EXTENSION ext DROP PROCEDURE "App"."P"(IN "Value" "App"."T"[], OUT result text);"#,
        "ext", MembershipAction::Drop, &["App", "P"], Some(&["App.T", "text"][..]),
    )]
    #[case::aggregate_star(
        "ALTER EXTENSION ext ADD AGGREGATE app.agg(*);",
        "ext", MembershipAction::Add, &["app", "agg"], Some(&[][..]),
    )]
    #[case::ordered_set(
        "ALTER EXTENSION ext DROP AGGREGATE app.agg(int ORDER BY text);",
        "ext", MembershipAction::Drop, &["app", "agg"], Some(&["pg_catalog.int4", "text"][..]),
    )]
    #[case::prefix_operator(
        "ALTER EXTENSION ext ADD OPERATOR app.!(NONE, integer);",
        "ext", MembershipAction::Add, &["app", "!"], Some(&["", "pg_catalog.int4"][..]),
    )]
    fn retains_signatures(
        #[case] sql: &str,
        #[case] extension: &str,
        #[case] action: MembershipAction,
        #[case] name: &[&str],
        #[case] types: Option<&[&str]>,
    ) {
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let StatementNode::ExtensionMembership(change) = statement.into_test_node() else {
            panic!("expected extension membership");
        };
        assert_eq!(change.extension, extension);
        assert_eq!(change.action, action);
        let MemberTarget::Object(target) = change.member.target else {
            panic!("expected an object signature");
        };
        assert_eq!(target.name(), name);
        let actual = target.argument_types().as_ref().map(|arguments| {
            arguments
                .iter()
                .map(|argument| {
                    argument
                        .type_name()
                        .as_ref()
                        .map_or_else(String::new, |ty| ty.name.0.join("."))
                })
                .collect::<Vec<_>>()
        });
        assert_eq!(
            actual,
            types.map(|types| types.iter().map(|ty| (*ty).to_owned()).collect())
        );
    }

    #[test]
    fn retains_composite_identities() {
        let sql = r#"
            ALTER EXTENSION ext ADD CAST ("App"."Source" AS "App"."Target");
            ALTER EXTENSION ext DROP TRANSFORM FOR "App"."T" LANGUAGE "Some Language";
            ALTER EXTENSION ext ADD OPERATOR FAMILY "App"."Ops" USING "Some Method";
        "#;
        let mut statements = parse_statements(sql, Dialect::postgres());
        let StatementNode::ExtensionMembership(cast) =
            statements.next().unwrap().unwrap().into_test_node()
        else {
            panic!("expected cast membership");
        };
        let MemberTarget::Object(cast) = cast.member.target else {
            panic!("expected a cast target");
        };
        let cast = cast.cast.unwrap();
        assert_eq!(cast.source.name.0, ["App", "Source"]);
        assert_eq!(cast.target.name.0, ["App", "Target"]);

        let StatementNode::ExtensionMembership(transform) =
            statements.next().unwrap().unwrap().into_test_node()
        else {
            panic!("expected transform membership");
        };
        let MemberTarget::Object(transform) = transform.member.target else {
            panic!("expected a transform target");
        };
        let transform = transform.transform.unwrap();
        assert_eq!(transform.type_name.name.0, ["App", "T"]);
        assert_eq!(transform.language, "Some Language");

        let StatementNode::ExtensionMembership(family) =
            statements.next().unwrap().unwrap().into_test_node()
        else {
            panic!("expected operator family membership");
        };
        let MemberTarget::AccessMethodObject(family) = family.member.target else {
            panic!("expected an access method and object name");
        };
        assert_eq!(family.name.0, ["App", "Ops"]);
        assert_eq!(family.method, "Some Method");
    }
}
