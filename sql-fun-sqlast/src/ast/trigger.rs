//! CREATE/DROP TRIGGER syntax conversion, without catalog access.

#[cfg(feature = "sqlite")]
use crate::ast::StatementNode as AstStatementNode;
use crate::ast::{DropBehavior, DropTrigger, postgres::required};

use crate::{
    ast::{
        CreateTrigger, Expr, Name, Relation, TriggerBody, TriggerConstraint, TriggerEvent,
        TriggerFunction, TriggerGranularity, TriggerTiming, TriggerTransition, TryFromWithTokens,
        TryFromWithTokensOpt,
    },
    sem::AnalysisError,
    syn,
    syn::{ListOpt, Opt},
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
mod drop_tests;

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
