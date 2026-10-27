//! Context-free PostgreSQL rule syntax conversion, without writable-view analysis.

use crate::ast::{
    CreateRule, Expr, Name, Relation, RuleEvent, StatementNode, TryFromWithTokens,
    TryFromWithTokensOpt,
};
use crate::sem::AnalysisError;
use crate::syn;
use crate::syn::{ListOpt, Opt};

use ::sql_fun_core::IVec;

impl TryFromWithTokens<&syn::RuleStmt> for CreateRule {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::RuleStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let relation = super::postgres::required(value.get_relation().as_inner(), "rule target")?;
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

impl TryFrom<&syn::DropStmt> for super::DropRule {
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
            behavior: super::DropBehavior::try_from(value.get_behavior().as_inner())?,
        })
    }
}

#[cfg(test)]
mod drop_tests;
