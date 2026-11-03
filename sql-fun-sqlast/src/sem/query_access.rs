//! Optional access-purpose extraction from completed semantic bindings.
use crate::{
    names::{ColumnName, FullName, TableName, ViewName},
    sem::{
        AccessKind, AliasSpec, AnalysisError, ParseContext, SemAst, SemScalarExpr,
        traversal::SemanticVisitor,
    },
};
use std::collections::{BTreeMap, BTreeSet};

/// SQL-visible object supplying an access, without inspecting its implementation.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AccessObjectKind {
    /// An ordinary table, including its system attributes.
    Table,
    /// A view, stopping at its declared outputs.
    View,
    /// A table-function invocation's output.
    Function,
    /// A materialized view, stopping at its declared outputs.
    MaterializedView,
}
impl AccessObjectKind {
    pub(crate) const fn table() -> Self {
        Self::Table
    }
    pub(crate) const fn view() -> Self {
        Self::View
    }
    pub(crate) const fn materialized_view() -> Self {
        Self::MaterializedView
    }
    const fn function() -> Self {
        Self::Function
    }
    /// Whether this object is a table.
    pub const fn is_table(self) -> bool {
        matches!(self, Self::Table)
    }
    /// Whether this object is a materialized view.
    pub const fn is_materialized_view(self) -> bool {
        matches!(self, Self::MaterializedView)
    }
    /// Whether this object is a view.
    pub const fn is_view(self) -> bool {
        matches!(self, Self::View)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct AccessColumn {
    kind: AccessObjectKind,
    relation: FullName,
    column: ColumnName,
}
impl AccessColumn {
    pub(crate) fn new(kind: AccessObjectKind, relation: &FullName, column: &ColumnName) -> Self {
        Self {
            kind,
            relation: relation.clone(),
            column: column.clone(),
        }
    }
}

// Query-local output edges are processing state, never schema identities.
// A visited-node set bounds recursive CTEs without losing contributing columns.
#[derive(Debug, Clone, Eq, PartialEq)]
enum AccessInput {
    Column(AccessColumn),
    Output(usize),
}
impl AccessInput {
    const fn column(column: AccessColumn) -> Self {
        Self::Column(column)
    }
    const fn output(index: usize) -> Self {
        Self::Output(index)
    }
    fn define(&self, graph: &mut AccessGraph, inputs: &[Self]) {
        if let Self::Output(index) = self {
            graph.0[*index] = inputs.to_vec();
        }
    }
    fn expand(
        &self,
        graph: &AccessGraph,
        visited: &mut BTreeSet<usize>,
        columns: &mut Vec<AccessColumn>,
    ) {
        match self {
            Self::Column(column) => {
                if !columns.contains(column) {
                    columns.push(column.clone());
                }
            }
            Self::Output(index) if visited.insert(*index) => {
                for input in &graph.0[*index] {
                    input.expand(graph, visited, columns);
                }
            }
            Self::Output(_) => {}
        }
    }
}
#[derive(Default)]
struct AccessGraph(Vec<Vec<AccessInput>>);
impl AccessGraph {
    fn output(&mut self) -> AccessInput {
        let input = AccessInput::output(self.0.len());
        self.0.push(Vec::new());
        input
    }
    fn columns(&self, inputs: &[AccessInput]) -> Vec<AccessColumn> {
        let mut columns = Vec::new();
        let mut visited = BTreeSet::new();
        for input in inputs {
            input.expand(self, &mut visited, &mut columns);
        }
        columns
    }
}

#[cfg(test)]
mod test_access_graph {
    #[test]
    fn cyclic_outputs_preserve_all_reachable_columns() {
        use {
            super::{AccessColumn, AccessGraph, AccessInput, AccessObjectKind},
            crate::names::{ColumnName, FullName},
        };
        let mut graph = AccessGraph::default();
        let x = graph.output();
        let y = graph.output();
        let first = AccessColumn::new(
            AccessObjectKind::table(),
            &FullName::with_schema("public", "a"),
            &ColumnName::from("id"),
        );
        let second = AccessColumn::new(
            AccessObjectKind::table(),
            &FullName::with_schema("public", "a"),
            &ColumnName::from("value"),
        );
        x.define(&mut graph, &[AccessInput::column(first.clone()), y.clone()]);
        y.define(
            &mut graph,
            &[AccessInput::column(second.clone()), x.clone()],
        );
        assert_eq!(graph.columns(&[x]), vec![first, second]);
    }
}

/// A resolved SQL-visible access for one top-level purpose.
#[derive(Debug, Clone, Eq, PartialEq, derive_getters::Getters)]
pub struct AccessRecord {
    access_kind: AccessKind,
    relation_kind: AccessObjectKind,
    relation: FullName,
    column_name: Option<ColumnName>,
}

/// Accesses in deterministic semantic encounter order, with identical records removed.
#[derive(Default)]
pub struct QueryAccess(Vec<AccessRecord>);
impl QueryAccess {
    /// Collect accesses without resolving names, solving types or examining Rust eligibility.
    /// The shared context supplies catalog identities already retained by the semantic tree.
    pub fn from_statement(
        context: &impl ParseContext,
        statement: &SemAst,
    ) -> Result<Self, AnalysisError> {
        let mut collector = AccessCollector::new(context);
        if statement.result_columns().is_some() {
            statement.visit_nested_command(&mut collector)?;
        }
        Ok(collector.finish())
    }
    /// Resolved relation and column records; relation-only actions have no column.
    pub fn records(&self) -> &[AccessRecord] {
        &self.0
    }
    fn record(
        &mut self,
        kind: AccessKind,
        object: AccessObjectKind,
        relation: &FullName,
        column: Option<&ColumnName>,
    ) {
        let record = AccessRecord {
            access_kind: kind,
            relation_kind: object,
            relation: relation.clone(),
            column_name: column.cloned(),
        };
        if !self.0.contains(&record) {
            self.0.push(record);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AccessKind;
    use crate::test_helpers::query_access;

    #[rstest::rstest]
    #[case::select(
        "SELECT a.id FROM a WHERE value = 1",
        "a",
        "id",
        AccessKind::select_output()
    )]
    #[case::filter(
        "SELECT a.id FROM a WHERE value = 1",
        "a",
        "value",
        AccessKind::select_filter()
    )]
    #[case::update_target(
        "UPDATE a SET value = id WHERE id = 1 RETURNING value",
        "a",
        "value",
        AccessKind::update_target()
    )]
    #[case::update_input(
        "UPDATE a SET value = id WHERE id = 1 RETURNING value",
        "a",
        "id",
        AccessKind::update_input()
    )]
    #[case::update_filter(
        "UPDATE a SET value = id WHERE id = 1 RETURNING value",
        "a",
        "id",
        AccessKind::update_filter()
    )]
    #[case::update_returning(
        "UPDATE a SET value = id WHERE id = 1 RETURNING value",
        "a",
        "value",
        AccessKind::update_returning()
    )]
    #[case::insert(
        "INSERT INTO a(id) SELECT id FROM b RETURNING id",
        "a",
        "id",
        AccessKind::insert_target()
    )]
    #[case::insert_input(
        "INSERT INTO a(id) SELECT id FROM b RETURNING id",
        "b",
        "id",
        AccessKind::insert_input()
    )]
    #[case::delete_filter(
        "DELETE FROM a WHERE id = 1 RETURNING value",
        "a",
        "id",
        AccessKind::delete_filter()
    )]
    #[case::delete_returning(
        "DELETE FROM a WHERE id = 1 RETURNING value",
        "a",
        "value",
        AccessKind::delete_returning()
    )]
    #[case::nested_output(
        "SELECT (SELECT b.value FROM b WHERE b.id = a.id) FROM a",
        "b",
        "id",
        AccessKind::select_output()
    )]
    #[case::correlated(
        "SELECT (SELECT b.value FROM b WHERE b.id = a.id) FROM a",
        "a",
        "id",
        AccessKind::select_output()
    )]
    #[case::nested_filter(
        "SELECT id FROM a WHERE id = (SELECT value FROM b WHERE b.id = 1)",
        "b",
        "id",
        AccessKind::select_filter()
    )]
    #[case::nested_update(
        "UPDATE a SET value = (SELECT value FROM b WHERE b.id = a.id)",
        "b",
        "id",
        AccessKind::update_input()
    )]
    #[case::cte(
        "WITH c AS (SELECT id, value FROM b) SELECT c.id FROM c WHERE c.value = 1",
        "b",
        "value",
        AccessKind::select_filter()
    )]
    #[case::derived(
        "SELECT x.id FROM (SELECT id FROM b) x",
        "b",
        "id",
        AccessKind::select_output()
    )]
    #[case::order(
        "SELECT value AS v FROM a ORDER BY v",
        "a",
        "value",
        AccessKind::select_ordering()
    )]
    #[case::join(
        "SELECT a.id FROM a JOIN b ON a.id = b.id",
        "b",
        "id",
        AccessKind::select_join()
    )]
    #[case::using(
        "SELECT a.value FROM a JOIN b USING(id)",
        "b",
        "id",
        AccessKind::select_join()
    )]
    #[case::star("SELECT * FROM a", "a", "value", AccessKind::select_output())]
    #[case::natural(
        "SELECT a.value FROM a NATURAL JOIN b",
        "b",
        "id",
        AccessKind::select_join()
    )]
    #[case::whole_row(
        "SELECT (a.*) IS NULL FROM a",
        "a",
        "value",
        AccessKind::select_output()
    )]
    #[case::ordinal(
        "SELECT value FROM a ORDER BY 1",
        "a",
        "value",
        AccessKind::select_ordering()
    )]
    #[case::group(
        "SELECT id FROM a GROUP BY id",
        "a",
        "id",
        AccessKind::select_grouping()
    )]
    #[case::having(
        "SELECT id FROM a GROUP BY id HAVING id = 1",
        "a",
        "id",
        AccessKind::select_group_filter()
    )]
    #[case::distinct(
        "SELECT DISTINCT ON(value) id FROM a",
        "a",
        "value",
        AccessKind::select_distinct()
    )]
    #[case::window(
        "SELECT id FROM a WINDOW w AS (PARTITION BY value)",
        "a",
        "value",
        AccessKind::select_window()
    )]
    #[case::implicit_insert(
        "INSERT INTO a VALUES (1, 2)",
        "a",
        "value",
        AccessKind::insert_target()
    )]
    #[case::returning_nested(
        "DELETE FROM a RETURNING (SELECT b.value FROM b WHERE b.id = a.id)",
        "b",
        "id",
        AccessKind::delete_returning()
    )]
    #[case::union(
        "SELECT x.id FROM (SELECT id FROM a UNION ALL SELECT value FROM b) x",
        "b",
        "value",
        AccessKind::select_output()
    )]
    #[case::union_left(
        "SELECT x.id FROM (SELECT id FROM a UNION ALL SELECT value FROM b) x WHERE x.id = 1",
        "a",
        "id",
        AccessKind::select_filter()
    )]
    #[case::merge_target(
        "MERGE INTO a USING b ON a.id = b.id WHEN MATCHED THEN UPDATE SET value = b.value",
        "a",
        "value",
        AccessKind::update_target()
    )]
    #[case::merge_input(
        "MERGE INTO a USING b ON a.id = b.id WHEN MATCHED THEN UPDATE SET value = b.value",
        "b",
        "value",
        AccessKind::update_input()
    )]
    #[case::merge_derived_input(
        "MERGE INTO a USING (SELECT id, value FROM b) s ON a.id = s.id WHEN MATCHED THEN UPDATE SET value = s.value",
        "b",
        "value",
        AccessKind::update_input()
    )]
    #[case::values_later_output(
        "WITH c(x) AS (VALUES ((SELECT id FROM a)), ((SELECT value FROM b))) SELECT x FROM c WHERE x = 1",
        "b",
        "value",
        AccessKind::select_output()
    )]
    #[case::values_later_filter(
        "WITH c(x) AS (VALUES ((SELECT id FROM a)), ((SELECT value FROM b))) SELECT x FROM c WHERE x = 1",
        "b",
        "value",
        AccessKind::select_filter()
    )]
    #[case::plain_distinct("SELECT DISTINCT id FROM a", "a", "id", AccessKind::select_distinct())]
    #[case::self_join(
        "SELECT x.value FROM a x JOIN a y ON x.id = y.id",
        "a",
        "value",
        AccessKind::select_output()
    )]
    #[case::nested_union(
        "SELECT x.id FROM (SELECT id FROM a UNION ALL (SELECT value FROM b UNION ALL SELECT id FROM b)) x WHERE x.id = 1",
        "b",
        "value",
        AccessKind::select_filter()
    )]
    #[case::default_insert(
        "INSERT INTO a DEFAULT VALUES",
        "a",
        "value",
        AccessKind::insert_target()
    )]
    #[case::nested_delete(
        "WITH gone AS (DELETE FROM a WHERE id = 1 RETURNING value) SELECT value FROM gone",
        "a",
        "id",
        AccessKind::select_source()
    )]
    #[case::delete_cte_output(
        "WITH gone AS (DELETE FROM a WHERE id = 1 RETURNING value) SELECT value FROM gone WHERE value = 1",
        "a",
        "value",
        AccessKind::select_filter()
    )]
    #[case::join_alias(
        "SELECT j.x FROM ((SELECT id FROM a) z JOIN (SELECT value FROM b) y ON z.id = y.value) j(x,v)",
        "a",
        "id",
        AccessKind::select_output()
    )]
    #[case::source(
        "SELECT 1 FROM (SELECT id FROM a) q",
        "a",
        "id",
        AccessKind::select_source()
    )]
    #[case::update_source(
        "UPDATE a SET value = 1 FROM (SELECT id FROM b) q",
        "b",
        "id",
        AccessKind::update_source()
    )]
    #[case::delete_source(
        "DELETE FROM a USING (SELECT id FROM b) q",
        "b",
        "id",
        AccessKind::delete_source()
    )]
    #[case::update_join(
        "UPDATE a SET value = 1 FROM b JOIN a x ON b.id = x.id",
        "b",
        "id",
        AccessKind::update_join()
    )]
    #[case::delete_join(
        "DELETE FROM a USING b JOIN a x ON b.id = x.id",
        "b",
        "id",
        AccessKind::delete_join()
    )]
    #[case::insert_returning(
        "INSERT INTO a(id) VALUES(1) RETURNING id",
        "a",
        "id",
        AccessKind::insert_returning()
    )]
    #[case::merge_join(
        "MERGE INTO a USING b ON a.id = b.id WHEN MATCHED THEN DELETE",
        "b",
        "id",
        AccessKind::merge_join()
    )]
    #[case::merge_condition(
        "MERGE INTO a USING b ON a.id = b.id WHEN MATCHED AND b.value = 1 THEN DELETE",
        "b",
        "value",
        AccessKind::merge_condition()
    )]
    #[case::merge_returning(
        "MERGE INTO a USING b ON a.id = b.id WHEN MATCHED THEN DELETE RETURNING a.id",
        "a",
        "id",
        AccessKind::merge_returning()
    )]
    fn records_top_level_purposes(
        #[case] sql: &str,
        #[case] relation: &str,
        #[case] column: &str,
        #[case] kind: AccessKind,
    ) {
        let accesses = query_access::analyze(sql);
        assert!(
            accesses
                .records()
                .iter()
                .any(|access| access.relation().local_name() == relation
                    && access
                        .column_name()
                        .as_ref()
                        .is_some_and(|name| name.as_str() == column)
                    && *access.access_kind() == kind),
            "{sql}: {:?}",
            accesses.records()
        );
    }

    #[rstest::rstest]
    #[case::subquery("SELECT (SELECT id FROM b) FROM a")]
    #[case::array_subquery("SELECT ARRAY(SELECT id FROM b) FROM a")]
    #[case::values(
        "WITH c(x) AS (VALUES ((SELECT id FROM a)), ((SELECT value FROM b))) SELECT DISTINCT x FROM c"
    )]
    #[case::update("UPDATE a SET (id, value) = (SELECT id, value FROM b) RETURNING id")]
    fn collection_preserves_result_bindings(#[case] sql: &str) {
        let accesses = query_access::analyze_preserving_output(sql);
        assert!(!accesses.records().is_empty());
    }

    #[test]
    fn values_observation_only_publishes_bound_output_positions() {
        let (accesses, _) = query_access::analyze_diagnosed("VALUES (1), (2, 3)");
        assert!(accesses.records().is_empty());
    }

    #[test]
    fn sql_visible_objects_stop_at_view_and_function_outputs() {
        let accesses = query_access::analyze_in(
            "SELECT v.id, f.f, m.id FROM v CROSS JOIN f() CROSS JOIN m",
            query_access::object_context(),
            crate::ast::Dialect::postgres(),
        );
        assert!(
            accesses
                .records()
                .iter()
                .all(|access| access.relation().local_name() != "a")
        );
        for (name, column, kind) in [
            ("v", "id", super::AccessObjectKind::view()),
            ("m", "id", super::AccessObjectKind::materialized_view()),
            ("f", "f", super::AccessObjectKind::function()),
        ] {
            assert!(
                accesses
                    .records()
                    .iter()
                    .any(|access| access.relation().local_name() == name
                        && access
                            .column_name()
                            .as_ref()
                            .is_some_and(|value| value.as_str() == column)
                        && *access.relation_kind() == kind),
                "{:?}",
                accesses.records()
            );
        }
    }
    #[test]
    fn rows_from_aliases_preserve_each_function_output() {
        let accesses = query_access::analyze_in(
            "SELECT r.x FROM ROWS FROM (f()) AS r(x)",
            query_access::object_context(),
            crate::ast::Dialect::postgres(),
        );
        assert!(
            accesses
                .records()
                .iter()
                .any(|access| access.relation().local_name() == "f"
                    && access
                        .column_name()
                        .as_ref()
                        .is_some_and(|column| column.as_str() == "r")
                    && *access.access_kind() == AccessKind::select_output()),
            "{:?}",
            accesses.records()
        );
    }
    #[test]
    fn insertion_does_not_hide_a_nested_read_of_the_same_table() {
        let accesses = query_access::analyze("INSERT INTO a(id) SELECT 1 FROM a");
        for role in [AccessKind::insert_rows(), AccessKind::read_rows()] {
            assert!(
                accesses
                    .records()
                    .iter()
                    .any(|access| *access.access_kind() == role && access.column_name().is_none()),
                "{:?}",
                accesses.records()
            );
        }
    }
    #[test]
    fn unresolved_recursive_names_keep_their_existing_diagnostic() {
        let (accesses, problems) = query_access::analyze_diagnosed(
            "WITH RECURSIVE c(id) AS (SELECT id FROM a UNION ALL SELECT id FROM c) SELECT id FROM c",
        );
        assert!(
            problems.iter().any(|code| code == "relation_not_found"),
            "{problems:?}"
        );
        assert!(
            accesses
                .records()
                .iter()
                .all(|access| access.relation().local_name() == "a")
        );
        assert!(accesses.records().iter().any(|access| {
            access
                .column_name()
                .as_ref()
                .is_some_and(|name| name.as_str() == "id")
        }));
    }
    #[test]
    fn diagnosed_conflict_selection_preserves_bound_column_purpose() {
        let (accesses, _) = query_access::analyze_diagnosed(
            "INSERT INTO a(id) VALUES(1) ON CONFLICT(id) DO NOTHING",
        );
        assert!(
            accesses.records().iter().any(|access| *access.access_kind()
                == AccessKind::insert_conflict()
                && access
                    .column_name()
                    .as_ref()
                    .is_some_and(|column| column.as_str() == "id")),
            "{:?}",
            accesses.records()
        );
    }
    #[rstest::rstest]
    #[case::left("LEFT", &["a"])]
    #[case::right("RIGHT", &["b"])]
    #[case::full("FULL", &["a", "b"])]
    fn joined_output_uses_the_shared_projection_contract(
        #[case] kind: &str,
        #[case] relations: &[&str],
    ) {
        let accesses = query_access::analyze(&format!(
            "SELECT j.id FROM ((SELECT id FROM a) x {kind} JOIN (SELECT id FROM b) y USING(id)) j"
        ));
        let actual = accesses
            .records()
            .iter()
            .filter(|access| *access.access_kind() == AccessKind::select_output())
            .map(|access| access.relation().local_name().to_string())
            .collect::<Vec<_>>();
        assert_eq!(actual, relations);
        for relation in ["a", "b"] {
            assert!(
                accesses
                    .records()
                    .iter()
                    .any(|access| access.relation().local_name() == relation
                        && *access.access_kind() == AccessKind::select_join())
            );
        }
    }
    #[test]
    fn identical_records_are_deduplicated_but_roles_are_preserved() {
        let accesses = query_access::analyze("SELECT id, id FROM a WHERE id = 1 ORDER BY id");
        assert_eq!(accesses.records().len(), 4, "{:?}", accesses.records());
        let roles = accesses
            .records()
            .iter()
            .map(|value| *value.access_kind())
            .collect::<Vec<_>>();
        for role in [
            AccessKind::read_rows(),
            AccessKind::select_output(),
            AccessKind::select_filter(),
            AccessKind::select_ordering(),
        ] {
            assert!(roles.contains(&role));
        }
    }
    #[test]
    fn relation_only_read_does_not_invent_a_column() {
        let accesses = query_access::analyze("SELECT 1 FROM a");
        assert_eq!(accesses.records().len(), 1);
        assert_eq!(
            *accesses.records()[0].access_kind(),
            AccessKind::read_rows()
        );
        assert!(accesses.records()[0].column_name().is_none());
    }
    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_uses_existing_semantic_bindings() {
        let context = query_access::context().with_dialect(crate::ast::Dialect::sqlite());
        let accesses =
            query_access::analyze_in("SELECT id FROM a", context, crate::ast::Dialect::sqlite());
        assert!(accesses.records().iter().any(|value| {
            value.relation().local_name() == "a"
                && value
                    .column_name()
                    .as_ref()
                    .is_some_and(|name| name.as_str() == "id")
                && *value.access_kind() == AccessKind::select_output()
        }));
    }
    #[test]
    fn nested_where_does_not_replace_output_purpose() {
        let accesses =
            query_access::analyze("SELECT (SELECT b.value FROM b WHERE b.id = a.id) FROM a");
        for access in accesses.records() {
            if access.column_name().is_some() {
                assert_eq!(*access.access_kind(), AccessKind::select_output());
            }
        }
    }
    #[test]
    fn delete_action_does_not_invent_column_writes() {
        let accesses = query_access::analyze("DELETE FROM a");
        assert_eq!(accesses.records().len(), 1);
        assert_eq!(
            *accesses.records()[0].access_kind(),
            AccessKind::delete_rows()
        );
        assert!(accesses.records()[0].column_name().is_none());
    }
}

