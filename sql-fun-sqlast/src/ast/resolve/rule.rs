//! PostgreSQL rule analysis over common syntax only.

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

impl CreateRule {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<SemCreateRule>, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self
            .resolve_definition(context)
            .and_then(|definition| Self::validate_resolved(context, definition));
        let result = ResolutionFailure::finish(result, context);
        context.finish_analysis(result)
    }
    fn validate_resolved(
        context: &impl ParseContext,
        definition: SemCreateRule,
    ) -> Result<SemCreateRule, ResolutionFailure> {
        if context.parameter_inference_needed() {
            return Self::invalid(
                definition.identity(),
                "rule definitions cannot contain bind parameters",
            );
        }
        if context.analysis_state().has_problems() {
            return Self::invalid(
                definition.identity(),
                "rule contains unresolved or invalid constructs",
            );
        }
        Ok(definition)
    }
    pub(crate) fn resolve_and_apply<C: ParseContext>(
        &self,
        mut context: C,
    ) -> Result<AstAndContextPair<C>, AnalysisError> {
        let Some(definition) = self.resolve(&mut context)? else {
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
    fn resolve_definition(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<SemCreateRule, ResolutionFailure> {
        AnalysisError::check_dialect(Dialect::postgres(), context.dialect())?;
        let requested = self.relation.name.full_name()?;
        let target = context
            .resolve_relation_name(&requested)
            .ok_or_else(|| ResolutionFailure::rule(&RuleProblem::target(&requested)))?;
        if context
            .get_view(&ViewName::from_full_name(&target))
            .is_some_and(ViewLike::is_materialized)
        {
            return Err(ResolutionFailure::rule(&RuleProblem::target(&target)));
        }
        let name = self.name.full_name()?;
        let identity = RuleIdentity::new(&target, name.name().as_str());
        self.validate_shape(context, &identity)?;
        let event = self.event.semantic();
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
        let predicate = self.resolve_rule_predicate(context, &predicate_scope, &identity)?;
        let scope = ScalarAnalysisContext::rule(&with, &rows, &identity, event, false);
        if self.event == RuleEvent::Select {
            for action in &self.actions {
                action.validate_select_rule_ctes(&identity)?;
            }
        }
        let mut actions = Vec::new();
        for action in &self.actions {
            actions.push(action.resolve_rule_action(context, &scope, &identity)?);
        }
        let mut definition = SemCreateRule::new(
            &identity,
            event,
            self.replace,
            self.instead,
            &predicate,
            &actions,
            context,
        )?;
        self.validate_results(context, &definition)?;
        if self.event == RuleEvent::Select {
            let syntax = self.actions[0]
                .rule_query()
                .expect("validated SELECT action");
            let query = definition.actions()[0]
                .select_statement()
                .expect("resolved SELECT action");
            let lineage = syntax.view_lineage(context, query)?;
            definition.retain_select_lineage(&lineage);
        }
        Ok(definition)
    }
    fn invalid<T>(identity: &RuleIdentity, detail: &str) -> Result<T, ResolutionFailure> {
        Err(ResolutionFailure::rule(&RuleProblem::definition(
            identity, detail,
        )))
    }
    fn validate_shape(
        &self,
        context: &impl ParseContext,
        identity: &RuleIdentity,
    ) -> Result<(), ResolutionFailure> {
        if !self.replace && context.rule(identity).is_some() {
            return Err(ResolutionFailure::rule(&RuleProblem::duplicate(identity)));
        }
        if self.event == RuleEvent::Select {
            if context
                .get_view(&ViewName::from_full_name(identity.target()))
                .is_none()
            {
                return Self::invalid(identity, "ON SELECT requires an ordinary view");
            }
            if !self.replace {
                return Err(ResolutionFailure::rule(&RuleProblem::duplicate(identity)));
            }
            if identity.name().name().as_str() != "_RETURN"
                || !self.instead
                || self.predicate.is_some()
                || !(self.actions.len() == 1 && self.actions[0].rule_query().is_some())
            {
                return Self::invalid(
                    identity,
                    "ON SELECT requires an unconditional _RETURN INSTEAD single SELECT action",
                );
            }
        } else if identity.name().name().as_str() == "_RETURN" {
            return Self::invalid(identity, "_RETURN is reserved for ON SELECT");
        }
        Ok(())
    }
    fn resolve_rule_predicate(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        identity: &RuleIdentity,
    ) -> Result<Option<SemScalarExpr>, ResolutionFailure> {
        let Some(syntax) = &self.predicate else {
            return Ok(None);
        };
        let mut predicate = syntax.resolve_in(context, scope)?;
        if syntax.is_rule_boolean_literal() {
            predicate = SemScalarExpr::wrap_implicit_cast(&PgBuiltInType::bool(), &predicate);
        } else {
            SemScalarExpr::implicit_cast_resolved(context, &PgBuiltInType::bool(), &mut predicate)?;
        }
        if predicate.get_type() != Some(PgBuiltInType::bool()) {
            return Self::invalid(identity, "WHERE must have boolean type");
        }
        let mut validator = PredicateValidator::new(context);
        predicate.walk(&mut validator)?;
        if validator.invalid {
            return Self::invalid(
                identity,
                "rule WHERE cannot contain aggregates, windows or set-returning functions at its own level",
            );
        }
        Ok(Some(predicate))
    }
    fn validate_results(
        &self,
        context: &impl ParseContext,
        definition: &SemCreateRule,
    ) -> Result<(), ResolutionFailure> {
        let mut returning = false;
        for action in definition.actions() {
            if let Some(columns) = action.rule_returning()
                && !columns.is_empty()
            {
                if returning || !self.instead || self.predicate.is_some() {
                    return Self::invalid(
                        definition.identity(),
                        "RETURNING requires an unconditional INSTEAD rule and at most one returning action",
                    );
                }
                returning = true;
                Self::check_columns(context, definition.identity(), columns, false)?;
            }
            if self.event == RuleEvent::Select
                && let Some(columns) = action.query_result_columns()
            {
                Self::check_columns(context, definition.identity(), columns, true)?;
            }
        }
        Ok(())
    }
    fn check_columns(
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
                return Self::invalid(
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
                    return Self::invalid(
                        identity,
                        "rule result must match target column count, order and types; ON SELECT also requires matching names",
                    );
                }
            }
            index += 1;
        }
    }
}

