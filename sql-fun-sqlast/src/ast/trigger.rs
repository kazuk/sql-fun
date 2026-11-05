//! CREATE/DROP TRIGGER syntax conversion, without catalog access.

#[cfg(feature = "sqlite")]
use crate::ast::StatementNode as AstStatementNode;
use crate::{
    ast::{
        CreateTrigger, DropBehavior, DropTrigger, Expr, Name, Relation, TriggerBody,
        TriggerConstraint, TriggerEvent, TriggerFunction, TriggerGranularity, TriggerTiming,
        TriggerTransition, TryFromWithTokens, TryFromWithTokensOpt, postgres::required,
    },
    error::AnalysisError,
    syn::{self, ListOpt, Opt},
};

use ::sql_fun_core::IVec;

impl TryFromWithTokens<&syn::CreateTrigStmt> for CreateTrigger {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::CreateTrigStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let relation = required(value.get_relation().as_inner(), "trigger target")?;
        let mut transitions = Vec::new();
        for node in value.get_transition_rels().as_inner().unwrap_or_default() {
            let transition = required(
                node.as_trigger_transition().as_inner(),
                "trigger transition relation",
            )?;
            transitions.push(TriggerTransition::from(&transition));
        }
        let constraint = if value.get_isconstraint() {
            Some(TriggerConstraint::try_from(value)?)
        } else {
            None
        };
        Ok(Self {
            name: Name::from_parts(&[value.get_trigname()]),
            relation: Relation::try_from(&relation)?,
            replace: value.get_replace(),
            if_not_exists: false,
            temporary: false,
            timing: Some(TriggerTiming::try_from(value.get_timing())?),
            events: TriggerEvent::postgres(value)?,
            granularity: Some(TriggerGranularity::postgres(value.get_row())),
            predicate: Expr::try_from_with_tokens_opt(value.get_when_clause(), tokens)?,
            constraint,
            transitions,
            body: TriggerBody::function(TriggerFunction::try_from(value)?),
        })
    }
}

#[cfg(test)]
mod test_postgres_trigger_conversion {
    use crate::{
        ast::{Dialect, Expr, TriggerBody, TriggerEvent, TriggerGranularity, TriggerTiming},
        test_helpers::trigger::trigger,
    };

    #[test]
    fn postgres_preserves_identity_flags_and_function_arguments() {
        let trigger = trigger(
            r#"CREATE OR REPLACE TRIGGER "T" BEFORE INSERT OR UPDATE OF "Value", other
        ON "Catalog"."Schema"."Items" FOR EACH ROW
        WHEN (NEW."Value" > 0) EXECUTE FUNCTION "Fns"."Run"('a''b', 42, identifier)"#,
            Dialect::postgres(),
        );
        assert_eq!(trigger.name.0, ["T"]);
        assert_eq!(trigger.relation.name.0, ["Catalog", "Schema", "Items"]);
        assert!(trigger.replace);
        assert!(!trigger.if_not_exists);
        assert!(!trigger.temporary);
        assert_eq!(trigger.timing, Some(TriggerTiming::Before));
        assert_eq!(trigger.granularity, Some(TriggerGranularity::Row));
        assert_eq!(
            trigger.events,
            [
                TriggerEvent::Insert,
                TriggerEvent::Update(vec!["Value".into(), "other".into()])
            ]
        );
        assert!(matches!(trigger.predicate, Some(Expr::Operator(_))));
        assert!(trigger.constraint.is_none());
        assert!(trigger.transitions.is_empty());
        let TriggerBody::Function(function) = trigger.body else {
            panic!("function body")
        };
        assert_eq!(function.name.0, ["Fns", "Run"]);
        assert_eq!(function.arguments, ["a'b", "42", "identifier"]);
    }

