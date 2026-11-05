use crate::{
    ast::Dialect,
    names::{AliasName, ColumnName, FullName},
    sem::{
        AnalysisError, AnalysisProblem, DataSource, ParseContext, WithClause,
        data_source::{AliasSpec, MergeRecordRole, QualifiedSource},
    },
};
use ::sql_fun_core::LookupResult;

/// Query-local namespace entries retain occurrences, rather than just local names.
#[derive(Debug, Default)]
pub(crate) struct SourceNamespace<'a> {
    bindings: Vec<SourceBinding<'a>>,
}

#[derive(Debug, Clone)]
struct SourceBinding<'a> {
    alias: AliasSpec,
    source: QualifiedSource<'a>,
    relation: Option<FullName>,
    unqualified: bool,
}

impl<'a> SourceBinding<'a> {
    fn new(source: &'a DataSource, alias: &AliasSpec) -> Self {
        Self {
            alias: alias.clone(),
            source: QualifiedSource::new(source, alias.name()),
            relation: source.unaliased_relation(alias),
            unqualified: true,
        }
    }

    fn conflicts(&self, other: &Self, context: &impl ParseContext) -> bool {
        if self.alias.name() != other.alias.name() {
            return false;
        }
        if context.dialect() == Dialect::postgres()
            && let (Some(left), Some(right)) = (&self.relation, &other.relation)
        {
            return left == right;
        }
        true
    }
}

impl<'a> SourceNamespace<'a> {
    pub(super) fn push(&mut self, source: &'a DataSource, alias: &AliasSpec) {
        self.bindings.push(SourceBinding::new(source, alias));
    }

    pub(crate) fn append(&mut self, mut other: Self) {
        self.bindings.append(&mut other.bindings);
    }

    pub(super) fn with_nullability(mut self, nullable: bool) -> Self {
        self.bindings = self
            .bindings
            .into_iter()
            .map(|mut binding| {
                binding.source = binding.source.with_nullability(nullable);
                binding
            })
            .collect();
        self
    }

    pub(super) fn with_merge(mut self, role: MergeRecordRole, nullable: bool) -> Self {
        self.bindings = self
            .bindings
            .into_iter()
            .map(|mut binding| {
                binding.source = binding.source.with_merge(role, nullable);
                binding
            })
            .collect();
        self
    }

    pub(super) fn hide_group_columns(&mut self) {
        for binding in &mut self.bindings {
            binding.unqualified = false;
        }
    }

