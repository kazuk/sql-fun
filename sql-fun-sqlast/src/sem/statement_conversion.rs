//! Common statement analysis and catalog application, owned by semantics.

use crate::{
    AstAndContextPair, CacheContext,
    ast::{
        AlterSequence, Comment, DropTable, EnumType, Extension, Sequence, StatementNode,
        VariableSet, View,
    },
    context::ParseContext,
    names::{FullName, TableName, ViewName},
    sem::{
        AlterOwner, AlterSequence as SemAlterSequence, AnalysisError, AnalysisProblem,
        Comment as SemComment, CreateCompositeType, CreateDomain, CreateEnum, CreateExtension,
        CreateFunction, CreateIndex, CreateRule, CreateSchema, CreateSequence, CreateTable,
        CreateTableAs, CreateTrigger, CreateView, DeleteStatement, DropRule,
        DropTable as SemDropTable, DropTrigger, EngineFeatureValidation, InsertStatement, Kind,
        MergeStatement, ParameterConstraints, RelationAlteration, ScalarAnalysisContext,
        SelectStatement, SemAst, SemScalarExpr, UpdateStatement, VariableSet as SemVariableSet,
        ViewCheckOption, ViewColumnLineage, problem::ViewProblem,
    },
};

impl SemAst {
    pub(crate) fn from_ast_node(
        ast: &StatementNode,
        context: &mut impl ParseContext,
        cache: &mut CacheContext,
    ) -> Result<Self, AnalysisError> {
        if !EngineFeatureValidation::statement(ast, context)? {
            return Ok(Self::unresolved_statement());
        }
        match ast {
            StatementNode::DefaultPrivileges(_) => {
                todo!("ALTER DEFAULT PRIVILEGES interpretation (#955)")
            }
            StatementNode::ForeignTable(_) => {
                todo!("CREATE FOREIGN TABLE interpretation (#956)")
            }
            StatementNode::Query(query) => Ok(Self::select(&SelectStatement::from_ast(
                query, context, cache,
            )?)),
            StatementNode::Insert(insert) => Ok(Self::from_insert_resolution(
                InsertStatement::from_ast(insert, context, cache)?.as_ref(),
            )),
            StatementNode::Update(update) => Ok(Self::from_update_resolution(
                UpdateStatement::from_ast(update, context, cache)?.as_ref(),
            )),
            StatementNode::Delete(delete) => Ok(Self::delete(&DeleteStatement::from_ast(
                delete, context, cache,
            )?)),
            StatementNode::Merge(merge) => Ok(Self::merge(&MergeStatement::from_ast(
                merge, context, cache,
            )?)),
            StatementNode::CreateRule(rule) => Ok(Self::from_create_rule_resolution(
                CreateRule::from_ast(rule, context)?.as_ref(),
            )),
            StatementNode::DropTrigger(trigger) => Ok(Self::from_drop_trigger_resolution(
                DropTrigger::from_ast(trigger, context)?.as_ref(),
            )),
            StatementNode::CreateTrigger(trigger) => Ok(Self::from_create_trigger_resolution(
                CreateTrigger::from_ast(trigger, context)?.as_ref(),
            )),
            StatementNode::DropRule(rule) => Ok(Self::from_drop_rule_resolution(
                DropRule::from_ast(rule, context)?.as_ref(),
            )),
            StatementNode::View(_) => {
                AnalysisError::raise_unexpected_input("view requires catalog application")
            }
            StatementNode::Extension(_) => {
                AnalysisError::raise_unexpected_input("extension requires catalog application")
            }
            StatementNode::VariableSet(_) => {
                AnalysisError::raise_unexpected_input("variable set requires catalog application")
            }
            StatementNode::EnumType(_) => {
                AnalysisError::raise_unexpected_input("enum type requires catalog application")
            }
            StatementNode::Sequence(_) => {
                AnalysisError::raise_unexpected_input("sequence requires catalog application")
            }
            StatementNode::DropTable(_) => {
                AnalysisError::raise_unexpected_input("drop table requires catalog application")
            }
            StatementNode::AlterSequence(_) => {
                AnalysisError::raise_unexpected_input("alter sequence requires catalog application")
            }
            StatementNode::Comment(_) => {
                AnalysisError::raise_unexpected_input("comment requires catalog application")
            }
            StatementNode::AlterOwner(alteration) => Ok(Self::alter_owner(&AlterOwner::from_ast(
                alteration, context,
            )?)),
            StatementNode::CompositeType(composite_type) => Ok(Self::create_composite_type(
                &CreateCompositeType::from_ast(composite_type, context)?,
            )),
            StatementNode::AlterRelation(_) => {
                AnalysisError::raise_unexpected_input("statement requires catalog application")
            }
            StatementNode::Function(function) => Ok(Self::create_function(
                &CreateFunction::from_ast(function, context)?,
            )),
            StatementNode::Index(index) => Ok(Self::from_index_resolution(&CreateIndex::from_ast(
                index, context,
            )?)),
            StatementNode::TableAs(_) => {
                AnalysisError::raise_unexpected_input("table as requires catalog application")
            }
            StatementNode::Table(_) => {
                AnalysisError::raise_unexpected_input("table requires source metadata")
            }
            StatementNode::Schema(_) => {
                AnalysisError::raise_unexpected_input("schema requires catalog application")
            }
            StatementNode::Domain(domain) => Ok(Self::create_domain(&CreateDomain::from_ast(
                domain, context,
            )?)),
        }
    }
    pub(crate) fn apply_ast_node<TContext>(
        ast: &StatementNode,
        context: TContext,
        cache: &mut CacheContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let mut context = context;
        context.set_parameter_constraints(&ParameterConstraints::default());
        if !EngineFeatureValidation::statement(ast, &mut context)? {
            return Ok(AstAndContextPair::new(
                Self::unresolved_statement(),
                context,
            ));
        }
        match ast {
            StatementNode::CreateTrigger(trigger) => CreateTrigger::apply_ast(trigger, context),
            StatementNode::DropTrigger(trigger) => DropTrigger::apply_ast(trigger, context),
            StatementNode::CreateRule(rule) => CreateRule::apply_ast(rule, context),
            StatementNode::DropRule(rule) => DropRule::apply_ast(rule, context),
            StatementNode::View(view) => CreateView::from_ast_and_apply(view, context, cache),
            StatementNode::Extension(extension) => {
                CreateExtension::from_ast_and_apply(extension, context)
            }
            StatementNode::VariableSet(variable_set) => {
                SemVariableSet::from_ast_and_apply(variable_set, context)
            }
            StatementNode::EnumType(enum_type) => {
                CreateEnum::from_ast_and_apply(enum_type, context)
            }
            StatementNode::Sequence(sequence) => {
                CreateSequence::from_ast_and_apply(sequence, context)
            }
            StatementNode::DropTable(drop_table) => {
                SemDropTable::from_ast_and_apply(drop_table, context)
            }
            StatementNode::AlterSequence(alter_sequence) => {
                SemAlterSequence::from_ast_and_apply(alter_sequence, context)
            }
            StatementNode::Comment(comment) => SemComment::from_ast_and_apply(comment, context),
            StatementNode::AlterOwner(alteration) => {
                AlterOwner::from_ast_and_apply(alteration, context)
            }
            StatementNode::CompositeType(composite_type) => {
                CreateCompositeType::from_ast_and_apply(composite_type, context)
            }
            StatementNode::AlterRelation(statement) => {
                RelationAlteration::from_ast_and_apply(statement, context)
            }
            StatementNode::Domain(domain) => CreateDomain::from_ast_and_apply(domain, context),
            StatementNode::Function(function) => {
                CreateFunction::from_ast_and_apply(function, context)
            }
            StatementNode::Schema(schema) => {
                CreateSchema::from_ast_and_apply(schema, context, cache)
            }
            StatementNode::Index(index) => CreateIndex::from_ast_and_apply(index, context),
            StatementNode::TableAs(table_as) => {
                CreateTableAs::from_ast_and_apply(table_as, context, cache)
            }
            StatementNode::Table(table) => CreateTable::from_ast_and_apply(table, context),
            _ => Ok(AstAndContextPair::new(
                Self::from_ast_node(ast, &mut context, cache)?,
                context,
            )),
        }
    }
}

