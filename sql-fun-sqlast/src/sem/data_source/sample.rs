use crate::{
    names::FullName,
    sem::{
        AnalysisError, AnalysisProblem, DataSource, MinMaxExpr, ParseContext, SamplingMethodLookup,
        SemScalarExpr, TypeReference,
        problem::TableSampleProblem,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

/// A sampled relation with its completed method lookup and analyzed inputs.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TableSampleSource {
    source: DataSource,
    requested_method: FullName,
    method: SamplingMethodLookup,
    arguments: Vec<SamplingInput>,
    repeatable: Option<SamplingInput>,
    relation_valid: bool,
}
impl TableSampleSource {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        source: &DataSource,
        name: &FullName,
        arguments: &[SemScalarExpr],
        repeatable: &Option<SemScalarExpr>,
    ) -> Result<Self, AnalysisError> {
        let relation_valid = source.validate_sampling(context)?;
        let method = context.lookup_sampling_method(name);
        method.validate_clause(context, name, arguments.len(), repeatable.is_some())?;
        let inputs = SamplingInput::bind_arguments(arguments, &method);
        let mut value = Self {
            source: source.clone(),
            requested_method: name.clone(),
            method,
            arguments: inputs,
            repeatable: repeatable.as_ref().map(|value| {
                SamplingInput::new(
                    value,
                    Some(&TypeReference::from_master_type_name("float8")),
                    true,
                )
            }),
            relation_valid,
        };
        value.validate_inputs(context)?;
        Ok(value)
    }
}

#[cfg(test)]
mod test_sampling_semantics {
    use crate::{
        names::SchemaName,
        sem::{SamplingMethod, data_source::sample::TableSampleSource},
        test_helpers::sampling::with_views,
    };

    use crate::{
        names::FullName,
        sem::{ParseContext, PgBuiltInType, SamplingMethodLookup},
        test_helpers::sampling::{context, query, sources},
    };
    use std::collections::BTreeMap;

    #[rstest::rstest]
    #[case::system("SELECT s.* FROM sampled AS s(x,y) TABLESAMPLE system(10) REPEATABLE(1)")]
    #[case::bernoulli("SELECT x,y FROM sampled AS s(x,y) TABLESAMPLE pg_catalog.bernoulli(10)")]
    fn retains_binding_aliases_and_columns(#[case] sql: &str) -> testresult::TestResult {
        let mut context = context();
        let query = query(sql, &mut context)?;
        assert!(
            context.reported_problems().is_empty(),
            "{:?}",
            context.reported_problems()
        );
        let sources = sources(&query);
        let sample = &sources[0];
        assert_eq!(
            sample
                .method()
                .method()
                .unwrap()
                .name()
                .schema()
                .unwrap()
                .as_str(),
            "pg_catalog"
        );
        assert!(sample.relation_valid());
        assert_eq!(
            sample.arguments()[0].expression().get_type(),
            Some(crate::sem::TypeReference::from_master_type_name("float4"))
        );
        let columns = query.result_columns();
        assert_eq!(columns.len(), 2);
        assert_eq!(
            columns
                .get_column_at(0)
                .unwrap()
                .column_name()
                .as_ref()
                .unwrap()
                .as_str(),
            "x"
        );
        assert_eq!(
            columns.get_column_at(1).unwrap().col_type(),
            &Some(PgBuiltInType::text())
        );
        let encoded = ::serde_json::to_string(sample)?;
        let restored: TableSampleSource = ::serde_json::from_str(&encoded)?;
        assert_eq!(sample, &restored);
        Ok(())
    }

