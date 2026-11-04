//! Context-free PostgreSQL rule syntax conversion, without writable-view analysis.

use crate::ast::{DropBehavior, DropRule, postgres::required};

use crate::{
    ast::{
        CreateRule, Expr, Name, Relation, RuleEvent, StatementNode, TryFromWithTokens,
        TryFromWithTokensOpt,
    },
    error::AnalysisError,
    syn,
    syn::{ListOpt, Opt},
};

use ::sql_fun_core::IVec;

impl TryFromWithTokens<&syn::RuleStmt> for CreateRule {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::RuleStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let relation = required(value.get_relation().as_inner(), "rule target")?;
        Ok(Self {
            name: Name::from_parts(&[value.get_rulename()]),
            relation: Relation::try_from(&relation)?,
            replace: value.get_replace(),
            event: RuleEvent::try_from(value.get_event().as_inner())?,
            predicate: Expr::try_from_with_tokens_opt(value.get_where_clause(), tokens)?,
            instead: value.get_instead(),
            actions: Self::actions(value.get_actions(), tokens)?,
        })
    }
}

#[cfg(test)]
mod test_rule_conversion {
    use crate::test_helpers::rule::postgres_rule_syntax;

    use crate::test_helpers::rule::rule;

    use crate::{
        ast::{CreateRule, RuleEvent, TryFromWithTokens},
        syn,
        syn::Opt,
    };

    #[rstest::rstest]
    #[case::select("SELECT", RuleEvent::select())]
    #[case::insert("INSERT", RuleEvent::insert())]
    #[case::update("UPDATE", RuleEvent::update())]
    #[case::delete("DELETE", RuleEvent::delete())]
    fn preserves_every_event_without_catalog(#[case] event: &str, #[case] expected: RuleEvent) {
        let rule = rule(&format!(
            "CREATE RULE MiXeD AS ON {event} TO absent DO NOTHING"
        ));
        assert_eq!(rule.name.0, ["mixed"]);
        assert_eq!(rule.relation.name.0, ["absent"]);
        assert_eq!(rule.event, expected);
        assert!(!rule.replace);
        assert!(!rule.instead);
        assert!(rule.predicate.is_none());
        assert!(rule.actions.is_empty());
    }

    #[rstest::rstest]
    #[case::nothing("NOTHING")]
    #[case::empty_list("()")]
    #[case::empty_statements("(;;)")]
    fn retains_parser_no_action_forms(#[case] body: &str) {
        let sql = format!("CREATE RULE r AS ON DELETE TO t DO INSTEAD {body}");
        let frontend = postgres_rule_syntax(&sql);
        assert!(frontend.actions.is_empty());
        assert!(rule(&sql).actions.is_empty());
    }

    #[rstest::rstest]
    #[case::unspecified(0)]
    #[case::non_event(::pg_query::protobuf::CmdType::CmdUtility as i32)]
    #[case::invalid(999)]
    #[should_panic(expected = "CREATE RULE event")]
    fn rejects_invalid_frontend_events(#[case] event: i32) {
        let mut syntax = postgres_rule_syntax("CREATE RULE r AS ON INSERT TO t DO NOTHING");
        syntax.event = event;
        let _ = CreateRule::try_from_with_tokens(&syn::RuleStmt::from(syntax), &Default::default());
    }

    #[test]
    fn rejects_missing_target() {
        let mut syntax = postgres_rule_syntax("CREATE RULE r AS ON INSERT TO t DO NOTHING");
        syntax.relation = None;
        let result =
            CreateRule::try_from_with_tokens(&syn::RuleStmt::from(syntax), &Default::default());
        assert!(result.is_err());
    }

    #[rstest::rstest]
    #[case::minimal("CREATE RULE r AS ON SELECT TO missing DO NOTHING")]
    #[case::full(r#"CREATE OR REPLACE RULE "R" AS ON UPDATE TO "C"."S"."T" WHERE NEW.x > OLD.x DO INSTEAD (SELECT NEW.x; DELETE FROM absent WHERE x=OLD.x RETURNING x)"#)]
    fn frontend_fields_agree_with_common_ast(#[case] sql: &str) {
        let frontend = postgres_rule_syntax(sql);
        let common = rule(sql);
        let relation = frontend.relation.expect("frontend relation");
        let mut name = Vec::new();
        if !relation.catalogname.is_empty() {
            name.push(relation.catalogname);
        }
        if !relation.schemaname.is_empty() {
            name.push(relation.schemaname);
        }
        name.push(relation.relname);
        assert_eq!(common.name.0, [frontend.rulename]);
        assert_eq!(common.relation.name.0, name);
        assert_eq!(common.relation.inherit, relation.inh);
        assert_eq!(common.relation.persistence, relation.relpersistence);
        assert_eq!(common.relation.alias.is_some(), relation.alias.is_some());
        assert_eq!(common.replace, frontend.replace);
        assert_eq!(common.instead, frontend.instead);
        assert_eq!(common.predicate.is_some(), frontend.where_clause.is_some());
        assert_eq!(common.actions.len(), frontend.actions.len());
        assert_eq!(
            common.event,
            RuleEvent::try_from(syn::CmdTypeOpt::from(frontend.event).as_inner()).expect("event")
        );
    }
}