impl SemComment {
    pub(crate) fn from_ast_and_apply<TContext>(
        ast: &Comment,
        context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let target = ast
            .canonical_target()
            .map_err(|error| AnalysisError::unexpected_input_error(&error.to_string()))?;
        let comment = Self::new(&target, ast.object_kind, &ast.text);
        let context = context.apply_comment(&comment)?;
        Ok(AstAndContextPair::new(
            SemAst::from_comment(&comment),
            context,
        ))
    }
}

impl SemAlterSequence {
    pub(crate) fn from_ast_and_apply<TContext>(
        ast: &AlterSequence,
        context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let mut context = context;
        let mut alter_sequence = Self::new(&FullName::from_ast_name(&ast.name)?, &[]);
        for option in &ast.options {
            if option.name == "owned_by" {
                let relation =
                    if let Some(requested) = CreateSequence::owned_table_from_ast_option(option)? {
                        if let Some(relation) = context.resolve_table_name(&requested) {
                            Some(relation)
                        } else {
                            context.report_analysis_problem(AnalysisProblem::table_not_found(
                                &requested,
                            ))?;
                            continue;
                        }
                    } else {
                        None
                    };
                alter_sequence = alter_sequence.with_owned_by(relation.as_ref());
            }
        }
        let context = context.apply_alter_sequence(&alter_sequence)?;
        Ok(AstAndContextPair::new(
            SemAst::alter_sequence(&alter_sequence),
            context,
        ))
    }
}