    #[rstest::rstest]
    #[case::after_omitted_level(
        "AFTER INSERT",
        "",
        TriggerTiming::After,
        TriggerGranularity::Statement
    )]
    #[case::before_statement(
        "BEFORE DELETE",
        "FOR EACH STATEMENT",
        TriggerTiming::Before,
        TriggerGranularity::Statement
    )]
    #[case::instead_row(
        "INSTEAD OF UPDATE",
        "FOR EACH ROW",
        TriggerTiming::InsteadOf,
        TriggerGranularity::Row
    )]
    fn postgres_timing_and_granularity(
        #[case] event: &str,
        #[case] level: &str,
        #[case] timing: TriggerTiming,
        #[case] granularity: TriggerGranularity,
    ) {
        let trigger = trigger(
            &format!("CREATE TRIGGER t {event} ON absent {level} EXECUTE PROCEDURE f()"),
            Dialect::postgres(),
        );
        assert_eq!(trigger.timing, Some(timing));
        assert_eq!(trigger.granularity, Some(granularity));
        let TriggerBody::Function(function) = trigger.body else {
            panic!("function body")
        };
        assert!(function.arguments.is_empty());
    }

    #[rstest::rstest]
    #[case::table("TABLE", true)]
    #[case::row("ROW", false)]
    fn postgres_preserves_all_events_and_transition_relations(
        #[case] relation_kind: &str,
        #[case] table: bool,
    ) {
        let trigger = trigger(
            &format!(
                r#"CREATE TRIGGER t AFTER INSERT OR DELETE OR UPDATE OR TRUNCATE ON missing
        REFERENCING OLD {relation_kind} AS "Before" NEW {relation_kind} AS "After"
        FOR EACH STATEMENT EXECUTE FUNCTION f()"#
            ),
            Dialect::postgres(),
        );
        assert_eq!(
            trigger.events,
            [
                TriggerEvent::Insert,
                TriggerEvent::Delete,
                TriggerEvent::Update(vec![]),
                TriggerEvent::Truncate
            ]
        );
        assert_eq!(trigger.transitions.len(), 2);
        assert_eq!(trigger.transitions[0].name, "Before");
        assert!(!trigger.transitions[0].new);
        assert_eq!(trigger.transitions[0].table, table);
        assert_eq!(trigger.transitions[1].name, "After");
        assert!(trigger.transitions[1].new);
        assert_eq!(trigger.transitions[1].table, table);
    }

    #[rstest::rstest]
    #[case::default("", "", false, false, false)]
    #[case::not_deferrable("", "NOT DEFERRABLE INITIALLY IMMEDIATE", false, false, false)]
    #[case::deferred(
        "FROM public.source",
        "DEFERRABLE INITIALLY DEFERRED",
        true,
        true,
        true
    )]
    #[case::immediate(
        "FROM public.source",
        "DEFERRABLE INITIALLY IMMEDIATE",
        true,
        true,
        false
    )]
    fn postgres_constraint_options(
        #[case] from: &str,
        #[case] options: &str,
        #[case] has_from: bool,
        #[case] deferrable: bool,
        #[case] deferred: bool,
    ) {
        let trigger = trigger(
            &format!(
                "CREATE CONSTRAINT TRIGGER t AFTER UPDATE ON public.missing {from} {options} FOR EACH ROW WHEN (NEW.id IS NOT NULL) EXECUTE FUNCTION f()"
            ),
            Dialect::postgres(),
        );
        let constraint = trigger.constraint.expect("constraint syntax");
        assert_eq!(constraint.relation.is_some(), has_from);
        if let Some(relation) = constraint.relation {
            assert_eq!(relation.name.0, ["public", "source"]);
        }
        assert_eq!(constraint.deferrable, deferrable);
        assert_eq!(constraint.initially_deferred, deferred);
        assert!(trigger.predicate.is_some());
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod tests;

impl TryFrom<i32> for TriggerTiming {
    type Error = AnalysisError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        // libpg_query's PostgreSQL trigger bit definitions (catalog/pg_trigger.h).
        match value {
            0 => Ok(Self::After),
            2 => Ok(Self::Before),
            64 => Ok(Self::InsteadOf),
            _ => AnalysisError::raise_unexpected_input("trigger timing bits"),
        }
    }
}

