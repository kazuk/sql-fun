use crate::{
    ast::{Query, StatementNode},
    context::ParseContext,
    names::FullName,
    sem::{
        CreateRule as SemCreateRule, CreateTrigger as SemCreateTrigger, DeleteStatement,
        InsertStatement, ResolutionFailure, RuleIdentity, ScalarAnalysisContext, SelectStatement,
        SemAst, UpdateStatement,
    },
};

impl SemAst {
    pub(crate) const fn ast_rule_query(ast: &StatementNode) -> Option<&Query> {
        if let StatementNode::Query(query) = ast {
            Some(query)
        } else {
            None
        }
    }
}

impl SemAst {
    pub(crate) fn validate_ast_select_rule_ctes(
        ast: &StatementNode,
        identity: &RuleIdentity,
    ) -> Result<(), ResolutionFailure> {
        if let StatementNode::Query(query) = ast {
            SelectStatement::validate_ast_select_rule_ctes(query, identity)?;
        }
        Ok(())
    }
}

impl SemAst {
    pub(crate) fn from_ast_rule_action(
        ast: &StatementNode,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        identity: &RuleIdentity,
    ) -> Result<Self, ResolutionFailure> {
        match ast {
            StatementNode::Query(query) => {
                SelectStatement::from_ast_in(query, context, Some(scope))
                    .map(|query| Self::select(&query))
                    .map_err(Into::into)
            }
            StatementNode::Insert(insert) => {
                InsertStatement::from_ast_in(insert, context, Some(scope))
                    .map(|value| {
                        value
                            .as_ref()
                            .map_or_else(Self::unresolved_statement, Self::insert)
                    })
                    .map_err(Into::into)
            }
            StatementNode::Update(update) => {
                UpdateStatement::from_ast_in(update, context, Some(scope))
                    .map(|value| {
                        value
                            .as_ref()
                            .map_or_else(Self::unresolved_statement, Self::update)
                    })
                    .map_err(Into::into)
            }
            StatementNode::Delete(delete) => {
                DeleteStatement::from_ast_in(delete, context, Some(scope))
                    .map(|delete| Self::delete(&delete))
                    .map_err(Into::into)
            }
            _ => SemCreateRule::ast_invalid(
                identity,
                "unsupported rule action; expected a common query or INSERT/UPDATE/DELETE statement",
            ),
        }
    }
}

impl SemAst {
    pub(crate) fn from_ast_trigger_command(
        ast: &StatementNode,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, ResolutionFailure> {
        match ast {
            StatementNode::Query(query) => {
                SelectStatement::from_ast_in(query, context, Some(scope))
                    .map(|statement| Self::select(&statement))
                    .map_err(Into::into)
            }
            StatementNode::Insert(insert) => {
                InsertStatement::from_ast_trigger_command(insert, context, scope)
            }
            StatementNode::Update(update) => {
                UpdateStatement::from_ast_trigger_command(update, context, scope)
            }
            StatementNode::Delete(delete) => {
                DeleteStatement::from_ast_trigger_command(delete, context, scope)
            }
            _ => SemCreateTrigger::ast_invalid(
                &FullName::from_local_name("trigger"),
                "illegal SQLite trigger body command",
            ),
        }
    }
}