impl SemDropTable {
    pub(crate) fn from_ast_and_apply<TContext>(
        ast: &DropTable,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let mut names = Vec::new();
        for name in &ast.names {
            let table_name = TableName::from(FullName::from_ast_name(name)?);
            if context.get_table(&table_name).is_none() {
                context.report_analysis_problem(AnalysisProblem::table_not_found(&table_name))?;
            }
            names.push(table_name);
        }
        let drop_table = Self::new(&names, ast.if_exists, ast.cascade);
        context = context.apply_drop_table(&drop_table)?;
        Ok(AstAndContextPair::new(
            SemAst::from_drop_table(&drop_table),
            context,
        ))
    }
}

impl CreateSequence {
    pub(crate) fn from_ast_and_apply<TContext>(
        ast: &Sequence,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let mut sequence = Self::new(&FullName::from_ast_name(&ast.name)?)
            .with_if_not_exists(ast.if_not_exists)
            .with_temporary(ast.temporary);
        for option in &ast.options {
            if let Some(table) = Self::owned_table_from_ast_option(option)? {
                let Some(resolved) = context.resolve_table_name(&table) else {
                    context.report_analysis_problem(AnalysisProblem::table_not_found(&table))?;
                    continue;
                };
                let table = resolved;
                sequence.set_owned_by(&table);
            }
        }
        let context = context.apply_create_sequence(&sequence)?;
        Ok(AstAndContextPair::new(
            SemAst::from_create_sequence(&sequence),
            context,
        ))
    }
}

impl CreateEnum {
    pub(crate) fn from_ast_and_apply<TContext>(
        ast: &EnumType,
        context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let enum_type = Self::new(&FullName::from_ast_name(&ast.name)?, &ast.values);
        let context = context.apply_create_enum(&enum_type)?;
        Ok(AstAndContextPair::new(
            SemAst::from_create_enum(&enum_type),
            context,
        ))
    }
}

impl SemVariableSet {
    pub(crate) fn from_ast_and_apply<TContext>(
        ast: &VariableSet,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let [argument] = ast.arguments.as_slice() else {
            return AnalysisError::raise_unexpected_input("VariableSetStmt.args len!=1");
        };
        let outcome =
            SemScalarExpr::from_ast(argument, &mut context, &ScalarAnalysisContext::default());
        let value = outcome?;
        let variable_set = Self::new(&ast.name, &value);
        let context = context.apply_variable_set(&variable_set)?;
        Ok(AstAndContextPair::new(
            SemAst::from_variable_set(&variable_set),
            context,
        ))
    }
}