impl TriggerGranularity {
    const fn postgres(row: bool) -> Self {
        if row { Self::Row } else { Self::Statement }
    }
}

impl TriggerEvent {
    fn postgres(value: &syn::CreateTrigStmt) -> Result<Vec<Self>, AnalysisError> {
        let mut events = Vec::new();
        let bits = value.get_events();
        // The parser stores an event bitset, so source event ordering is unavailable.
        if bits & 4 != 0 {
            events.push(Self::insert());
        }
        if bits & 8 != 0 {
            events.push(Self::delete());
        }
        if bits & 16 != 0 {
            events.push(Self::update(Name::try_from(value.get_columns())?.0));
        }
        if bits & 32 != 0 {
            events.push(Self::truncate());
        }
        if bits == 0 || bits & !(4 | 8 | 16 | 32) != 0 {
            return AnalysisError::raise_unexpected_input("trigger event bits");
        }
        Ok(events)
    }

    const fn insert() -> Self {
        Self::Insert
    }
    const fn update(columns: Vec<String>) -> Self {
        Self::Update(columns)
    }
    const fn delete() -> Self {
        Self::Delete
    }
    const fn truncate() -> Self {
        Self::Truncate
    }
}

impl TryFrom<&syn::CreateTrigStmt> for TriggerConstraint {
    type Error = AnalysisError;

    fn try_from(value: &syn::CreateTrigStmt) -> Result<Self, Self::Error> {
        let relation = value.get_constrrel().as_inner();
        Ok(Self {
            relation: relation.as_ref().map(Relation::try_from).transpose()?,
            deferrable: value.get_deferrable(),
            initially_deferred: value.get_initdeferred(),
        })
    }
}

impl From<&syn::TriggerTransition> for TriggerTransition {
    fn from(value: &syn::TriggerTransition) -> Self {
        Self {
            name: value.get_name(),
            new: value.get_is_new(),
            table: value.get_is_table(),
        }
    }
}

impl TryFrom<&syn::CreateTrigStmt> for TriggerFunction {
    type Error = AnalysisError;

    fn try_from(value: &syn::CreateTrigStmt) -> Result<Self, Self::Error> {
        Ok(Self {
            name: Name::try_from(value.get_funcname())?,
            // Trigger arguments are strings in the raw parse tree, not evaluated Exprs.
            arguments: Self::arguments(value.get_args())?,
        })
    }
}

impl TriggerBody {
    const fn function(value: TriggerFunction) -> Self {
        Self::Function(value)
    }

    #[cfg(feature = "sqlite")]
    const fn commands(value: Vec<AstStatementNode>) -> Self {
        Self::Commands(value)
    }
}

#[cfg(feature = "sqlite")]
impl TryFrom<&syn::sqlite::CreateTriggerStatement> for CreateTrigger {
    type Error = AnalysisError;

