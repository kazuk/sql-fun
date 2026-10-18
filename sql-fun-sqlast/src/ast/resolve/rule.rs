//! PostgreSQL rule analysis over common syntax only.
use crate::ast::{CreateRule, Dialect, RuleEvent, StatementNode};
use crate::sem::problem::RuleProblem;
use crate::sem::{
    self, AliasSpec, AnalysisError, DataSource, FromClause, ParseContext, RuleIdentity,
    ScalarAnalysisContext, SemAst, WithClause,
};

impl CreateRule {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<sem::CreateRule, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_definition(context);
        let result = result.and_then(|definition| Self::validate_resolved(context, definition));
        if let Err(error) = &result {
            error.report_rule_problem(context)?;
        }
        context.finish_analysis(result)
    }
    fn validate_resolved(
        context: &impl ParseContext,
        definition: sem::CreateRule,
    ) -> Result<sem::CreateRule, AnalysisError> {
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
    ) -> Result<crate::AstAndContextPair<C>, AnalysisError> {
        let definition = self.resolve(&mut context)?;
        let context = definition.apply(context)?;
        Ok(crate::AstAndContextPair::new(
            SemAst::create_rule(&definition),
            context,
        ))
    }
    fn resolve_definition(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<sem::CreateRule, AnalysisError> {
        AnalysisError::check_dialect(Dialect::postgres(), context.dialect())?;
        let requested = self.relation.name.full_name()?;
        let target = context
            .resolve_relation_name(&requested)
            .ok_or_else(|| AnalysisError::rule(&RuleProblem::target(&requested)))?;
        if context
            .get_view(&crate::sem::ViewName::from_full_name(&target))
            .is_some_and(sem::ViewLike::is_materialized)
        {
            return Err(AnalysisError::rule(&RuleProblem::target(&target)));
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
                &AliasSpec::from_local_name(alias),
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
        let mut definition = sem::CreateRule::new(
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
            let info = syntax.rule_write_info(context, query, &target)?;
            definition.retain_select_write_info(&info, context)?;
        }
        Ok(definition)
    }
    fn invalid<T>(identity: &RuleIdentity, detail: &str) -> Result<T, AnalysisError> {
        Err(AnalysisError::rule(&RuleProblem::definition(
            identity, detail,
        )))
    }
    fn validate_shape(
        &self,
        context: &impl ParseContext,
        identity: &RuleIdentity,
    ) -> Result<(), AnalysisError> {
        if !self.replace && context.rule(identity).is_some() {
            return Err(AnalysisError::rule(&RuleProblem::duplicate(identity)));
        }
        if self.event == RuleEvent::Select {
            if context
                .get_view(&crate::sem::ViewName::from_full_name(identity.target()))
                .is_none()
            {
                return Self::invalid(identity, "ON SELECT requires an ordinary view");
            }
            if !self.replace {
                return Err(AnalysisError::rule(&RuleProblem::duplicate(identity)));
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
    ) -> Result<Option<sem::SemScalarExpr>, AnalysisError> {
        let Some(syntax) = &self.predicate else {
            return Ok(None);
        };
        let mut predicate = syntax.resolve_in(context, scope)?;
        if syntax.is_rule_boolean_literal() {
            predicate =
                sem::SemScalarExpr::wrap_implicit_cast(&sem::PgBuiltInType::bool(), &predicate);
        } else {
            sem::SemScalarExpr::implicit_cast_resolved(
                context,
                &sem::PgBuiltInType::bool(),
                &mut predicate,
            )?;
        }
        if predicate.get_type() != Some(sem::PgBuiltInType::bool()) {
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
        definition: &sem::CreateRule,
    ) -> Result<(), AnalysisError> {
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
        columns: &sem::ResultColumnCollection,
        names: bool,
    ) -> Result<(), AnalysisError> {
        let mut index = 0;
        loop {
            let expected = if let Some(table) =
                context.get_table(&crate::sem::TableName::from(identity.target().clone()))
            {
                table.get_column_at(index).cloned()
            } else if let Some(view) =
                context.get_view(&crate::sem::ViewName::from_full_name(identity.target()))
            {
                view.result_columns()
                    .get_column_at(index)
                    .map(sem::ResultColumn::create_column_definition)
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
            if expected_type.is_some_and(sem::TypeReference::rule_shape_needs_modifiers)
                || actual_type.is_some_and(sem::TypeReference::rule_shape_needs_modifiers)
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
    const fn semantic(self) -> sem::RuleEvent {
        match self {
            Self::Select => sem::RuleEvent::select(),
            Self::Insert => sem::RuleEvent::insert(),
            Self::Update => sem::RuleEvent::update(),
            Self::Delete => sem::RuleEvent::delete(),
        }
    }
}
#[cfg(test)]
mod tests;

impl crate::ast::CteBody {
    fn validate_select_rule_ctes(&self, identity: &RuleIdentity) -> Result<(), AnalysisError> {
        match self {
            Self::Query(query) => query.validate_select_rule_ctes(identity),
            _ => CreateRule::invalid(
                identity,
                "ON SELECT rule cannot contain data-modifying CTEs",
            ),
        }
    }
}
impl crate::ast::Query {
    fn validate_select_rule_ctes(&self, identity: &RuleIdentity) -> Result<(), AnalysisError> {
        for entry in &self.with.entries {
            entry.body.validate_select_rule_ctes(identity)?;
        }
        Ok(())
    }
}
impl StatementNode {
    const fn rule_query(&self) -> Option<&crate::ast::Query> {
        if let Self::Query(query) = self {
            Some(query)
        } else {
            None
        }
    }
    fn validate_select_rule_ctes(&self, identity: &RuleIdentity) -> Result<(), AnalysisError> {
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
    ) -> Result<SemAst, AnalysisError> {
        match self {
            Self::Query(query) => query
                .resolve_in(context, Some(scope))
                .map(|query| SemAst::select(&query)),
            Self::Insert(insert) => insert
                .resolve_in(context, Some(scope))
                .map(|insert| SemAst::insert(&insert)),
            Self::Update(update) => update
                .resolve_in(context, Some(scope))
                .map(|update| SemAst::update(&update)),
            Self::Delete(delete) => delete
                .resolve_in(context, Some(scope))
                .map(|delete| SemAst::delete(&delete)),
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
impl<C: ParseContext> sem::traversal::SemanticVisitor for PredicateValidator<'_, C> {
    fn enter_query(&mut self) {
        self.depth += 1;
    }
    fn leave_query(&mut self) {
        self.depth -= 1;
    }
    fn expression(&mut self, expression: &mut sem::SemScalarExpr) -> Result<(), AnalysisError> {
        if self.depth == 0 && !expression.preserves_view_rows(self.context) {
            self.invalid = true;
        }
        Ok(())
    }
}

impl crate::ast::SubqueryKind {
    pub(super) const fn pending_rule_semantics(self) -> Option<&'static str> {
        match self {
            Self::Scalar => None,
            Self::Exists => Some("EXISTS subquery semantics are pending #570"),
            Self::Any | Self::All | Self::Row => {
                Some("quantified/row subquery semantics are pending #571")
            }
            Self::Array => Some("ARRAY subquery semantics are pending #572"),
            Self::MultiExpression => Some("multi-expression subquery semantics are pending #573"),
        }
    }
}

impl sem::TypeReference {
    fn rule_shape_needs_modifiers(&self) -> bool {
        if self.is_dynamic() {
            return false;
        }
        let name = self.full_name();
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

impl crate::ast::Expr {
    const fn is_rule_boolean_literal(&self) -> bool {
        if let Self::Literal(literal) = self {
            literal.is_rule_boolean_literal()
        } else {
            false
        }
    }
}
impl crate::ast::Literal {
    const fn is_rule_boolean_literal(&self) -> bool {
        matches!(self, Self::Null | Self::String(_))
    }
}