#[derive(Default, Clone)]
struct Outputs(Vec<(ColumnName, Vec<AccessInput>)>);
impl Outputs {
    fn aliased(&self, alias: &AliasSpec) -> Self {
        let names = alias.visible_column_names(
            &self
                .0
                .iter()
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>(),
        );
        Self(
            names
                .into_iter()
                .zip(&self.0)
                .map(|(name, (_, columns))| (name, columns.clone()))
                .collect(),
        )
    }
    fn named(&self, name: &ColumnName) -> Vec<AccessInput> {
        self.0
            .iter()
            .filter(|(column, _)| column == name)
            .flat_map(|(_, columns)| columns.clone())
            .collect()
    }
}
#[derive(Default)]
struct Scope {
    sources: BTreeMap<String, Outputs>,
    source_order: Vec<Outputs>,
    ctes: BTreeMap<String, Outputs>,
    outputs: Outputs,
    target: Option<FullName>,
    target_bound: bool,
}
impl Scope {
    fn bind(&mut self, alias: &AliasSpec, outputs: Outputs) {
        let name = alias.name().as_str().to_owned();
        self.source_order.push(outputs.clone());
        self.sources.insert(name, outputs);
    }
    fn source_columns(&self, start: usize, end: usize) -> Vec<(ColumnName, Vec<AccessInput>)> {
        self.source_order[start..end]
            .iter()
            .flat_map(|outputs| outputs.0.clone())
            .collect()
    }
}
struct AccessCollector<'a, C> {
    context: &'a C,
    records: QueryAccess,
    graph: AccessGraph,
    requests: Vec<(AccessKind, Vec<AccessInput>)>,
    purpose: AccessKind,
    scopes: Vec<Scope>,
    finished: Outputs,
    expressions: Vec<Vec<AccessInput>>,
    last_expression: Vec<AccessInput>,
    joins: Vec<(usize, usize)>,
    join_purposes: Vec<AccessKind>,
    set_operands: Vec<Vec<Outputs>>,
    rows_functions: Vec<(Outputs, usize)>,
}
impl<'a, C: ParseContext> AccessCollector<'a, C> {
    fn new(context: &'a C) -> Self {
        Self {
            context,
            records: QueryAccess::default(),
            graph: AccessGraph::default(),
            requests: Vec::new(),
            purpose: AccessKind::select_source(),
            scopes: Vec::new(),
            finished: Outputs::default(),
            expressions: Vec::new(),
            last_expression: Vec::new(),
            joins: Vec::new(),
            join_purposes: Vec::new(),
            set_operands: Vec::new(),
            rows_functions: Vec::new(),
        }
    }
    fn scope(&mut self) -> &mut Scope {
        self.scopes.last_mut().expect("query scope entered")
    }
    fn object_kind(&self, name: &FullName) -> Option<AccessObjectKind> {
        if self
            .context
            .get_table(&TableName::from(name.clone()))
            .is_some()
        {
            Some(AccessObjectKind::table())
        } else {
            self.context
                .get_view(&ViewName::from_full_name(name))
                .map(|view| {
                    if view.is_materialized() {
                        AccessObjectKind::materialized_view()
                    } else {
                        AccessObjectKind::view()
                    }
                })
        }
    }

