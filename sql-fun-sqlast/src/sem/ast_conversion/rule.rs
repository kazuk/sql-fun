//! PostgreSQL rule analysis over common syntax only.

use crate::names::FullName;
use crate::sem::CteStatement;
use crate::sem::DeleteStatement;
use crate::sem::InsertStatement;
use crate::sem::SelectStatement;
use crate::sem::UpdateStatement;
use crate::sem::ViewColumnLineage;

use crate::{
    AstAndContextPair,
    ast::{
        CreateRule, CteBody, Dialect, Expr, Literal, Query, RuleEvent, StatementNode, SubqueryKind,
    },
    context::ParseContext,
    names::{TableName, ViewName},
    sem::{
        AliasSpec, AnalysisError, CreateRule as SemCreateRule, DataSource, FromClause,
        PgBuiltInType, ResolutionFailure, ResultColumn, ResultColumnCollection,
        RuleEvent as SemRuleEvent, RuleIdentity, ScalarAnalysisContext, SemAst, SemScalarExpr,
        TypeReference, ViewLike, WithClause, problem::RuleProblem, traversal::SemanticVisitorMut,
    },
};

impl SemCreateRule {
    pub(crate) fn from_ast(
        ast: &CreateRule,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_definition(ast, context)
            .and_then(|definition| Self::validate_ast_resolved(context, definition));
        let result = ResolutionFailure::finish(result, context);
        context.finish_analysis(result)
    }
}

impl SemCreateRule {
    pub(crate) fn validate_ast_resolved(
        context: &impl ParseContext,
        definition: Self,
    ) -> Result<Self, ResolutionFailure> {
        if context.parameter_inference_needed() {
            return Self::ast_invalid(
                definition.identity(),
                "rule definitions cannot contain bind parameters",
            );
        }
        if context.analysis_state().has_problems() {
            return Self::ast_invalid(
                definition.identity(),
                "rule contains unresolved or invalid constructs",
            );
        }
        Ok(definition)
    }
}

impl SemCreateRule {
    pub(crate) fn apply_ast<C: ParseContext>(
        ast: &CreateRule,
        mut context: C,
    ) -> Result<AstAndContextPair<C>, AnalysisError> {
        let Some(definition) = Self::from_ast(ast, &mut context)? else {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        };
        let context = definition.apply(context)?;
        Ok(AstAndContextPair::new(
            SemAst::create_rule(&definition),
            context,
        ))
    }
}

impl SemCreateRule {
    pub(crate) fn from_ast_definition(
        ast: &CreateRule,
        context: &mut impl ParseContext,
    ) -> Result<Self, ResolutionFailure> {
        AnalysisError::check_dialect(Dialect::postgres(), context.dialect())?;
        let requested = FullName::from_ast_name(&ast.relation.name)?;
        let target = context
            .resolve_relation_name(&requested)
            .ok_or_else(|| ResolutionFailure::rule(&RuleProblem::target(&requested)))?;
        if context
            .get_view(&ViewName::from_full_name(&target))
            .is_some_and(ViewLike::is_materialized)
        {
            return Err(ResolutionFailure::rule(&RuleProblem::target(&target)));
        }
        let name = FullName::from_ast_name(&ast.name)?;
        let identity = RuleIdentity::new(&target, name.name().as_str());
        Self::validate_ast_shape(ast, context, &identity)?;
        let event = SemRuleEvent::ast_semantic(&ast.event);
        let with = WithClause::default();
        let mut rows = FromClause::default();
        for alias in ["old", "new"] {
            rows.push(DataSource::resolve_relation(
                context,
                &target,
                &AliasSpec::from_names(alias, &[]),
                &with,
            )?);
        }
        let predicate_scope = ScalarAnalysisContext::rule(&with, &rows, &identity, event, true);
        let predicate = Self::from_ast_rule_predicate(ast, context, &predicate_scope, &identity)?;
        let scope = ScalarAnalysisContext::rule(&with, &rows, &identity, event, false);
        if ast.event == RuleEvent::Select {
            for action in &ast.actions {
                SemAst::validate_ast_select_rule_ctes(action, &identity)?;
            }
        }
        let mut actions = Vec::new();
        for action in &ast.actions {
            actions.push(SemAst::from_ast_rule_action(
                action, context, &scope, &identity,
            )?);
        }
        let mut definition = Self::new(
            &identity,
            event,
            ast.replace,
            ast.instead,
            &predicate,
            &actions,
            context,
        )?;
        Self::validate_ast_results(ast, context, &definition)?;
        if ast.event == RuleEvent::Select {
            let syntax = SemAst::ast_rule_query(&ast.actions[0]).expect("validated SELECT action");
            let query = definition.actions()[0]
                .select_statement()
                .expect("resolved SELECT action");
            let lineage = ViewColumnLineage::from_ast_query(syntax, context, query)?;
            definition.retain_select_lineage(&lineage);
        }
        Ok(definition)
    }
}

