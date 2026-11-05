use crate::{
    AstAndContextPair,
    ast::{CreateRule, Dialect, RuleEvent},
    context::ParseContext,
    names::{FullName, TableName, ViewName},
    sem::{
        AliasSpec, AnalysisError, CreateRule as SemCreateRule, DataSource, FromClause,
        PgBuiltInType, ResolutionFailure, ResultColumn, ResultColumnCollection,
        RuleEvent as SemRuleEvent, RuleIdentity, ScalarAnalysisContext, SemAst, SemScalarExpr,
        TypeReference, ViewColumnLineage, ViewLike, WithClause, problem::RuleProblem,
        traversal::SemanticVisitorMut,
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
        let requested = FullName::from_ast_name(ast.relation().name())?;
        let target = context
            .resolve_relation_name(&requested)
            .ok_or_else(|| ResolutionFailure::rule(&RuleProblem::target(&requested)))?;
        if context
            .get_view(&ViewName::from_full_name(&target))
            .is_some_and(ViewLike::is_materialized)
        {
            return Err(ResolutionFailure::rule(&RuleProblem::target(&target)));
        }
        let name = FullName::from_ast_name(ast.name())?;
        let identity = RuleIdentity::new(&target, name.name().as_str());
        Self::validate_ast_shape(ast, context, &identity)?;
        let event = SemRuleEvent::ast_semantic(&ast.event());
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
        if ast.event() == RuleEvent::Select {
            for action in ast.actions() {
                SemAst::validate_ast_select_rule_ctes(action, &identity)?;
            }
        }
        let mut actions = Vec::new();
        for action in ast.actions() {
            actions.push(SemAst::from_ast_rule_action(
                action, context, &scope, &identity,
            )?);
        }
        let mut definition = Self::new(
            &identity,
            event,
            ast.replace(),
            ast.instead(),
            &predicate,
            &actions,
            context,
        )?;
        Self::validate_ast_results(ast, context, &definition)?;
        if ast.event() == RuleEvent::Select {
            let syntax =
                SemAst::ast_rule_query(&ast.actions()[0]).expect("validated SELECT action");
            let query = definition.actions()[0]
                .select_statement()
                .expect("resolved SELECT action");
            let lineage = ViewColumnLineage::from_ast_query(syntax, context, query)?;
            definition.retain_select_lineage(&lineage);
        }
        Ok(definition)
    }
}

#[cfg(test)]
mod test_resolves_complete_statement_without_applying {
    use crate::{
        context::RuleInfoRead,
        sem::RuleEvent,
        test_helpers::{rule as test_helpers, statements},
    };