impl CreateExtension {
    pub(crate) fn from_ast_and_apply<TContext>(
        ast: &Extension,
        context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let extension = Self::from_name(&ast.name).with_if_not_exists(ast.if_not_exists);
        let context = context.apply_create_extension(&extension)?;
        Ok(AstAndContextPair::new(
            SemAst::from_create_extension(&extension),
            context,
        ))
    }
}

impl CreateView {
    pub(crate) fn from_ast_and_apply<TContext>(
        ast: &View,
        context: TContext,
        cache: &mut CacheContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let mut context = context;
        let requested = FullName::from_ast_name(&ast.name)?;
        if requested.schema().is_none() && !ast.temporary && context.get_default_schema().is_none()
        {
            context.report_analysis_problem(AnalysisProblem::view(
                &ViewProblem::invalid_target(&requested),
            ))?;
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        let name = context.creation_name(&requested, ast.temporary)?;
        let kind = if ast.materialized {
            Kind::materialized_view()
        } else {
            Kind::view()
        };
        if !context.permits_relation_creation(&name, kind, ast.if_not_exists, ast.replace)? {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        let outcome = SelectStatement::from_ast(&ast.query, &mut context, cache);
        let mut query = outcome?;
        query.rename_columns(&ast.aliases);
        let lineage = ViewColumnLineage::from_ast_query(&ast.query, &context, &query)?;
        let options = Self::from_ast_options(ast, &mut context)?;
        let mut view = Self::new(&name, &query, &[], &None, ast.materialized)
            .with_if_not_exists(ast.if_not_exists)
            .with_replace(ast.replace)
            .with_temporary(ast.temporary)
            .with_column_lineage(&lineage)
            .with_options(&options, ViewCheckOption::from_ast(&ast.check_option));
        let replacement_rule = if ast.replace {
            if let Some(existing) = context.get_view(&ViewName::from_full_name(&name))
                && let Some(references) = existing.rules()
            {
                view.retain_rule_references(references);
            }
            CreateRule::replace_view_definition(&context, &name, &query)?
        } else {
            None
        };
        let context = context.apply_create_view(&view)?;
        let context = if let Some(rule) = replacement_rule {
            context.register_rule(&rule)?
        } else {
            context
        };
        Ok(AstAndContextPair::new(
            SemAst::from_create_view(&view),
            context,
        ))
    }
}

use crate::ast::Statement;
impl SemAst {
    /// Analyze this common statement directly using the supplied context.
    ///
    /// No SQL reparsing or catalog-file discovery occurs. This uses the same
    /// resolver and diagnostic reporting as the normal semantic-analysis path.
    ///
    /// The caller retains and reuses `cache_context` across analyses against
    /// the same immutable schema base. It memoizes calculation facts without
    /// changing analysis results. The caller owns this association; the cache
    /// performs no environment validation.
    ///
    /// # Panics
    ///
    /// CREATE TRIGGER analysis is not implemented yet (#602 / #555).
    /// ALTER DEFAULT PRIVILEGES and CREATE FOREIGN TABLE interpretation remains
    /// unimplemented (#955 / #956), after syntax-version checks succeed.
    ///
    /// # Errors
    ///
    /// Returns a dialect mismatch, resolver, or diagnostic reporting error.
    /// The statement dialect must match the supplied context metadata.
    pub fn from_ast<TContext: ParseContext>(
        statement: &Statement,
        context: TContext,
        cache_context: &mut CacheContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        AnalysisError::check_dialect(statement.dialect(), context.dialect())?;
        Self::apply_ast_node(&statement.node, context, cache_context)
    }
}
#[cfg(test)]
mod test_statement_analyze {
    use crate::sem::SemAst;

    use crate::test_helpers::public_analysis::statement;

    use crate::test_helpers::create_cache_context;

    use crate::{
        ast::{CteDefinition, Dialect, HaveWithClause},
        sem::PgBuiltInType,
        test_helpers::TestParseContext,
    };

    #[rstest::rstest]
    #[case::postgres(Dialect::postgres())]
    #[cfg_attr(feature = "sqlite", case::sqlite(Dialect::sqlite()))]
    fn analyzes_structurally_inserted_dependencies(#[case] dialect: Dialect) {
        let mut cache_context = create_cache_context();
        let mut base = statement(
            "with existing as (select id from leaf) select id from existing",
            dialect,
        );
        let first = CteDefinition::new("root", &statement("select 1 as id", dialect)).unwrap();
        let second =
            CteDefinition::new("leaf", &statement("select id from root", dialect)).unwrap();
        let mut with = base.ensure_with_clause().unwrap();
        with.insert(0, &first).unwrap();
        with.insert(1, &second).unwrap();
        let mut context = TestParseContext::default().with_dialect(dialect);
        context.setup_type(PgBuiltInType::int4());
        let analyzed = SemAst::from_ast(&base, context, &mut cache_context).unwrap();
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let column = query.result_columns().get_column_at(0).unwrap();
        assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
        assert_eq!(column.is_not_null(), &Some(true));
    }
}
#[cfg(test)]
mod test_parameter_analysis {
    use crate::sem::SemAst;

    use crate::test_helpers::public_analysis::{analyze, context};

    use crate::{
        sem::parameter_resolution::inference_runs as parameter_resolution_inference_runs,
        test_helpers::{create_cache_context, schema_layer, source_span},
    };

    use crate::{
        ast::Dialect,
        context::ParseContext,
        names::OperatorName,
        parse_statements,
        sem::{BinaryOperatorDefinition, OperatorDefinition, PgBuiltInType},
    };
    use std::collections::BTreeMap;

    #[rstest::rstest]
    #[case::postgres("select $7 is null", Dialect::postgres())]
    #[cfg_attr(
        feature = "sqlite",
        case::sqlite("select ?7 is null", Dialect::sqlite())
    )]
    fn null_test_infers_any(#[case] sql: &str, #[case] dialect: Dialect) {
        let result = analyze(sql, dialect).unwrap();
        let inputs = result.context().parameter_constraints();
        assert_eq!(
            inputs
                .iter()
                .map(crate::sem::ParameterConstraint::position)
                .collect::<Vec<_>>(),
            [7]
        );
        assert!(inputs.parameter(7).unwrap().constraint().is_any());
        assert!(inputs.allows(&BTreeMap::from([(7, PgBuiltInType::text())])));
        let select = result.ast().clone().into_select_statemtnt().unwrap();
        let output = select.result_columns().get_column_at(0).unwrap();
        assert_eq!(output.col_type(), &Some(PgBuiltInType::bool()));
        assert_eq!(output.is_not_null(), &Some(true));
    }