    #[rstest::rstest]
    #[case::custom("SELECT * FROM sampled TABLESAMPLE custom_sample(1, 'x')", None)]
    #[case::arity(
        "SELECT * FROM sampled TABLESAMPLE system(1,2)",
        Some("tablesample_arity")
    )]
    #[case::type_error(
        "SELECT * FROM sampled TABLESAMPLE system(true)",
        Some("tablesample_argument_type")
    )]
    #[case::seed(
        "SELECT * FROM sampled TABLESAMPLE system(1) REPEATABLE(true)",
        Some("tablesample_seed_type")
    )]
    #[case::capability(
        "SELECT * FROM sampled TABLESAMPLE custom_sample(1,'x') REPEATABLE(1)",
        Some("tablesample_repeatable")
    )]
    #[case::missing(
        "SELECT * FROM sampled TABLESAMPLE absent507(1)",
        Some("tablesample_method_not_found")
    )]
    #[case::local_reference(
        "SELECT * FROM sampled TABLESAMPLE system(sampled.id)",
        Some("undefined_alias")
    )]
    #[case::row(
        "SELECT * FROM sampled TABLESAMPLE system(ROW(1,2))",
        Some("tablesample_argument_type")
    )]
    #[case::subquery("SELECT * FROM sampled TABLESAMPLE system((SELECT 10))", None)]
    #[case::scalar("SELECT * FROM sampled TABLESAMPLE system(scalar507(1))", None)]
    #[case::aggregate(
        "SELECT * FROM sampled TABLESAMPLE system(aggregate507(1))",
        Some("tablesample_expression")
    )]
    #[case::set(
        "SELECT * FROM sampled TABLESAMPLE system(set507(1))",
        Some("tablesample_expression")
    )]
    #[case::deferred_set(
        "SELECT * FROM sampled TABLESAMPLE system(set507($1))",
        Some("tablesample_expression")
    )]
    #[case::nested_aggregate(
        "SELECT * FROM sampled TABLESAMPLE system((SELECT aggregate507(1)))",
        None
    )]
    #[case::preceding(
        "SELECT * FROM sampled a, sampled b TABLESAMPLE system(a.id)",
        Some("undefined_alias")
    )]
    #[case::cte(
        "WITH c AS (SELECT * FROM sampled) SELECT * FROM c TABLESAMPLE system(1)",
        Some("tablesample_relation")
    )]
    #[case::null(
        "SELECT * FROM sampled TABLESAMPLE system(NULL) REPEATABLE(NULL)",
        None
    )]
    #[case::unknown_literal("SELECT * FROM sampled TABLESAMPLE system('10')", None)]
    #[case::window(
        "SELECT * FROM sampled TABLESAMPLE system(scalar507(1) OVER ())",
        Some("tablesample_expression")
    )]
    #[case::seed_set(
        "SELECT * FROM sampled TABLESAMPLE system(1) REPEATABLE(set507(1))",
        Some("tablesample_expression")
    )]
    #[case::outer_reference(
        "SELECT (SELECT s.id FROM sampled s TABLESAMPLE system(a.id)) FROM sampled a",
        None
    )]
    fn validates_method_and_inputs(
        #[case] sql: &str,
        #[case] code: Option<&str>,
    ) -> testresult::TestResult {
        let mut context = context();
        let analyzed = query(sql, &mut context)?;
        assert_eq!(sources(&analyzed).len(), 1);
        if let Some(code) = code {
            assert!(
                context.has_reported_problem(code),
                "{:?}",
                context.reported_problems()
            );
        } else {
            assert!(
                context.reported_problems().is_empty(),
                "{:?}",
                context.reported_problems()
            );
        }
        Ok(())
    }

    #[rstest::rstest]
    #[case::unavailable(
        SamplingMethodLookup::unavailable(),
        "tablesample_metadata_unavailable"
    )]
    #[case::ambiguous(SamplingMethodLookup::ambiguous(&[FullName::with_schema("a","system"), FullName::with_schema("b","system")]), "tablesample_method_ambiguous")]
    #[case::invalid(SamplingMethodLookup::invalid_handler(&FullName::with_schema("pg_catalog","system")), "tablesample_invalid_handler")]
    fn retains_lookup_failures_and_nested_inputs(
        #[case] lookup: SamplingMethodLookup,
        #[case] code: &str,
    ) -> testresult::TestResult {
        let mut context = context();
        context.set_sampling_method(&FullName::with_schema("pg_catalog", "system"), &lookup);
        let analyzed = query(
            "SELECT * FROM sampled TABLESAMPLE system($1) REPEATABLE($2)",
            &mut context,
        )?;
        let samples = sources(&analyzed);
        assert_eq!(samples[0].method(), &lookup);
        assert_eq!(samples[0].arguments().len(), 1);
        assert!(samples[0].repeatable().is_some());
        assert!(context.has_reported_problem(code));
        let next = query("SELECT 1", &mut context)?;
        assert_eq!(next.result_columns().len(), 1);
        Ok(())
    }

    #[rstest::rstest]
    #[case::ordinary("ordinary507", false)]
    #[case::materialized("materialized507", true)]
    fn validates_relation_kind(#[case] name: &str, #[case] valid: bool) -> testresult::TestResult {
        let mut context = with_views(context());
        let analyzed = query(
            &format!("SELECT * FROM {name} TABLESAMPLE system(1)"),
            &mut context,
        )?;
        assert_eq!(sources(&analyzed)[0].relation_valid(), valid);
        assert_eq!(context.has_reported_problem("tablesample_relation"), !valid);
        assert_eq!(analyzed.result_columns().len(), 2);
        Ok(())
    }

    #[rstest::rstest]
    #[case::qualified(
        "SELECT public.sampled.* FROM public.sampled TABLESAMPLE system(1)",
        Some(true)
    )]
    #[case::outer_join(
        "SELECT s.id,s.label FROM sampled a LEFT JOIN sampled s TABLESAMPLE system(1) ON true",
        Some(false)
    )]
    fn delegates_projection_and_nullability(
        #[case] sql: &str,
        #[case] not_null: Option<bool>,
    ) -> testresult::TestResult {
        let mut context = context();
        let analyzed = query(sql, &mut context)?;
        assert!(
            context.reported_problems().is_empty(),
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(
            analyzed
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .is_not_null(),
            &not_null
        );
        assert_eq!(
            analyzed
                .result_columns()
                .get_column_at(1)
                .unwrap()
                .is_not_null(),
            &Some(false)
        );
        Ok(())
    }

    // PostgreSQL 15/18 EXPLAIN oracles use sum, abs and generate_series
    // respectively for aggregate507, scalar507 and set507 (one output row).
    #[rstest::rstest]
    #[case::aggregate_local("aggregate507(1)", false)]
    #[case::aggregate_outer("aggregate507(a.id)", true)]
    #[case::aggregate_nested_outer("scalar507(aggregate507(a.id))", true)]
    #[case::aggregate_subquery("(SELECT aggregate507(1))", true)]
    #[case::aggregate_subquery_outer("(SELECT aggregate507(a.id))", true)]
    #[case::window_direct("aggregate507(1) OVER ()", false)]
    #[case::window_nested_call("scalar507(aggregate507(1) OVER ())", false)]
    #[case::window_subquery("(SELECT aggregate507(1) OVER ())", true)]
    #[case::window_outer("aggregate507(a.id) OVER ()", false)]
    #[case::window_subquery_outer("(SELECT aggregate507(a.id) OVER ())", true)]
    #[case::srf_direct("set507(1)", false)]
    #[case::srf_nested_call("scalar507(set507(1))", false)]
    #[case::srf_subquery("(SELECT set507(1))", true)]
    #[case::srf_outer("set507(a.id)", false)]
    #[case::srf_subquery_outer("(SELECT set507(a.id))", true)]
    #[case::scalar("scalar507(1)", true)]
    fn respects_expression_placement_and_query_ownership(
        #[case] expression: &str,
        #[case] valid: bool,
        #[values(false, true)] seed: bool,
    ) -> testresult::TestResult {
        let clause = if seed {
            format!("system(1) REPEATABLE({expression})")
        } else {
            format!("system({expression})")
        };
        let sql = format!(
            "SELECT (SELECT s.id FROM sampled s TABLESAMPLE {clause} LIMIT 1) FROM sampled a"
        );
        let mut context = context();
        query(&sql, &mut context)?;
        assert_eq!(
            context.reported_problem_count(),
            usize::from(!valid),
            "{sql}: {:?}",
            context.reported_problems()
        );
        assert_eq!(
            context.has_reported_problem("tablesample_expression"),
            !valid
        );
        Ok(())
    }

    #[test]
    fn lookup_obeys_search_path_and_explicit_qualification() -> testresult::TestResult {
        let mut context = context();
        let public = FullName::with_schema("public", "system");
        let method = SamplingMethod::new(&public, &[PgBuiltInType::text()], false);
        context.set_sampling_method(&public, &SamplingMethodLookup::found(&method));
        let implicit = query("SELECT * FROM sampled TABLESAMPLE system(1)", &mut context)?;
        assert_eq!(
            sources(&implicit)[0].method().method().unwrap().name(),
            &FullName::with_schema("pg_catalog", "system")
        );
        context.set_get_search_path_result(&[
            SchemaName::from("public"),
            SchemaName::from("pg_catalog"),
        ]);
        let selected = query(
            "SELECT * FROM sampled TABLESAMPLE system('x')",
            &mut context,
        )?;
        assert_eq!(
            sources(&selected)[0].method().method().unwrap().name(),
            &public
        );
        let explicit = query(
            "SELECT * FROM sampled TABLESAMPLE pg_catalog.system(1)",
            &mut context,
        )?;
        assert_eq!(
            sources(&explicit)[0].method().method().unwrap().name(),
            &FullName::with_schema("pg_catalog", "system")
        );
        assert!(
            context.reported_problems().is_empty(),
            "{:?}",
            context.reported_problems()
        );
        Ok(())
    }

    #[test]
    fn failed_method_lookup_does_not_discard_expression_failures() -> testresult::TestResult {
        let mut context = context();
        let analyzed = query(
            "SELECT * FROM sampled TABLESAMPLE absent507(undefined507)",
            &mut context,
        )?;
        assert!(context.has_reported_problem("tablesample_method_not_found"));
        assert!(
            context.has_reported_problem("column_not_found_in_source"),
            "{:?}",
            context.reported_problems()
        );
        assert!(sources(&analyzed)[0].method().is_not_found());
        assert!(sources(&analyzed)[0].arguments()[0].target_type().is_none());
        Ok(())
    }

    #[test]
    fn custom_inputs_and_other_occurrences_share_constraints() -> testresult::TestResult {
        let mut context = context();
        let analyzed = query(
            "SELECT $2 FROM sampled TABLESAMPLE custom_sample($1,$2)",
            &mut context,
        )?;
        assert!(
            context.reported_problems().is_empty(),
            "{:?}",
            context.reported_problems()
        );
        assert!(context.parameter_constraints().allows(&BTreeMap::from([
            (1, PgBuiltInType::int4()),
            (2, PgBuiltInType::text())
        ])));
        assert!(!context.parameter_constraints().allows(&BTreeMap::from([
            (1, PgBuiltInType::int4()),
            (2, PgBuiltInType::bool())
        ])));
        assert!(sources(&analyzed)[0].repeatable().is_none());
        Ok(())
    }

    #[test]
    fn incompatible_occurrences_report_parameter_conflict() -> testresult::TestResult {
        let mut context = context();
        query(
            "SELECT $1::boolean FROM sampled TABLESAMPLE system($1)",
            &mut context,
        )?;
        assert!(
            context.has_reported_problem("parameter_constraint"),
            "{:?}",
            context.reported_problems()
        );
        assert!(!context.has_reported_problem("tablesample_argument_type"));
        Ok(())
    }

    #[test]
    fn sampling_inputs_participate_in_parameter_constraints() -> testresult::TestResult {
        let mut context = context();
        let analyzed = query(
            "SELECT * FROM sampled TABLESAMPLE system($1) REPEATABLE($2)",
            &mut context,
        )?;
        let constraints = context.parameter_constraints();
        assert!(constraints.allows(&BTreeMap::from([
            (
                1,
                crate::sem::TypeReference::from_master_type_name("float4")
            ),
            (
                2,
                crate::sem::TypeReference::from_master_type_name("float8")
            )
        ])));
        assert!(!constraints.allows(&BTreeMap::from([
            (1, PgBuiltInType::bool()),
            (
                2,
                crate::sem::TypeReference::from_master_type_name("float8")
            )
        ])));
        assert!(!constraints.allows(&BTreeMap::from([
            (
                1,
                crate::sem::TypeReference::from_master_type_name("float4")
            ),
            (2, PgBuiltInType::bool())
        ])));
        let samples = sources(&analyzed);
        assert!(samples[0].arguments()[0].target_type().is_some());
        Ok(())
    }
}