    fn columns(&self, expression: &SemScalarExpr) -> Vec<AccessInput> {
        if let Some(reference) = expression.access_reference() {
            if let Some(mut column) = reference.access_column() {
                if column.kind.is_view() {
                    column.kind = self.object_kind(&column.relation).unwrap_or(column.kind);
                }
                return vec![AccessInput::column(column)];
            }
            if let Some(index) = expression.output_index()
                && let Some(scope) = self.scopes.last()
                && let Some((_, columns)) = scope.outputs.0.get(index)
            {
                return columns.clone();
            }
            if let Some((alias, name)) = reference.projection_reference() {
                for scope in self.scopes.iter().rev() {
                    if let Some(outputs) = scope.sources.get(alias)
                        && outputs.0.iter().any(|(column, _)| column == name)
                    {
                        return outputs.named(name);
                    }
                }
            }
        }
        Vec::new()
    }
    fn record_columns(&mut self, columns: &[AccessInput]) {
        if !columns.is_empty() {
            self.requests.push((self.purpose, columns.to_vec()));
        }
    }
    fn finish(mut self) -> QueryAccess {
        for (purpose, inputs) in &self.requests {
            for column in self.graph.columns(inputs) {
                self.records.record(
                    *purpose,
                    column.kind,
                    &column.relation,
                    Some(&column.column),
                );
                if !purpose.is_column_target() {
                    self.records.record(
                        AccessKind::read_rows(),
                        column.kind,
                        &column.relation,
                        None,
                    );
                }
            }
        }
        self.records
    }
}
impl<C: ParseContext> SemanticVisitor for AccessCollector<'_, C> {
    fn enter_set_operation(&mut self) {
        self.set_operands.push(Vec::new());
    }
    fn leave_set_operation(&mut self) {
        self.set_operands.pop();
    }
    fn leave_set_operand(&mut self) {
        self.set_operands
            .last_mut()
            .expect("set operation entered")
            .push(self.finished.clone());
    }
    fn set_output(&mut self, index: usize, name: &str, _value: &SemScalarExpr) {
        let mut columns = Vec::new();
        for operand in self.set_operands.last().expect("set operation entered") {
            if let Some((_, inputs)) = operand.0.get(index) {
                for column in inputs {
                    if !columns.contains(column) {
                        columns.push(column.clone());
                    }
                }
            }
        }
        self.scope()
            .outputs
            .0
            .push((ColumnName::from(name), columns));
    }
    fn enter_query(&mut self) {
        self.scopes.push(Scope::default());
    }
    fn leave_query(&mut self) {
        self.finished = self.scopes.pop().expect("balanced query scope").outputs;
    }
    fn access_purpose(&mut self, kind: AccessKind) {
        if self.scopes.len() == 1 {
            self.purpose = kind;
        }
    }
    fn access_action(&mut self, name: &FullName, kind: AccessKind) {
        self.scope().target = Some(name.clone());
        if let Some(object) = self.object_kind(name) {
            self.records.record(kind, object, name, None);
        }
    }
    fn source_binding(&mut self, name: &FullName, alias: &AliasSpec) {
        let Some(kind) = self.object_kind(name) else {
            return;
        };
        let scope = self.scope();
        let target_binding = !scope.target_bound && scope.target.as_ref() == Some(name);
        scope.target_bound |= target_binding;
        if !target_binding {
            self.records
                .record(AccessKind::read_rows(), kind, name, None);
        }
        let names = if let Some(table) = self.context.get_table(&TableName::from(name.clone())) {
            table.column_names()
        } else if let Some(view) = self.context.get_view(&ViewName::from_full_name(name)) {
            view.result_columns().names()
        } else {
            Vec::new()
        };
        let outputs = Outputs(
            names
                .into_iter()
                .map(|column| {
                    (
                        column.clone(),
                        vec![AccessInput::column(AccessColumn::new(kind, name, &column))],
                    )
                })
                .collect(),
        )
        .aliased(alias);
        self.scope().bind(alias, outputs);
    }
    fn enter_rows_functions(&mut self) {
        let start = self.scope().source_order.len();
        self.rows_functions.push((Outputs::default(), start));
    }
    fn leave_rows_functions(&mut self, alias: &AliasSpec, ordinality: bool) {
        let (mut outputs, start) = self.rows_functions.pop().expect("ROWS FROM entered");
        self.scope().source_order.truncate(start);
        if ordinality {
            outputs.0.push((ColumnName::from("ordinality"), Vec::new()));
        }
        self.scope().bind(alias, outputs.aliased(alias));
    }
    fn function_binding(&mut self, name: &FullName, alias: &AliasSpec, columns: &[ColumnName]) {
        self.records.record(
            AccessKind::read_rows(),
            AccessObjectKind::function(),
            name,
            None,
        );
        let outputs = Outputs(
            columns
                .iter()
                .map(|column| {
                    (
                        column.clone(),
                        vec![AccessInput::column(AccessColumn::new(
                            AccessObjectKind::function(),
                            name,
                            column,
                        ))],
                    )
                })
                .collect(),
        )
        .aliased(alias);
        if let Some(group) = self.rows_functions.last_mut() {
            group.0.0.extend(outputs.0.clone());
        }
        self.scope().bind(alias, outputs);
    }
    fn leave_access_join(&mut self) {
        self.purpose = self.join_purposes.pop().expect("join purpose entered");
    }
    fn enter_join(&mut self) {
        let before = self.scope().source_order.len();
        self.joins.push((before, before));
    }
    fn join_right(&mut self) {
        let boundary = self.scope().source_order.len();
        self.joins.last_mut().expect("join entered").1 = boundary;
    }
    fn access_join(&mut self, join: &crate::sem::data_source::JoinExprDataSource) {
        self.join_purposes.push(self.purpose);
        if self.scopes.len() == 1 {
            self.purpose = self.purpose.join_purpose();
        }
        let (before, boundary) = self.joins.pop().expect("join entered");
        let scope = self.scope();
        let left = scope.source_columns(before, boundary);
        let right = scope.source_columns(boundary, scope.source_order.len());
        let layout = join.projection_layout(
            self.context.dialect(),
            &left
                .iter()
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>(),
            &right
                .iter()
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>(),
        );
        for (name, columns) in left.iter().chain(&right) {
            if layout.keys().contains(name) {
                self.record_columns(columns);
            }
        }
        let output = layout.project(&left, &right, |left_index, right_index| {
            let (name, columns) = &left[left_index];
            let mut columns = columns.clone();
            columns.extend(right[right_index].1.clone());
            (name.clone(), columns)
        });
        self.scope().source_order.truncate(before);
        if let Some(alias) = join.alias() {
            self.scope().bind(alias, Outputs(output).aliased(alias));
        } else {
            self.scope().source_order.push(Outputs(output));
        }
    }

    fn derived_binding(&mut self, alias: &AliasSpec, _columns: &[ColumnName]) {
        let outputs = self.finished.aliased(alias);
        self.scope().bind(alias, outputs);
    }
    fn enter_cte(&mut self, name: &str, columns: &[ColumnName]) {
        let outputs = Outputs(
            columns
                .iter()
                .map(|name| (name.clone(), vec![self.graph.output()]))
                .collect(),
        );
        self.scope().ctes.insert(name.to_owned(), outputs);
    }
    fn cte_columns(&mut self, name: &str, _columns: &[ColumnName]) {
        let outputs = self.scope().ctes[name].clone();
        for ((_, placeholders), (_, inputs)) in outputs.0.iter().zip(&self.finished.0) {
            for placeholder in placeholders {
                placeholder.define(&mut self.graph, inputs);
            }
        }
    }
    fn cte_source(&mut self, name: &str, alias: &AliasSpec) {
        let output = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.ctes.get(name))
            .cloned();
        if let Some(output) = output {
            self.scope().bind(alias, output.aliased(alias));
        }
    }
    fn enter_expression(&mut self, _expression: &SemScalarExpr) -> Result<(), AnalysisError> {
        self.expressions.push(Vec::new());
        Ok(())
    }
    fn expression(&mut self, expression: &SemScalarExpr) -> Result<(), AnalysisError> {
        let columns = self.columns(expression);
        self.record_columns(&columns);
        let mut dependencies = self.expressions.pop().expect("balanced expression");
        for column in columns {
            if !dependencies.contains(&column) {
                dependencies.push(column);
            }
        }
        if let Some(parent) = self.expressions.last_mut() {
            for column in &dependencies {
                if !parent.contains(column) {
                    parent.push(column.clone());
                }
            }
        }
        self.last_expression = dependencies;
        Ok(())
    }
    fn values_output(&mut self, index: usize, name: &str, _expression: &SemScalarExpr) {
        let inputs = self.last_expression.clone();
        if let Some((_, columns)) = self.scope().outputs.0.get_mut(index) {
            for input in inputs {
                if !columns.contains(&input) {
                    columns.push(input);
                }
            }
        } else {
            self.scope()
                .outputs
                .0
                .push((ColumnName::from(name), inputs));
        }
    }
    fn distinct_outputs(&mut self) {
        let columns = self
            .scope()
            .outputs
            .0
            .iter()
            .flat_map(|(_, inputs)| inputs.clone())
            .collect::<Vec<_>>();
        self.record_columns(&columns);
    }
    fn output(&mut self, name: &str, _expression: &SemScalarExpr) {
        let columns = self.last_expression.clone();
        self.scope()
            .outputs
            .0
            .push((ColumnName::from(name), columns));
    }
    fn assignment_target(&mut self, target: &SemScalarExpr) {
        let columns = self.columns(target);
        self.record_columns(&columns);
    }
}