#[cfg(test)]
mod tests;

impl CreateRule {
    fn actions(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<StatementNode>, AnalysisError> {
        let mut actions = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            if node.as_notify_stmt().as_inner().is_some() {
                // NOTIFY has no owned common statement yet; never discard an action.
                todo!("CREATE RULE NOTIFY action syntax conversion: #608");
            }
            actions.push(StatementNode::try_from_with_tokens(&node, tokens)?);
        }
        Ok(actions)
    }
}

#[cfg(test)]
mod test_action_boundary {

    use crate::test_helpers::rule::rule;

    #[test]
    #[should_panic(expected = "CREATE RULE NOTIFY action syntax conversion: #608")]
    fn notify_is_an_explicit_tracked_nested_boundary() {
        let _ = rule(
            "CREATE RULE r AS ON INSERT TO absent DO (SELECT NEW.id; NOTIFY channel, 'payload')",
        );
    }
}

impl RuleEvent {
    const fn select() -> Self {
        Self::Select
    }
    const fn insert() -> Self {
        Self::Insert
    }
    const fn update() -> Self {
        Self::Update
    }
    const fn delete() -> Self {
        Self::Delete
    }
}

impl TryFrom<Option<syn::CmdType>> for RuleEvent {
    type Error = AnalysisError;

    fn try_from(value: Option<syn::CmdType>) -> Result<Self, Self::Error> {
        match value {
            Some(syn::CmdType::CmdSelect) => Ok(Self::select()),
            Some(syn::CmdType::CmdInsert) => Ok(Self::insert()),
            Some(syn::CmdType::CmdUpdate) => Ok(Self::update()),
            Some(syn::CmdType::CmdDelete) => Ok(Self::delete()),
            _ => AnalysisError::raise_unexpected_input("CREATE RULE event"),
        }
    }
}

impl TryFrom<&syn::DropStmt> for DropRule {
    type Error = AnalysisError;

    fn try_from(value: &syn::DropStmt) -> Result<Self, Self::Error> {
        if !matches!(
            value.get_remove_type().as_inner(),
            Some(syn::ObjectType::ObjectRule)
        ) {
            return AnalysisError::raise_unexpected_input("DROP RULE object type");
        }
        if value.get_concurrent() {
            return AnalysisError::raise_unexpected_input("DROP RULE cannot be concurrent");
        }
        let objects = value.get_objects().as_inner().unwrap_or_default();
        let [object] = objects.as_slice() else {
            return AnalysisError::raise_unexpected_input("one DROP RULE target required");
        };
        let mut parts = Name::try_from(object.as_list().get_items())?.0;
        // PostgreSQL appends the rule identifier to the ON relation's name list.
        if parts.len() < 2 {
            return AnalysisError::raise_unexpected_input("DROP RULE relation and name");
        }
        let name = parts.pop().expect("checked DROP RULE target length");
        Ok(Self {
            name: Name::from_parts(&[name]),
            relation: Name::from_parts(&parts),
            if_exists: value.get_missing_ok(),
            behavior: DropBehavior::try_from(value.get_behavior().as_inner())?,
        })
    }
}

#[cfg(test)]
mod test_drop_rule_conversion {
    use crate::ast::{Dialect, DropBehavior, DropRule};

    use crate::syn;

    use crate::syn::Opt;

    use crate::test_helpers::drop_rule_syntax::drop_rule;

    use crate::test_helpers::drop_syntax::postgres_drop_syntax;