    fn try_from(value: &syn::sqlite::CreateTriggerStatement) -> Result<Self, Self::Error> {
        use crate::ast::{StatementNode, TryFromList, TryFromOpt};

        Ok(Self {
            name: Name::from(value.trigger_name()),
            relation: Relation::sqlite_target(value.table_name(), None),
            replace: false,
            if_not_exists: *value.if_not_exists(),
            temporary: *value.temporary(),
            timing: value.time().as_ref().map(TriggerTiming::from),
            events: vec![TriggerEvent::from(value.event())],
            // False means the clause was omitted, not statement-level execution.
            granularity: if *value.for_each_row() {
                Some(TriggerGranularity::Row)
            } else {
                None
            },
            predicate: Expr::try_from_opt(value.when_clause().as_deref())?,
            constraint: None,
            transitions: Vec::new(),
            body: TriggerBody::commands(StatementNode::try_from_list(value.commands())?),
        })
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod test_sqlite_trigger_conversion {
    use crate::{
        ast::{Dialect, TriggerEvent, TriggerGranularity, TriggerTiming},
        test_helpers::trigger::trigger,
    };

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::omitted("", "", None, None)]
    #[case::before(
        "BEFORE",
        "FOR EACH ROW",
        Some(TriggerTiming::Before),
        Some(TriggerGranularity::Row)
    )]
    #[case::after("AFTER", "", Some(TriggerTiming::After), None)]
    #[case::instead(
        "INSTEAD OF",
        "FOR EACH ROW",
        Some(TriggerTiming::InsteadOf),
        Some(TriggerGranularity::Row)
    )]
    fn sqlite_preserves_creation_and_optional_clauses(
        #[case] time: &str,
        #[case] level: &str,
        #[case] timing: Option<TriggerTiming>,
        #[case] granularity: Option<TriggerGranularity>,
    ) {
        let trigger = trigger(
            &format!(
                r#"CREATE TEMP TRIGGER IF NOT EXISTS "temp"."T" {time} UPDATE OF "Value", other ON "main"."Missing" {level} WHEN NEW."Value" > 0 BEGIN SELECT 1; END"#
            ),
            Dialect::sqlite(),
        );
        assert_eq!(trigger.name.0, ["temp", "T"]);
        assert_eq!(trigger.relation.name.0, ["main", "Missing"]);
        assert!(trigger.temporary);
        assert!(trigger.if_not_exists);
        assert!(!trigger.replace);
        assert_eq!(trigger.timing, timing);
        assert_eq!(trigger.granularity, granularity);
        assert_eq!(
            trigger.events,
            [TriggerEvent::Update(vec!["Value".into(), "other".into()])]
        );
        assert!(trigger.predicate.is_some());
        assert!(trigger.constraint.is_none());
        assert!(trigger.transitions.is_empty());
    }
}

#[cfg(feature = "sqlite")]
impl From<&syn::sqlite::TriggerTime> for TriggerTiming {
    fn from(value: &syn::sqlite::TriggerTime) -> Self {
        match value {
            syn::sqlite::TriggerTime::Before => Self::Before,
            syn::sqlite::TriggerTime::After => Self::After,
            syn::sqlite::TriggerTime::InsteadOf => Self::InsteadOf,
        }
    }
}

#[cfg(feature = "sqlite")]
impl From<&syn::sqlite::TriggerEvent> for TriggerEvent {
    fn from(value: &syn::sqlite::TriggerEvent) -> Self {
        match value {
            syn::sqlite::TriggerEvent::Insert => Self::insert(),
            syn::sqlite::TriggerEvent::Delete => Self::delete(),
            syn::sqlite::TriggerEvent::Update => Self::update(Vec::new()),
            syn::sqlite::TriggerEvent::UpdateOf(columns) => {
                Self::update(Self::sqlite_columns(columns))
            }
        }
    }
}

#[cfg(feature = "sqlite")]
impl TriggerEvent {
    fn sqlite_columns(columns: &[syn::sqlite::Name]) -> Vec<String> {
        let mut names = Vec::new();
        for column in columns {
            names.push(column.value().to_owned());
        }
        names
    }
}

impl TriggerFunction {
    fn arguments(value: syn::NodeListOpt) -> Result<Vec<String>, AnalysisError> {
        let mut arguments = Vec::new();
        for node in value.as_inner().unwrap_or_default() {
            arguments.push(required(node.as_string().get_sval(), "trigger argument")?);
        }
        Ok(arguments)
    }
}

impl TryFrom<&syn::DropStmt> for DropTrigger {
    type Error = AnalysisError;