    #[rstest::rstest]
    #[case::operator("select $1 + 1 as n")]
    #[case::cte("with x as (select $1 as n) select n + 1 as total from x")]
    #[case::derived("select n + 1 as total from (select $1 as n) x")]
    #[case::scalar("select (select $1) + 1 as total")]
    #[case::later_use("with x as (select $1 as n) select n, $1 + 1 as total from x")]
    fn operator_preserves_implicit_input_range(#[case] sql: &str) {
        let result = analyze(sql, Dialect::postgres()).unwrap();
        let constraint = result
            .context()
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint();
        assert!(!constraint.is_any());
        assert!(constraint.allows(&PgBuiltInType::int4()));
        assert!(constraint.allows(&PgBuiltInType::int2()));
        assert!(!constraint.allows(&PgBuiltInType::text()));
        assert_eq!(result.context().reported_problem_count(), 0);
    }

    #[rstest::rstest]
    #[case::direct("select $1 + 1 as n, lower($1) as s")]
    #[case::cte("with x as (select $1 + 1 as n) select lower($1) as s from x")]
    fn conflicting_uses_report_recoverable_problem(#[case] sql: &str) {
        let analyzed =
            analyze(sql, Dialect::postgres()).expect("analysis continues after a conflict");
        let problems = analyzed.context().reported_problems();
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].code(), "parameter_constraint");
        let metadata = problems[0].render_metadata().expect("parameter metadata");
        assert!(metadata.message().contains("$1"));
        assert!(
            !analyzed
                .context()
                .parameter_constraints()
                .allows(&BTreeMap::new())
        );
    }

    #[test]
    fn tied_best_overloads_are_not_accepted_as_a_type_range() {
        let mut cache_context = create_cache_context();
        let mut context = context();
        let int = PgBuiltInType::int4();
        let overloads = OperatorDefinition::binary(&[
            BinaryOperatorDefinition::new(&int, &int, &int, true),
            BinaryOperatorDefinition::new(&int, &int, &PgBuiltInType::text(), true),
        ]);
        context.set_get_operator_op_result(
            &OperatorName::from_parts(None, "+"),
            true,
            true,
            Some(&overloads),
        );
        let analyzed = SemAst::from_ast(
            &parse_statements("select $1 + 1", Dialect::postgres())
                .next()
                .unwrap()
                .unwrap(),
            context,
            &mut cache_context,
        )
        .expect("ambiguity is recoverable");
        assert_eq!(analyzed.context().reported_problem_count(), 1);
        assert_eq!(
            analyzed.context().reported_problems()[0].code(),
            "parameter_constraint"
        );
    }

    #[test]
    fn distinct_inputs_and_gaps_remain_independent() {
        let result = analyze(
            "select $7 + 1 as n, $2 is null as missing, $7 is null as repeated",
            Dialect::postgres(),
        )
        .unwrap();
        let inputs = result.context().parameter_constraints();
        assert_eq!(
            inputs
                .iter()
                .map(crate::sem::ParameterConstraint::position)
                .collect::<Vec<_>>(),
            [2, 7]
        );
        assert!(inputs.parameter(2).unwrap().constraint().is_any());
        assert!(!inputs.parameter(7).unwrap().constraint().is_any());
        assert!(inputs.parameter(1).is_none());
    }

    #[rstest::rstest]
    #[case::direct("select $1")]
    #[case::cte("with x as (select $1 as n) select n from x")]
    fn plain_projection_keeps_any_input_and_postgres_text_output(#[case] sql: &str) {
        let result = analyze(sql, Dialect::postgres()).unwrap();
        assert!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
                .is_any()
        );
        assert_eq!(
            result
                .context()
                .parameter_constraints()
                .result_column(0)
                .unwrap()
                .admissible_types(),
            Some([PgBuiltInType::text()].as_slice())
        );
        let select = result.ast().clone().into_select_statemtnt().unwrap();
        assert_eq!(
            select.result_columns().get_column_at(0).unwrap().col_type(),
            &Some(PgBuiltInType::text())
        );
    }
    #[test]
    fn subsequent_analysis_replaces_context_constraints() {
        let mut cache_context = create_cache_context();
        let first = analyze("select $7 + 1 as n", Dialect::postgres()).unwrap();
        assert!(
            first
                .context()
                .parameter_constraints()
                .parameter(7)
                .is_some()
        );
        let second = SemAst::from_ast(
            &parse_statements("select 1 as n", Dialect::postgres())
                .next()
                .unwrap()
                .unwrap(),
            first.into_context(),
            &mut cache_context,
        )
        .unwrap();
        assert!(
            second
                .context()
                .parameter_constraints()
                .parameter(7)
                .is_none()
        );
        assert!(
            second
                .context()
                .parameter_constraints()
                .result_column(0)
                .is_none()
        );
    }

    #[test]
    fn parameter_state_restarts_after_parameter_free_analysis() {
        let mut cache_context = create_cache_context();
        let first = analyze("select $7 + 1", Dialect::postgres()).unwrap();
        let plain = SemAst::from_ast(
            &parse_statements("select 1", Dialect::postgres())
                .next()
                .unwrap()
                .unwrap(),
            first.into_context(),
            &mut cache_context,
        )
        .unwrap();
        let third = SemAst::from_ast(
            &parse_statements("select $2 + 1", Dialect::postgres())
                .next()
                .unwrap()
                .unwrap(),
            plain.into_context(),
            &mut cache_context,
        )
        .unwrap();
        assert!(
            third
                .context()
                .parameter_constraints()
                .parameter(7)
                .is_none()
        );
        assert!(
            third
                .context()
                .parameter_constraints()
                .parameter(2)
                .is_some()
        );
    }

    #[rstest::rstest]
    #[case::query("with x as (select 1 as n) select n + 1 from x")]
    #[case::subquery("select (select 1) + 1 as n")]
    fn parameter_free_analysis_skips_inference(#[case] sql: &str) {
        let before = parameter_resolution_inference_runs();
        let result = analyze(sql, Dialect::postgres()).unwrap();
        assert_eq!(crate::sem::parameter_resolution::inference_runs(), before);
        assert!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .is_none()
        );
        assert!(
            result
                .context()
                .parameter_constraints()
                .result_column(0)
                .is_none()
        );
    }

    #[test]
    fn parameter_free_function_keeps_null_argument_resolution() {
        let before = parameter_resolution_inference_runs();
        let result = analyze("select lower(null) as s", Dialect::postgres()).unwrap();
        assert_eq!(crate::sem::parameter_resolution::inference_runs(), before);
        assert_eq!(result.context().reported_problem_count(), 0);
        let query = result.ast().clone().into_select_statemtnt().unwrap();
        assert_eq!(
            query.result_columns().get_column_at(0).unwrap().col_type(),
            &Some(PgBuiltInType::text())
        );
    }

    #[test]
    fn schema_loading_skips_parameter_inference() {
        use crate::{context::MutSourcePathWithSpan, names::SchemaName};
        let mut cache_context = create_cache_context();
        let before = parameter_resolution_inference_runs();
        let mut context = schema_layer(std::path::Path::new("schema.sql"), &context().freeze());
        context.set_source_path_with_span(&source_span());
        context.set_search_path(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
        let statements = parse_statements(
            "create table t (n int4 default 1); create view v as select n from t group by n having true window w as (order by n) order by n limit 1 offset 0;",
            Dialect::postgres(),
        );
        for statement in statements {
            let statement = statement.unwrap();
            context.set_statement_span(*statement.span());
            context = SemAst::from_ast(&statement, context, &mut cache_context)
                .unwrap()
                .into_context();
        }
        assert_eq!(crate::sem::parameter_resolution::inference_runs(), before);
        assert!(context.parameter_constraints().result_column(0).is_none());
    }

    #[test]
    fn inference_preserves_unrelated_name_resolution_diagnostics() {
        let baseline = analyze("select missing_column as absent", Dialect::postgres()).unwrap();
        let expected = baseline.context().reported_problems();
        assert!(!expected.is_empty());
        let before = parameter_resolution_inference_runs();
        let result = analyze(
            "select $1 + 1 as n, missing_column as absent",
            Dialect::postgres(),
        )
        .unwrap();
        assert_eq!(
            crate::sem::parameter_resolution::inference_runs(),
            before + 1
        );
        assert_eq!(
            format!("{:?}", result.context().reported_problems()),
            format!("{expected:?}")
        );
        assert!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .is_some()
        );
    }

    #[test]
    fn concrete_scalar_subquery_exposes_its_scalar_type() {
        let result = analyze("select (select 1) + 1 as n", Dialect::postgres()).unwrap();
        assert_eq!(result.context().reported_problem_count(), 0);
        let query = result.ast().clone().into_select_statemtnt().unwrap();
        assert_eq!(
            query.result_columns().get_column_at(0).unwrap().col_type(),
            &Some(PgBuiltInType::int4())
        );
    }

    #[test]
    fn diagnostic_sink_failure_still_aborts_parameter_analysis() {
        let mut cache_context = create_cache_context();
        let mut context = context();
        context.reject_diagnostics_after(0);
        let error = SemAst::from_ast(
            &parse_statements("select $1 + 1, lower($1)", Dialect::postgres())
                .next()
                .unwrap()
                .unwrap(),
            context,
            &mut cache_context,
        )
        .err()
        .expect("diagnostic reporting must fail");
        assert_eq!(error.to_string(), "diagnostic sink rejected a problem");
    }

    #[rstest::rstest]
    #[case::cast("select $1::missing")]
    #[case::function("select missing_function($1)")]
    #[case::operator("select $1 @@@ 1")]
    fn unresolved_parameter_operations_report_once(#[case] sql: &str) {
        let result = analyze(sql, Dialect::postgres())
            .expect("analysis recovers from unresolved operations");
        assert_eq!(result.context().reported_problem_count(), 1);
        assert!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .is_some()
        );
    }
}
#[cfg(test)]
mod test_parameter_casts {
    use crate::sem::SemAst;