    #[rstest::rstest]
    #[case::nothing(
        "CREATE RULE r AS ON INSERT TO items DO INSTEAD NOTHING",
        RuleEvent::insert(),
        0
    )]
    #[case::also_nothing(
        "CREATE RULE r AS ON UPDATE TO items DO NOTHING",
        RuleEvent::update(),
        0
    )]
    #[case::insert(
        "CREATE RULE r AS ON INSERT TO items DO INSERT INTO sink VALUES (NEW.id)",
        RuleEvent::insert(),
        1
    )]
    #[case::update(
        "CREATE RULE r AS ON UPDATE TO items DO UPDATE sink SET id=NEW.id WHERE id=OLD.id",
        RuleEvent::update(),
        1
    )]
    #[case::delete(
        "CREATE RULE r AS ON DELETE TO items DO DELETE FROM sink WHERE id=OLD.id",
        RuleEvent::delete(),
        1
    )]
    #[case::set_member_local_alias(
        "CREATE RULE r AS ON UPDATE TO items DO SELECT old.id FROM sink old UNION SELECT id FROM sink",
        RuleEvent::update(),
        1
    )]
    #[case::multiple(
        "CREATE RULE r AS ON UPDATE TO items DO (SELECT OLD.id; SELECT NEW.id)",
        RuleEvent::update(),
        2
    )]
    #[case::star(
        "CREATE RULE r AS ON UPDATE TO items DO SELECT NEW.*",
        RuleEvent::update(),
        1
    )]
    #[case::correlated(
        "CREATE RULE r AS ON UPDATE TO items DO SELECT (SELECT OLD.id)",
        RuleEvent::update(),
        1
    )]
    #[case::array_query(
        "CREATE RULE r AS ON UPDATE TO items DO SELECT ARRAY(SELECT id FROM sink)",
        RuleEvent::update(),
        1
    )]
    #[case::cte(
        "CREATE RULE r AS ON UPDATE TO items DO WITH x AS (SELECT id FROM sink) SELECT id FROM x",
        RuleEvent::update(),
        1
    )]
    #[case::view(
        "CREATE RULE r AS ON UPDATE TO item_view DO SELECT NEW.id",
        RuleEvent::update(),
        1
    )]
    #[case::predicate(
        "CREATE RULE r AS ON UPDATE TO items WHERE OLD.id IS NOT NULL DO SELECT NEW.id",
        RuleEvent::update(),
        1
    )]
    #[case::null_predicate(
        "CREATE RULE r AS ON UPDATE TO items WHERE NULL DO NOTHING",
        RuleEvent::update(),
        0
    )]
    #[case::subquery_predicate(
        "CREATE RULE r AS ON UPDATE TO items WHERE (SELECT true FROM sink WHERE sink.id=OLD.id) DO NOTHING",
        RuleEvent::update(),
        0
    )]
    #[case::nested_shadow(
        "CREATE RULE r AS ON UPDATE TO items DO SELECT (SELECT old.id FROM sink old)",
        RuleEvent::update(),
        1
    )]
    #[case::returning(
        "CREATE RULE r AS ON INSERT TO items DO INSTEAD INSERT INTO sink VALUES(NEW.id) RETURNING id AS different",
        RuleEvent::insert(),
        1
    )]
    #[case::returning_old(
        "CREATE RULE r AS ON UPDATE TO items DO INSTEAD UPDATE sink SET id=NEW.id RETURNING OLD.id",
        RuleEvent::update(),
        1
    )]
    fn resolves_complete_statement_without_applying(
        #[case] sql: &str,
        #[case] event: RuleEvent,
        #[case] actions: usize,
    ) -> testresult::TestResult {
        let mut context = test_helpers::catalog();
        let statement = statements::resolve_postgres(sql, &mut context)?;
        let rule = statement.rule().unwrap();
        assert_eq!(rule.event(), event);
        assert_eq!(rule.actions().len(), actions);
        assert!(context.rule(rule.identity()).is_none());
        assert!(rule.dependencies().relations().contains(rule.target()));
        assert!(rule.source().is_some());
        assert_eq!(context.reported_problem_count(), 0);
        Ok(())
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
        if !ast.replace() && context.rule(identity).is_some() {
            return Err(ResolutionFailure::rule(&RuleProblem::duplicate(identity)));
        }
        if ast.event() == RuleEvent::Select {
            if context
                .get_view(&ViewName::from_full_name(identity.target()))
                .is_none()
            {
                return Self::ast_invalid(identity, "ON SELECT requires an ordinary view");
            }
            if !ast.replace() {
                return Err(ResolutionFailure::rule(&RuleProblem::duplicate(identity)));
            }
            if identity.name().name().as_str() != "_RETURN"
                || !ast.instead()
                || ast.predicate().is_some()
                || !(ast.actions().len() == 1
                    && SemAst::ast_rule_query(&ast.actions()[0]).is_some())
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
        let Some(syntax) = &ast.predicate() else {
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
                if returning || !ast.instead() || ast.predicate().is_some() {
                    return Self::ast_invalid(
                        definition.identity(),
                        "RETURNING requires an unconditional INSTEAD rule and at most one returning action",
                    );
                }
                returning = true;
                Self::ast_check_columns(context, definition.identity(), columns, false)?;
            }
            if ast.event() == RuleEvent::Select
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
mod test_modifier_capable_results_have_an_explicit_semantic_boundary {
    use crate::{
        ast::CreateTableMode as SemCreateTableMode,
        context::RuleInfoRead,
        names::{ColumnName as NamesColumnName, FullName, TableName as NamesTableName},
        sem::{
            ColumnDefinition as SemColumnDefinition, CreateTable as SemCreateTable, CreateView,
            RuleIdentity, TypeReference, ViewLike,
        },
        test_helpers::{rule as test_helpers, source_span, statements},
    };

    #[test]
    fn modifier_capable_results_have_an_explicit_semantic_boundary() -> testresult::TestResult {
        let mut context = test_helpers::catalog();
        let name = FullName::with_schema("public", "sized");
        let column = SemColumnDefinition::new(
            &Some(NamesColumnName::from("id")),
            Some(&TypeReference::concrete_type_ref(
                &FullName::with_schema("pg_catalog", "varchar"),
                false,
            )),
            Some(false),
            &[],
        );
        let table = SemCreateTable::new(
            &NamesTableName::from(name.clone()),
            vec![column],
            SemCreateTableMode::from_if_not_exists(false),
            false,
            &source_span(),
        );
        context.set_get_table_impl_result(&name, &Some(table));
        let query = statements::resolve_postgres("SELECT id FROM sized", &mut context)?
            .into_select_statemtnt()
            .unwrap();
        let name = FullName::with_schema("public", "sized_view");
        let view = CreateView::new(&name, &query, &[], &None, false);
        context.set_get_view_impl_result(&name, &Some(ViewLike::from(&view)));
        let unresolved = statements::resolve_postgres(r#"CREATE OR REPLACE RULE "_RETURN" AS ON SELECT TO sized_view DO INSTEAD SELECT id FROM sized"#, &mut context).unwrap();
        assert!(matches!(
            unresolved,
            crate::sem::SemAst::UnresolvedStatement
        ));
        assert!(context.reported_problems().iter().any(|problem| {
            problem.rule_problem().is_some_and(|rule| {
                rule.definition_detail()
                    .is_some_and(|detail| detail.contains("#619"))
            })
        }));
        assert!(context.rule(&RuleIdentity::new(&name, "_RETURN")).is_none());
        Ok(())
    }
}

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