impl SemCreateRule {
    pub(crate) fn ast_invalid<T>(
        identity: &RuleIdentity,
        detail: &str,
    ) -> Result<T, ResolutionFailure> {
        Err(ResolutionFailure::rule(&RuleProblem::definition(
            identity, detail,
        )))
    }
}

impl SemCreateRule {
    pub(crate) fn validate_ast_shape(
        ast: &CreateRule,
        context: &impl ParseContext,
        identity: &RuleIdentity,
    ) -> Result<(), ResolutionFailure> {
        if !ast.replace && context.rule(identity).is_some() {
            return Err(ResolutionFailure::rule(&RuleProblem::duplicate(identity)));
        }
        if ast.event == RuleEvent::Select {
            if context
                .get_view(&ViewName::from_full_name(identity.target()))
                .is_none()
            {
                return Self::ast_invalid(identity, "ON SELECT requires an ordinary view");
            }
            if !ast.replace {
                return Err(ResolutionFailure::rule(&RuleProblem::duplicate(identity)));
            }
            if identity.name().name().as_str() != "_RETURN"
                || !ast.instead
                || ast.predicate.is_some()
                || !(ast.actions.len() == 1 && SemAst::ast_rule_query(&ast.actions[0]).is_some())
            {
                return Self::ast_invalid(
                    identity,
                    "ON SELECT requires an unconditional _RETURN INSTEAD single SELECT action",
                );
            }
        } else if identity.name().name().as_str() == "_RETURN" {
            return Self::ast_invalid(identity, "_RETURN is reserved for ON SELECT");
        }
        Ok(())
    }
}

impl SemCreateRule {
    pub(crate) fn from_ast_rule_predicate(
        ast: &CreateRule,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        identity: &RuleIdentity,
    ) -> Result<Option<SemScalarExpr>, ResolutionFailure> {
        let Some(syntax) = &ast.predicate else {
            return Ok(None);
        };
        let mut predicate = SemScalarExpr::from_ast_in(syntax, context, scope)?;
        if SemScalarExpr::ast_is_rule_boolean(syntax) {
            predicate = SemScalarExpr::wrap_implicit_cast(&PgBuiltInType::bool(), &predicate);
        } else {
            SemScalarExpr::implicit_cast_resolved(context, &PgBuiltInType::bool(), &mut predicate)?;
        }
        if predicate.get_type() != Some(PgBuiltInType::bool()) {
            return Self::ast_invalid(identity, "WHERE must have boolean type");
        }
        let mut validator = PredicateValidator::new(context);
        predicate.walk(&mut validator)?;
        if validator.invalid {
            return Self::ast_invalid(
                identity,
                "rule WHERE cannot contain aggregates, windows or set-returning functions at its own level",
            );
        }
        Ok(Some(predicate))
    }
}

impl SemCreateRule {
    pub(crate) fn validate_ast_results(
        ast: &CreateRule,
        context: &impl ParseContext,
        definition: &Self,
    ) -> Result<(), ResolutionFailure> {
        let mut returning = false;
        for action in definition.actions() {
            if let Some(columns) = action.rule_returning()
                && !columns.is_empty()
            {
                if returning || !ast.instead || ast.predicate.is_some() {
                    return Self::ast_invalid(
                        definition.identity(),
                        "RETURNING requires an unconditional INSTEAD rule and at most one returning action",
                    );
                }
                returning = true;
                Self::ast_check_columns(context, definition.identity(), columns, false)?;
            }
            if ast.event == RuleEvent::Select
                && let Some(columns) = action.query_result_columns()
            {
                Self::ast_check_columns(context, definition.identity(), columns, true)?;
            }
        }
        Ok(())
    }
}