    #[rstest::rstest]
    #[case::bare("DROP RULE RuleName ON Items", &["items"], "rulename")]
    #[case::schema("DROP RULE t ON SchemaName.Items", &["schemaname", "items"], "t")]
    #[case::catalog(
    r#"DROP RULE "Rule.Name" ON "Catalog"."Schema"."Items""#,
    &["Catalog", "Schema", "Items"],
    "Rule.Name"
)]
    #[case::unresolved_qualification("DROP RULE t ON a.b.c.d", &["a", "b", "c", "d"], "t")]
    #[case::escaped_quotes(r#"DROP RULE "T""Name" ON "R""Name""#, &["R\"Name"], "T\"Name")]
    fn postgres_preserves_target_names(
        #[case] sql: &str,
        #[case] relation: &[&str],
        #[case] name: &str,
    ) {
        let rule = drop_rule(sql, Dialect::postgres());
        assert_eq!(rule.name.0, [name]);
        assert_eq!(rule.relation.0, relation);
    }

    #[rstest::rstest]
    #[case::implicit_restrict("", DropBehavior::restrict())]
    #[case::explicit_restrict(" RESTRICT", DropBehavior::restrict())]
    #[case::cascade(" CASCADE", DropBehavior::cascade())]
    fn postgres_preserves_drop_options(
        #[case] suffix: &str,
        #[case] behavior: DropBehavior,
        #[values(false, true)] if_exists: bool,
    ) {
        let clause = if if_exists { "IF EXISTS " } else { "" };
        let rule = drop_rule(
            &format!("DROP RULE {clause}t ON absent{suffix}"),
            Dialect::postgres(),
        );
        assert_eq!(rule.if_exists, if_exists);
        assert_eq!(rule.behavior, behavior);
    }

    #[rstest::rstest]
    #[case::trigger("DROP TRIGGER t ON r")]
    #[case::table("DROP TABLE r")]
    #[should_panic(expected = "DROP RULE object type")]
    fn rejects_other_drop_kinds(#[case] sql: &str) {
        let syntax = syn::DropStmt::from(postgres_drop_syntax(sql));
        let _ = DropRule::try_from(&syntax);
    }

    #[test]
    #[should_panic(expected = "DROP RULE cannot be concurrent")]
    fn rejects_contradictory_concurrent_flag() {
        let mut syntax = postgres_drop_syntax("DROP RULE t ON r");
        syntax.concurrent = true;
        let _ = DropRule::try_from(&syn::DropStmt::from(syntax));
    }

    #[rstest::rstest]
    #[case::empty(false)]
    #[case::multiple(true)]
    #[should_panic(expected = "one DROP RULE target required")]
    fn rejects_invalid_target_cardinality(#[case] multiple: bool) {
        let mut syntax = postgres_drop_syntax("DROP RULE t ON r");
        if multiple {
            syntax.objects.extend(syntax.objects.clone());
        } else {
            syntax.objects.clear();
        }
        let _ = DropRule::try_from(&syn::DropStmt::from(syntax));
    }

    #[test]
    #[should_panic(expected = "DROP RULE relation and name")]
    fn rejects_missing_relation_in_target() {
        let mut syntax = postgres_drop_syntax("DROP TABLE t");
        syntax.remove_type = ::pg_query::protobuf::ObjectType::ObjectRule as i32;
        let _ = DropRule::try_from(&syn::DropStmt::from(syntax));
    }

    #[test]
    #[should_panic(expected = "DROP RULE relation and name")]
    fn rejects_non_list_target() {
        let mut syntax = postgres_drop_syntax("DROP RULE t ON r");
        syntax.objects = vec![::pg_query::protobuf::Node::default()];
        let _ = DropRule::try_from(&syn::DropStmt::from(syntax));
    }

    #[rstest::rstest]
    #[case::undefined(0)]
    #[case::invalid(999)]
    #[should_panic(expected = "DROP behavior")]
    fn rejects_unspecified_behavior(#[case] behavior: i32) {
        let mut syntax = postgres_drop_syntax("DROP RULE t ON r");
        syntax.behavior = behavior;
        let _ = DropRule::try_from(&syn::DropStmt::from(syntax));
    }

    #[test]
    #[should_panic(expected = "DROP RULE object type")]
    fn rejects_unspecified_object_type() {
        let mut syntax = postgres_drop_syntax("DROP RULE t ON r");
        syntax.remove_type = 0;
        let _ = DropRule::try_from(&syn::DropStmt::from(syntax));
    }

    #[test]
    fn rejects_non_string_name_component() {
        let mut syntax = postgres_drop_syntax("DROP RULE t ON r");
        let Some(::pg_query::protobuf::node::Node::List(list)) = &mut syntax.objects[0].node else {
            panic!("DROP RULE name list");
        };
        list.items[0] = ::pg_query::protobuf::Node {
            node: Some(::pg_query::protobuf::node::Node::Integer(
                ::pg_query::protobuf::Integer { ival: 1 },
            )),
        };
        assert!(DropRule::try_from(&syn::DropStmt::from(syntax)).is_err());
    }

    #[rstest::rstest]
    #[case::minimal("DROP RULE r ON absent")]
    #[case::full(r#"DROP RULE IF EXISTS "Rule.Name" ON "Catalog"."Schema"."Relation" CASCADE"#)]
    fn frontend_fields_agree_with_common_ast(#[case] sql: &str) {
        let frontend = postgres_drop_syntax(sql);
        let common = drop_rule(sql, Dialect::postgres());
        assert_eq!(frontend.objects.len(), 1);
        let Some(::pg_query::protobuf::node::Node::List(list)) = &frontend.objects[0].node else {
            panic!("frontend name list");
        };
        let mut parts = Vec::new();
        for item in &list.items {
            let Some(::pg_query::protobuf::node::Node::String(value)) = &item.node else {
                panic!("frontend name component");
            };
            parts.push(value.sval.clone());
        }
        assert_eq!(common.name.0, [parts.pop().expect("rule name")]);
        assert_eq!(common.relation.0, parts);
        assert_eq!(common.if_exists, frontend.missing_ok);
        assert_eq!(
            common.behavior,
            DropBehavior::try_from(syn::DropBehaviorOpt::from(frontend.behavior).as_inner())
                .expect("drop behavior"),
        );
        assert_eq!(
            frontend.remove_type,
            ::pg_query::protobuf::ObjectType::ObjectRule as i32
        );
        assert!(!frontend.concurrent);
    }
}