impl TableSampleSource {
    /// The sampled relation, retaining its original column and alias bindings.
    pub const fn source(&self) -> &DataSource {
        &self.source
    }
    /// Method spelling supplied by the SQL statement.
    pub const fn requested_method(&self) -> &FullName {
        &self.requested_method
    }
    /// Completed lookup, independent of expression type validation.
    pub const fn method(&self) -> &SamplingMethodLookup {
        &self.method
    }
    /// Ordered analyzed sampling inputs.
    pub fn arguments(&self) -> &[SamplingInput] {
        &self.arguments
    }
    /// Explicit seed, absent when REPEATABLE was omitted.
    pub const fn repeatable(&self) -> Option<&SamplingInput> {
        self.repeatable.as_ref()
    }
    /// Whether the resolved relation supports TABLESAMPLE.
    pub const fn relation_valid(&self) -> bool {
        self.relation_valid
    }
    pub(crate) fn validate_inputs(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        for argument in &mut self.arguments {
            argument.validate(context)?;
        }
        if let Some(seed) = &mut self.repeatable {
            seed.validate(context)?;
        }
        Ok(())
    }
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.source.walk(visitor)?;
        for argument in &mut self.arguments {
            argument.walk(visitor)?;
        }
        if let Some(seed) = &mut self.repeatable {
            seed.walk(visitor)?;
        }
        visitor.table_sample(self)
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        self.source.visit(visitor)?;
        for argument in &self.arguments {
            argument.visit(visitor)?;
        }
        if let Some(seed) = &self.repeatable {
            seed.visit(visitor)?;
        }
        visitor.table_sample(self)
    }
}