    fn try_from(value: &syn::DropStmt) -> Result<Self, Self::Error> {
        if !matches!(
            value.get_remove_type().as_inner(),
            Some(syn::ObjectType::ObjectTrigger)
        ) {
            return AnalysisError::raise_unexpected_input("DROP TRIGGER object type");
        }
        if value.get_concurrent() {
            return AnalysisError::raise_unexpected_input("DROP TRIGGER cannot be concurrent");
        }
        let objects = value.get_objects().as_inner().unwrap_or_default();
        let [object] = objects.as_slice() else {
            return AnalysisError::raise_unexpected_input("one DROP TRIGGER target required");
        };
        let mut parts = Name::try_from(object.as_list().get_items())?.0;
        // The raw parser appends the trigger identifier to the ON relation's name list.
        if parts.len() < 2 {
            return AnalysisError::raise_unexpected_input("DROP TRIGGER relation and name");
        }
        let name = parts.pop().expect("checked DROP TRIGGER target length");
        Ok(Self {
            name: Name::from_parts(&[name]),
            relation: Some(Name::from_parts(&parts)),
            if_exists: value.get_missing_ok(),
            behavior: Some(DropBehavior::try_from(value.get_behavior().as_inner())?),
        })
    }
}

#[cfg(test)]
mod test_postgres_drop_conversion {
    use crate::{
        ast::{Dialect, DropBehavior, DropTrigger},
        syn,
        test_helpers::{drop_syntax::postgres_drop_syntax, drop_trigger_syntax::drop_trigger},
    };

    #[rstest::rstest]
    #[case::bare("DROP TRIGGER TriggerName ON Items", &["items"], "triggername")]
    #[case::schema("DROP TRIGGER t ON SchemaName.Items", &["schemaname", "items"], "t")]
    #[case::catalog(
    r#"DROP TRIGGER "Trigger.Name" ON "Catalog"."Schema"."Items""#,
    &["Catalog", "Schema", "Items"],
    "Trigger.Name"
)]
    #[case::unresolved_qualification("DROP TRIGGER t ON a.b.c.d", &["a", "b", "c", "d"], "t")]
    #[case::escaped_quotes(r#"DROP TRIGGER "T""Name" ON "R""Name""#, &["R\"Name"], "T\"Name")]
    fn postgres_preserves_target_names(
        #[case] sql: &str,
        #[case] relation: &[&str],
        #[case] name: &str,
    ) {
        let trigger = drop_trigger(sql, Dialect::postgres());
        assert_eq!(trigger.name.0, [name]);
        assert_eq!(trigger.relation.expect("explicit ON relation").0, relation);
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
        let trigger = drop_trigger(
            &format!("DROP TRIGGER {clause}t ON absent{suffix}"),
            Dialect::postgres(),
        );
        assert_eq!(trigger.if_exists, if_exists);
        assert_eq!(trigger.behavior, Some(behavior));
    }

    #[test]
    #[should_panic(expected = "DROP TRIGGER object type")]
    fn rejects_other_drop_kinds() {
        let syntax = syn::DropStmt::from(postgres_drop_syntax("DROP RULE t ON r"));
        let _ = DropTrigger::try_from(&syntax);
    }

    #[test]
    #[should_panic(expected = "DROP TRIGGER cannot be concurrent")]
    fn rejects_contradictory_concurrent_flag() {
        let mut syntax = postgres_drop_syntax("DROP TRIGGER t ON r");
        syntax.concurrent = true;
        let _ = DropTrigger::try_from(&syn::DropStmt::from(syntax));
    }

    #[rstest::rstest]
    #[case::empty(false)]
    #[case::multiple(true)]
    #[should_panic(expected = "one DROP TRIGGER target required")]
    fn rejects_invalid_target_cardinality(#[case] multiple: bool) {
        let mut syntax = postgres_drop_syntax("DROP TRIGGER t ON r");
        if multiple {
            syntax.objects.extend(syntax.objects.clone());
        } else {
            syntax.objects.clear();
        }
        let _ = DropTrigger::try_from(&syn::DropStmt::from(syntax));
    }

    #[test]
    #[should_panic(expected = "DROP TRIGGER relation and name")]
    fn rejects_missing_relation_in_target() {
        let mut syntax = postgres_drop_syntax("DROP TABLE t");
        syntax.remove_type = ::pg_query::protobuf::ObjectType::ObjectTrigger as i32;
        let _ = DropTrigger::try_from(&syn::DropStmt::from(syntax));
    }

    #[test]
    #[should_panic(expected = "DROP TRIGGER relation and name")]
    fn rejects_non_list_target() {
        let mut syntax = postgres_drop_syntax("DROP TRIGGER t ON r");
        syntax.objects = vec![::pg_query::protobuf::Node::default()];
        let _ = DropTrigger::try_from(&syn::DropStmt::from(syntax));
    }

    #[test]
    #[should_panic(expected = "DROP behavior")]
    fn rejects_unspecified_behavior() {
        let mut syntax = postgres_drop_syntax("DROP TRIGGER t ON r");
        syntax.behavior = 0;
        let _ = DropTrigger::try_from(&syn::DropStmt::from(syntax));
    }
}