impl SemCreateRule {
    pub(crate) fn ast_check_columns(
        context: &impl ParseContext,
        identity: &RuleIdentity,
        columns: &ResultColumnCollection,
        names: bool,
    ) -> Result<(), ResolutionFailure> {
        let mut index = 0;
        loop {
            let expected = if let Some(table) =
                context.get_table(&TableName::from(identity.target().clone()))
            {
                table.get_column_at(index).cloned()
            } else if let Some(view) =
                context.get_view(&ViewName::from_full_name(identity.target()))
            {
                view.result_columns()
                    .get_column_at(index)
                    .map(ResultColumn::create_column_definition)
            } else {
                None
            };
            let expected_type = if let Some(expected) = &expected {
                expected.get_type()
            } else {
                None
            };
            let actual_type = if let Some(actual) = columns.get_column_at(index) {
                actual.col_type().as_ref()
            } else {
                None
            };
            if expected_type.is_some_and(TypeReference::rule_shape_needs_modifiers)
                || actual_type.is_some_and(TypeReference::rule_shape_needs_modifiers)
            {
                return Self::ast_invalid(
                    identity,
                    "exact result compatibility for modifier-capable types is pending #619",
                );
            }
            match (expected, columns.get_column_at(index)) {
                (None, None) => return Ok(()),
                (Some(expected), Some(actual))
                    if expected.get_type() == actual.col_type().as_ref()
                        && (!names || expected.name() == actual.column_name()) => {}
                _ => {
                    return Self::ast_invalid(
                        identity,
                        "rule result must match target column count, order and types; ON SELECT also requires matching names",
                    );
                }
            }
            index += 1;
        }
    }
}

#[cfg(test)]
mod tests;

impl SemRuleEvent {
    pub(crate) const fn ast_semantic(ast: &RuleEvent) -> Self {
        match ast {
            RuleEvent::Select => Self::select(),
            RuleEvent::Insert => Self::insert(),
            RuleEvent::Update => Self::update(),
            RuleEvent::Delete => Self::delete(),
        }
    }
}

impl CteStatement {
    pub(crate) fn validate_ast_select_rule_ctes(
        ast: &CteBody,
        identity: &RuleIdentity,
    ) -> Result<(), ResolutionFailure> {
        match ast {
            CteBody::Query(query) => {
                SelectStatement::validate_ast_select_rule_ctes(query.as_ref(), identity)
            }
            _ => SemCreateRule::ast_invalid(
                identity,
                "ON SELECT rule cannot contain data-modifying CTEs",
            ),
        }
    }
}

impl SelectStatement {
    pub(crate) fn validate_ast_select_rule_ctes(
        ast: &Query,
        identity: &RuleIdentity,
    ) -> Result<(), ResolutionFailure> {
        for entry in &ast.with.entries {
            CteStatement::validate_ast_select_rule_ctes(&entry.body, identity)?;
        }
        Ok(())
    }
}

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

struct PredicateValidator<'a, C> {
    context: &'a C,
    depth: usize,
    invalid: bool,
}
impl<'a, C> PredicateValidator<'a, C> {
    const fn new(context: &'a C) -> Self {
        Self {
            context,
            depth: 0,
            invalid: false,
        }
    }
}
impl<C: ParseContext> SemanticVisitorMut for PredicateValidator<'_, C> {
    fn enter_query(&mut self) {
        self.depth += 1;
    }
    fn leave_query(&mut self) {
        self.depth -= 1;
    }
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        if self.depth == 0 && !expression.preserves_view_rows(self.context) {
            self.invalid = true;
        }
        Ok(())
    }
}

impl SemScalarExpr {
    pub(crate) const fn ast_pending_rule_semantics(ast: &SubqueryKind) -> Option<&'static str> {
        match ast {
            SubqueryKind::Scalar | SubqueryKind::Array => None,
            SubqueryKind::Exists => Some("EXISTS subquery semantics are pending #570"),
            SubqueryKind::Any | SubqueryKind::All | SubqueryKind::Row => {
                Some("quantified/row subquery semantics are pending #571")
            }
            SubqueryKind::MultiExpression => {
                Some("multi-expression subquery semantics are pending #573")
            }
        }
    }
}

impl TypeReference {
    fn rule_shape_needs_modifiers(&self) -> bool {
        if self.is_dynamic() {
            return false;
        }
        let Some(name) = self.full_name() else {
            return false;
        };
        name.schema()
            .is_some_and(|schema| schema.as_str() == "pg_catalog")
            && matches!(
                name.name().as_str(),
                "varchar"
                    | "bpchar"
                    | "numeric"
                    | "time"
                    | "timetz"
                    | "timestamp"
                    | "timestamptz"
                    | "interval"
                    | "bit"
                    | "varbit"
            )
    }
}

impl SemScalarExpr {
    pub(crate) const fn ast_is_rule_boolean(ast: &Expr) -> bool {
        if let Expr::Literal(literal) = ast {
            Self::ast_literal_is_rule_boolean(literal)
        } else {
            false
        }
    }
}

impl SemScalarExpr {
    pub(crate) const fn ast_literal_is_rule_boolean(ast: &Literal) -> bool {
        matches!(ast, Literal::Null | Literal::String(_))
    }
}