/// An analyzed TABLESAMPLE input with its required assignment-coercion target.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SamplingInput {
    expression: SemScalarExpr,
    target: Option<TypeReference>,
    seed: bool,
    checked_type: Option<TypeReference>,
    type_valid: Option<bool>,
    invalid_expression: bool,
}
impl SamplingInput {
    fn bind_arguments(arguments: &[SemScalarExpr], method: &SamplingMethodLookup) -> Vec<Self> {
        let targets = match method.method() {
            Some(method) => method.input_targets(arguments.len()),
            None => None,
        };
        arguments
            .iter()
            .enumerate()
            .map(|(index, argument)| {
                let target = match targets {
                    Some(types) => types.get(index),
                    None => None,
                };
                Self::new(argument, target, false)
            })
            .collect()
    }
    fn new(expression: &SemScalarExpr, target: Option<&TypeReference>, seed: bool) -> Self {
        Self {
            expression: expression.clone(),
            target: target.cloned(),
            seed,
            checked_type: None,
            type_valid: None,
            invalid_expression: false,
        }
    }
    /// Retained operand, including nested references and explicit failure outcomes.
    pub const fn expression(&self) -> &SemScalarExpr {
        &self.expression
    }
    /// Required input type, if method metadata and arity establish it.
    pub const fn target_type(&self) -> Option<&TypeReference> {
        self.target.as_ref()
    }
    /// Completed type applicability, or None while type/metadata is unresolved.
    pub const fn type_valid(&self) -> Option<bool> {
        self.type_valid
    }
    fn validate(&mut self, context: &mut impl ParseContext) -> Result<(), AnalysisError> {
        self.validate_placement(context)?;
        self.validate_type(context)
    }
    pub(crate) fn validate_aggregate_placement(
        context: &mut impl ParseContext,
        owner_placement: &str,
    ) -> Result<(), AnalysisError> {
        if matches!(owner_placement, "TABLESAMPLE" | "REPEATABLE") {
            context.report_analysis_problem(AnalysisProblem::table_sample(
                TableSampleProblem::expression(owner_placement),
            ))?;
        }
        Ok(())
    }
    fn validate_placement(&mut self, context: &mut impl ParseContext) -> Result<(), AnalysisError> {
        let mut placement = SamplingPlacement {
            context,
            depth: 0,
            invalid: false,
        };
        self.expression.walk(&mut placement)?;
        let invalid = placement.invalid;
        if invalid && !self.invalid_expression {
            context.report_analysis_problem(AnalysisProblem::table_sample(
                TableSampleProblem::expression(if self.seed {
                    "REPEATABLE"
                } else {
                    "TABLESAMPLE"
                }),
            ))?;
            self.invalid_expression = true;
        }
        Ok(())
    }
    fn validate_type(&mut self, context: &mut impl ParseContext) -> Result<(), AnalysisError> {
        let Some(target) = &self.target else {
            return Ok(());
        };
        let Some(source) = MinMaxExpr::input_type(&self.expression) else {
            return Ok(());
        };
        if self.checked_type.as_ref() == Some(&source) {
            return Ok(());
        }
        let valid = source == MinMaxExpr::unknown()
            || source == *target
            || source.assignment_match_context(target, context).is_some();
        self.checked_type = Some(source.clone());
        self.type_valid = Some(valid);
        if valid {
            if source != *target && self.expression.constraint_node().is_none() {
                self.expression = SemScalarExpr::wrap_implicit_cast(target, &self.expression);
                self.checked_type = Some(target.clone());
            }
        } else {
            let target: ::sql_fun_core::analysis_problem::TypeReference = target.into();
            let source: ::sql_fun_core::analysis_problem::TypeReference = (&source).into();
            let detail = format!("expected {target}, supplied {source}");
            let problem = if self.seed {
                TableSampleProblem::seed_type(&detail)
            } else {
                TableSampleProblem::argument_type(&detail)
            };
            context.report_analysis_problem(AnalysisProblem::table_sample(problem))?;
        }
        Ok(())
    }
    fn walk<V: SemanticVisitorMut>(&mut self, visitor: &mut V) -> Result<(), AnalysisError> {
        self.expression.walk(visitor)?;
        if let Some(target) = &self.target {
            visitor.sampling_input(&self.expression, target);
        }
        Ok(())
    }
    fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        self.expression.visit(visitor)?;
        if let Some(target) = &self.target {
            visitor.sampling_input(&self.expression, target);
        }
        Ok(())
    }
}