#[cfg(feature = "sqlite")]
impl TryFrom<&syn::sqlite::DropTriggerStatement> for DropTrigger {
    type Error = AnalysisError;

    fn try_from(value: &syn::sqlite::DropTriggerStatement) -> Result<Self, Self::Error> {
        if value.trigger_name().alias().is_some() {
            return AnalysisError::raise_unexpected_input("DROP TRIGGER cannot have an alias");
        }
        Ok(Self {
            name: Name::from(value.trigger_name()),
            relation: None,
            if_exists: *value.if_exists(),
            behavior: None,
        })
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod test_sqlite_drop_conversion {
    use crate::{
        ast::{Dialect, DropBehavior, DropTrigger},
        syn,
        test_helpers::drop_trigger_syntax::drop_trigger,
    };

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::bare("TriggerName", &["TriggerName"])]
    #[case::qualified("main.TriggerName", &["main", "TriggerName"])]
    #[case::quoted(r#""Database"."Trigger.Name""#, &["Database", "Trigger.Name"])]
    #[case::escaped_quotes(r#""T""Name""#, &["T\"Name"])]
    fn sqlite_preserves_names_without_inventing_relation_or_behavior(
        #[case] target: &str,
        #[case] names: &[&str],
        #[values(false, true)] if_exists: bool,
    ) {
        let clause = if if_exists { "IF EXISTS " } else { "" };
        let trigger = drop_trigger(&format!("DROP TRIGGER {clause}{target}"), Dialect::sqlite());
        assert_eq!(trigger.name.0, names);
        assert_eq!(trigger.if_exists, if_exists);
        assert!(trigger.relation.is_none());
        assert!(trigger.behavior.is_none());
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn equivalent_concepts_use_common_shapes() {
        let postgres = drop_trigger(
            r#"DROP TRIGGER IF EXISTS "T" ON absent"#,
            Dialect::postgres(),
        );
        let sqlite = drop_trigger(r#"DROP TRIGGER IF EXISTS "T""#, Dialect::sqlite());
        assert_eq!(postgres.name, sqlite.name);
        assert_eq!(postgres.if_exists, sqlite.if_exists);
        assert!(postgres.relation.is_some());
        assert!(sqlite.relation.is_none());
        assert_eq!(postgres.behavior, Some(DropBehavior::restrict()));
        assert!(sqlite.behavior.is_none());
    }

    #[cfg(feature = "sqlite")]
    #[test]
    #[should_panic(expected = "DROP TRIGGER cannot have an alias")]
    fn rejects_contradictory_sqlite_alias() {
        let mut name = ::turso_parser::ast::QualifiedName::single(
            ::turso_parser::ast::Name::exact("t".to_owned()),
        );
        name.alias = Some(::turso_parser::ast::Name::exact("alias".to_owned()));
        let syntax =
            syn::sqlite::DropTriggerStatement::new(false, syn::sqlite::QualifiedName::from(name));
        let _ = DropTrigger::try_from(&syntax);
    }
}