    pub(crate) fn validate_against(
        &self,
        previous: &Self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        for binding in &self.bindings {
            if previous
                .bindings
                .iter()
                .any(|other| binding.conflicts(other, context))
            {
                context.report_analysis_problem(AnalysisProblem::duplicate_alias(
                    binding.alias.name(),
                ))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_distinct_relations_bind_without_problems {

    use crate::{
        names::FullName, sem::PgBuiltInType, test_helpers::query::source_namespace_statement,
    };

    #[rstest::rstest]
    #[case::comma("SELECT a.t.id, b.t.id FROM a.t, b.t")]
    #[case::reversed("SELECT a.t.id, b.t.id FROM b.t, a.t")]
    #[case::cross_join("SELECT a.t.id, b.t.id FROM a.t CROSS JOIN b.t")]
    #[case::inner_join("SELECT a.t.id, b.t.id FROM a.t JOIN b.t ON true")]
    #[case::search_path("SELECT a.t.id, b.t.id FROM t, b.t")]
    #[case::join_on("SELECT a.t.id, b.t.id FROM a.t JOIN b.t ON a.t.id IS NOT NULL")]
    fn distinct_relations_bind_without_problems(#[case] sql: &str) {
        let analyzed = source_namespace_statement(sql);
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        for (index, schema, ty) in [
            (0, "a", PgBuiltInType::int4()),
            (1, "b", PgBuiltInType::int8()),
        ] {
            let column = query.result_columns().get_column_at(index).unwrap();
            assert_eq!(
                column
                    .expression()
                    .unwrap()
                    .table_column_reference()
                    .unwrap()
                    .relation(),
                &FullName::with_schema(schema, "t")
            );
            assert_eq!(column.col_type(), &Some(ty));
        }
    }

    #[rstest::rstest]
    #[case::identical("SELECT 1 FROM a.t, a.t")]
    #[case::different_spelling("SELECT 1 FROM a.t, t")]
    #[case::explicit_same("SELECT 1 FROM a.t AS t, b.t")]
    #[case::explicit_right("SELECT 1 FROM a.t, b.t AS t")]
    #[case::both_explicit("SELECT 1 FROM a.t AS x, b.t AS x")]
    #[case::cte("WITH t AS (SELECT 1 AS id) SELECT 1 FROM t, a.t")]
    #[case::derived("SELECT 1 FROM (SELECT 1 AS id) AS t, b.t")]
    #[case::join("SELECT 1 FROM a.t CROSS JOIN a.t")]
    #[case::repeated_view("SELECT 1 FROM a.v, a.v")]
    #[case::update("UPDATE a.t SET id=1 FROM a.t")]
    #[case::delete("DELETE FROM a.t USING a.t")]
    #[case::merge("MERGE INTO a.t USING b.t ON true WHEN MATCHED THEN DELETE")]
    #[case::merge_ambiguous_source(
        "MERGE INTO a.t USING (b.t CROSS JOIN c.t) ON true WHEN MATCHED THEN DELETE"
    )]
    fn genuine_source_collisions_remain_invalid(#[case] sql: &str) {
        let analyzed = source_namespace_statement(sql);
        assert!(
            analyzed
                .context()
                .reported_problems()
                .iter()
                .any(|problem| problem.code() == "duplicate_alias"),
            "{:?}",
            analyzed.context().reported_problems()
        );
    }

    #[rstest::rstest]
    #[case::views("SELECT a.v.id, b.v.id FROM a.v, b.v")]
    #[case::mixed("SELECT a.t.id, c.t.id FROM a.t, c.t")]
    fn views_share_the_relation_collision_rule(#[case] sql: &str) {
        let analyzed = source_namespace_statement(sql);
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        assert_eq!(
            query.result_columns().get_column_at(0).unwrap().col_type(),
            &Some(PgBuiltInType::int4())
        );
        assert_eq!(
            query.result_columns().get_column_at(1).unwrap().col_type(),
            &Some(PgBuiltInType::int8())
        );
    }
}

impl<'a> SourceNamespace<'a> {
    pub(crate) fn contains_alias(&self, name: &AliasName) -> bool {
        self.bindings
            .iter()
            .any(|binding| binding.alias.name() == name)
    }

    pub(crate) fn resolve_name(&self, name: &AliasName) -> Result<Option<QualifiedSource<'a>>, ()> {
        let mut result = None;
        for binding in &self.bindings {
            if binding.alias.name() == name {
                if result.is_some() {
                    return Err(());
                }
                result = Some(binding.source.clone());
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod test_local_source_ambiguity_is_retained {

    use crate::test_helpers::{
        query::source_namespace_statement, query_source_namespace as test_helpers,
    };

    #[rstest::rstest]
    #[case::common_column("SELECT t.id FROM a.t, b.t")]
    #[case::unique_column("SELECT t.only_b FROM a.t, b.t")]
    #[case::unique_other_order("SELECT t.only_b FROM b.t, a.t")]
    #[case::join("SELECT t.id FROM a.t CROSS JOIN b.t")]
    #[case::star("SELECT t.* FROM a.t, b.t")]
    #[case::local_blocks_outer("SELECT (SELECT t.id FROM a.t, b.t) FROM a.t AS t")]
    fn local_source_ambiguity_is_retained(#[case] sql: &str) {
        let analyzed = source_namespace_statement(sql);
        let problems = analyzed.context().reported_problems();
        assert!(
            problems
                .iter()
                .any(|problem| problem.code() == "column_source_ambiguous"),
            "{problems:?}"
        );
        assert!(
            !problems
                .iter()
                .any(|problem| problem.code() == "duplicate_alias")
        );
        let mut query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let mut failures = test_helpers::SourceFailures::default();
        query.walk(&mut failures).unwrap();
        failures.assert_source_ambiguity();
        assert!(
            query
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .col_type()
                .is_none()
        );
    }
}

impl<'a> SourceNamespace<'a> {
    pub(crate) fn resolve_column(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
        column: &ColumnName,
    ) -> Result<Result<LookupResult<QualifiedSource<'a>>, ()>, AnalysisError> {
        let mut result = None;
        let mut unknown = false;
        for binding in &self.bindings {
            if !binding.unqualified {
                continue;
            }
            let candidates = binding.source.column_candidates(context, with, column)?;
            match candidates.resolve() {
                Err(_) => return Ok(Err(())),
                Ok(LookupResult::Found(_)) => {
                    if result.is_some() {
                        return Ok(Err(()));
                    }
                    result = Some(binding.source.clone());
                }
                Ok(LookupResult::Unknown) => unknown = true,
                Ok(LookupResult::NotFound) => {}
            }
        }
        if unknown {
            Ok(Ok(LookupResult::unknown()))
        } else if let Some(source) = result {
            Ok(Ok(LookupResult::found(source)))
        } else {
            Ok(Ok(LookupResult::not_found()))
        }
    }
}

#[cfg(test)]
mod test_competing_derived_aliases_are_ambiguous {
    use crate::{
        ast::Dialect,
        sem::{ColumnReferenceExpr, SemScalarExpr},
        test_helpers::query::resolve,
    };

    #[test]
    fn competing_derived_aliases_are_ambiguous() {
        let (query, context) = resolve(
            "SELECT renamed FROM (SELECT 1 AS n) x(renamed), (SELECT 2 AS n) y(renamed)",
            Dialect::postgres(),
            false,
        )
        .unwrap();
        assert_eq!(context.reported_problem_count(), 1);
        assert_eq!(context.reported_problems()[0].code(), "ambiguous_column");
        assert_matches::assert_matches!(
            query
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .expression(),
            Some(SemScalarExpr::ColumnRef(
                ColumnReferenceExpr::AmbiguousColumnRef(_)
            ))
        );
    }
}

#[cfg(test)]
mod test_unique_column_retains_its_occurrence {

    use crate::{
        names::FullName,
        sem::{ColumnReferenceExpr, PgBuiltInType, SemScalarExpr, TypeReference},
        test_helpers::{query::source_namespace_statement, query_source_namespace as test_helpers},
    };

    #[rstest::rstest]
    #[case::second("SELECT only_b FROM a.t, b.t", "b", "t", PgBuiltInType::int8())]
    #[case::first("SELECT only_b FROM b.t, a.t", "b", "t", PgBuiltInType::int8())]
    #[case::join(
        "SELECT only_b FROM a.t CROSS JOIN b.t",
        "b",
        "t",
        PgBuiltInType::int8()
    )]
    #[case::quoted(
        r#"SELECT "b.b"."t.x".id FROM "a.b"."t.x", "b.b"."t.x""#,
        "b.b",
        "t.x",
        PgBuiltInType::int8()
    )]
    #[case::outer(
        "SELECT (SELECT only_b FROM a.u) FROM a.t, b.t",
        "b",
        "t",
        PgBuiltInType::int8()
    )]
    #[case::join_alias(
        "SELECT j.only_b FROM (a.t CROSS JOIN b.t) AS j",
        "b",
        "t",
        PgBuiltInType::int8()
    )]
    #[case::join_alias_bare(
        "SELECT only_b FROM (a.t CROSS JOIN b.t) AS j",
        "b",
        "t",
        PgBuiltInType::int8()
    )]
    #[case::join_alias_renamed(
        "SELECT j.z FROM (a.t CROSS JOIN b.t) AS j(x,y,z,w)",
        "b",
        "t",
        PgBuiltInType::int8()
    )]
    #[case::hidden_namespace(
        "SELECT b.t.id FROM (a.t CROSS JOIN a.u) AS j, b.t",
        "b",
        "t",
        PgBuiltInType::int8()
    )]
    #[case::outer_qualifier(
        "SELECT (SELECT t.id FROM a.u) FROM b.t",
        "b",
        "t",
        PgBuiltInType::int8()
    )]
    fn unique_column_retains_its_occurrence(
        #[case] sql: &str,
        #[case] schema: &str,
        #[case] table: &str,
        #[case] ty: TypeReference,
    ) {
        let analyzed = source_namespace_statement(sql);
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let mut query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let mut bindings = test_helpers::TableBindings::default();
        query.walk(&mut bindings).unwrap();
        bindings.assert_only_relation(&FullName::with_schema(schema, table));
        assert_eq!(
            query.result_columns().get_column_at(0).unwrap().col_type(),
            &Some(ty)
        );
    }

    #[rstest::rstest]
    #[case::comma("SELECT id FROM a.t, b.t")]
    #[case::join("SELECT id FROM a.t CROSS JOIN b.t")]
    #[case::joined_alias("SELECT j.id FROM (a.t CROSS JOIN b.t) AS j")]
    fn bare_column_ambiguity_is_retained(#[case] sql: &str) {
        let analyzed = source_namespace_statement(sql);
        assert!(
            analyzed
                .context()
                .reported_problems()
                .iter()
                .any(|problem| problem.code() == "ambiguous_column")
        );
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let column = query.result_columns().get_column_at(0).unwrap();
        assert!(matches!(
            column.expression(),
            Some(SemScalarExpr::ColumnRef(
                ColumnReferenceExpr::AmbiguousColumnRef(_)
            ))
        ));
        assert!(column.col_type().is_none());
    }
}

impl DataSource {
    pub(super) fn unaliased_relation(&self, alias: &AliasSpec) -> Option<FullName> {
        if alias.is_explicit() {
            return None;
        }
        match self {
            Self::Table(table) => Some(table.name().full_name().clone()),
            Self::View(view) => Some(view.view_name().full_name().clone()),
            Self::RangeTableSample(sample) => sample.source().unaliased_relation(alias),
            _ => None,
        }
    }

    pub(crate) fn namespace(&self) -> SourceNamespace<'_> {
        match self {
            Self::RangeTableSample(sample) => sample.source().namespace(),
            Self::JoinExpr(join) => join.namespace(self),
            Self::Group(group) => group.namespace(self),
            Self::MergeRecord(record) => record
                .source()
                .namespace()
                .with_merge(record.role(), record.is_nullable()),
            _ => {
                let mut namespace = SourceNamespace::default();
                for alias in self.get_aliases() {
                    namespace.push(self, &alias);
                }
                namespace
            }
        }
    }
}