struct SamplingPlacement<'a, C: ParseContext> {
    context: &'a C,
    depth: usize,
    invalid: bool,
}
impl<C: ParseContext> SemanticVisitorMut for SamplingPlacement<'_, C> {
    fn enter_query(&mut self) {
        self.depth += 1;
    }
    fn leave_query(&mut self) {
        self.depth -= 1;
    }
    fn expression(&mut self, value: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        if self.depth == 0 && value.is_invalid_sampling_function(self.context) {
            self.invalid = true;
        }
        Ok(())
    }
}

impl DataSource {
    pub(crate) fn sampled(source: &TableSampleSource) -> Self {
        Self::RangeTableSample(Box::new(source.clone()))
    }
    /// Sampling source semantics, if this source has a TABLESAMPLE clause.
    pub const fn as_table_sample(&self) -> Option<&TableSampleSource> {
        if let Self::RangeTableSample(sample) = self {
            Some(sample)
        } else {
            None
        }
    }
    fn validate_sampling(&self, context: &mut impl ParseContext) -> Result<bool, AnalysisError> {
        let valid = self.permits_sampling(context);
        if !valid && self.projection_shape_is_resolved() {
            context.report_analysis_problem(AnalysisProblem::table_sample(
                TableSampleProblem::relation("FROM source"),
            ))?;
        }
        Ok(valid)
    }
    fn permits_sampling(&self, context: &impl ParseContext) -> bool {
        match self {
            Self::Table(_) => true,
            Self::View(view) => context
                .get_view(view.view_name())
                .is_some_and(|view| view.is_materialized()),
            _ => false,
        }
    }
}