    #[cfg(feature = "sqlite")]
    use crate::names::FullName;
    #[cfg(feature = "sqlite")]
    use crate::sem::TypeReference;
    use crate::test_helpers::create_cache_context;

    use crate::{
        ast::Dialect, context::ParseContext, parse_statements, sem::PgBuiltInType,
        test_helpers::TestParseContext,
    };

    #[rstest::rstest]
    #[case::boolean("select $1 as flag where $1")]
    #[case::not("select not $1 as flag")]
    fn boolean_context_constrains_input(#[case] sql: &str) {
        let mut cache_context = create_cache_context();
        let result = SemAst::from_ast(
            &parse_statements(sql, Dialect::postgres())
                .next()
                .unwrap()
                .unwrap(),
            TestParseContext::default(),
            &mut cache_context,
        )
        .unwrap();
        let input = result
            .context()
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint();
        assert!(input.allows(&PgBuiltInType::bool()));
        assert!(!input.allows(&PgBuiltInType::int4()));
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::projection("select cast(?1 as integer) as n")]
    #[case::cte("with x as (select cast(?1 as integer) as n) select n from x")]
    fn sqlite_casts_do_not_require_postgres_casts(#[case] sql: &str) {
        let mut cache_context = create_cache_context();
        let mut context = TestParseContext::default().with_dialect(Dialect::sqlite());
        context.setup_type(TypeReference::from_full_name(FullName::from_local_name(
            "integer",
        )));
        let result = SemAst::from_ast(
            &parse_statements(sql, Dialect::sqlite())
                .next()
                .unwrap()
                .unwrap(),
            context,
            &mut cache_context,
        )
        .unwrap();
        assert!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
                .is_any()
        );
        assert_eq!(result.context().reported_problem_count(), 0);
        let select = result.ast().clone().into_select_statemtnt().unwrap();
        assert!(
            select
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .col_type()
                .is_some()
        );
    }
}
impl SemAst {
    fn from_insert_resolution(value: Option<&InsertStatement>) -> Self {
        value.map_or_else(Self::unresolved_statement, Self::insert)
    }
    fn from_update_resolution(value: Option<&UpdateStatement>) -> Self {
        value.map_or_else(Self::unresolved_statement, Self::update)
    }
    fn from_create_rule_resolution(value: Option<&CreateRule>) -> Self {
        value.map_or_else(Self::unresolved_statement, Self::create_rule)
    }
    fn from_drop_trigger_resolution(value: Option<&DropTrigger>) -> Self {
        value.map_or_else(Self::unresolved_statement, Self::drop_trigger)
    }
    fn from_create_trigger_resolution(value: Option<&CreateTrigger>) -> Self {
        value.map_or_else(Self::unresolved_statement, Self::create_trigger)
    }
    fn from_drop_rule_resolution(value: Option<&DropRule>) -> Self {
        value.map_or_else(Self::unresolved_statement, Self::drop_rule)
    }
    pub(crate) fn create_composite_type(value: &CreateCompositeType) -> Self {
        Self::CreateCompType(value.clone())
    }
}

impl SemAst {
    pub(crate) fn from_comment(value: &SemComment) -> Self {
        Self::Comment(value.clone())
    }
    pub(crate) fn from_drop_table(value: &SemDropTable) -> Self {
        Self::DropTable(value.clone())
    }
    pub(crate) fn from_create_sequence(value: &CreateSequence) -> Self {
        Self::CreateSequence(value.clone())
    }
    pub(crate) fn from_create_enum(value: &CreateEnum) -> Self {
        Self::CreateEnum(value.clone())
    }
    pub(crate) fn from_variable_set(value: &SemVariableSet) -> Self {
        Self::VariableSet(value.clone())
    }
    pub(crate) fn from_create_extension(value: &CreateExtension) -> Self {
        Self::CreateExtension(value.clone())
    }
    pub(crate) fn from_create_view(value: &CreateView) -> Self {
        Self::CreateView(value.clone())
    }
}