impl RuleEvent {
    const fn semantic(self) -> SemRuleEvent {
        match self {
            Self::Select => SemRuleEvent::select(),
            Self::Insert => SemRuleEvent::insert(),
            Self::Update => SemRuleEvent::update(),
            Self::Delete => SemRuleEvent::delete(),
        }
    }
}
#[cfg(test)]
mod tests;

impl CteBody {
    fn validate_select_rule_ctes(&self, identity: &RuleIdentity) -> Result<(), ResolutionFailure> {
        match self {
            Self::Query(query) => query.validate_select_rule_ctes(identity),
            _ => CreateRule::invalid(
                identity,
                "ON SELECT rule cannot contain data-modifying CTEs",
            ),
        }
    }
}
impl Query {
    fn validate_select_rule_ctes(&self, identity: &RuleIdentity) -> Result<(), ResolutionFailure> {
        for entry in &self.with.entries {
            entry.body.validate_select_rule_ctes(identity)?;
        }
        Ok(())
    }
}
impl StatementNode {
    const fn rule_query(&self) -> Option<&Query> {
        if let Self::Query(query) = self {
            Some(query)
        } else {
            None
        }
    }
    fn validate_select_rule_ctes(&self, identity: &RuleIdentity) -> Result<(), ResolutionFailure> {
        if let Self::Query(query) = self {
            query.validate_select_rule_ctes(identity)?;
        }
        Ok(())
    }
    fn resolve_rule_action(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        identity: &RuleIdentity,
    ) -> Result<SemAst, ResolutionFailure> {
        match self {
            Self::Query(query) => query
                .resolve_in(context, Some(scope))
                .map(|query| SemAst::select(&query))
                .map_err(Into::into),
            Self::Insert(insert) => insert
                .resolve_in(context, Some(scope))
                .map(|value| {
                    value
                        .as_ref()
                        .map_or_else(SemAst::unresolved_statement, SemAst::insert)
                })
                .map_err(Into::into),
            Self::Update(update) => update
                .resolve_in(context, Some(scope))
                .map(|value| {
                    value
                        .as_ref()
                        .map_or_else(SemAst::unresolved_statement, SemAst::update)
                })
                .map_err(Into::into),
            Self::Delete(delete) => delete
                .resolve_in(context, Some(scope))
                .map(|delete| SemAst::delete(&delete))
                .map_err(Into::into),
            _ => CreateRule::invalid(
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

impl SubqueryKind {
    pub(super) const fn pending_rule_semantics(self) -> Option<&'static str> {
        match self {
            Self::Scalar | Self::Array => None,
            Self::Exists => Some("EXISTS subquery semantics are pending #570"),
            Self::Any | Self::All | Self::Row => {
                Some("quantified/row subquery semantics are pending #571")
            }
            Self::MultiExpression => Some("multi-expression subquery semantics are pending #573"),
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

impl Expr {
    const fn is_rule_boolean_literal(&self) -> bool {
        if let Self::Literal(literal) = self {
            literal.is_rule_boolean_literal()
        } else {
            false
        }
    }
}
impl Literal {
    const fn is_rule_boolean_literal(&self) -> bool {
        matches!(self, Self::Null | Self::String(_))
    }
}
