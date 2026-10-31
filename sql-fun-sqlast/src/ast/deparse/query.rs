use super::{DeparseError, SqlEmit, SqlWriter};
use crate::ast::{
    Alias, CommitAction, Cte, CteBody, CteColumn, Cycle, DerivedTable, Frame, FrameBound,
    FrameExclusion, FrameMode, FromGroup, FromItem, IndexExpression, Into, Join, JoinKind, Lock,
    LockStrength, LockWait, Name, Order, Query, QueryBody, Relation, Search, Select, SetOperation,
    SetQuery, SyntaxOption, SyntaxValue, TableFunction, TableFunctions, TableSample, Target,
    TypedColumn, Window, With,
};

impl SqlEmit for Query {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        crate::sem::with_stack(|| {
            self.with.emit(w)?;
            self.body.emit(w)?;
            if !self.order.is_empty() {
                w.sql(" ORDER BY ");
                w.list(&self.order, ", ")?;
            }
            self.emit_row_limit(w)?;
            for lock in &self.locks {
                lock.emit(w)?;
            }
            Ok(())
        })
    }
}

impl Query {
    fn emit_row_limit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.with_ties {
            return self.emit_fetch_with_ties(w);
        }
        if let Some(limit) = &self.limit {
            w.sql(" LIMIT ");
            limit.emit(w)?;
        } else if self.offset.is_some() && !w.is_postgres() {
            w.sql(" LIMIT -1");
        }
        if let Some(offset) = &self.offset {
            w.sql(" OFFSET ");
            offset.emit(w)?;
        }
        Ok(())
    }

    fn emit_fetch_with_ties(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.postgres_only("FETCH WITH TIES")?;
        if let Some(offset) = &self.offset {
            w.sql(" OFFSET ");
            offset.emit(w)?;
            w.sql(" ROWS");
        }
        w.sql(" FETCH FIRST ");
        let Some(limit) = &self.limit else {
            return w.invalid("FETCH", "WITH TIES requires a count");
        };
        limit.emit(w)?;
        w.sql(" ROWS WITH TIES");
        Ok(())
    }
}

#[cfg(test)]
mod test_query_emission {
    use crate::ast::Dialect;
    use crate::parse_statements;

    #[rstest::rstest]
    #[case::distinct_on("SELECT DISTINCT ON (a) a, b FROM t ORDER BY a, b DESC NULLS LAST")]
    #[case::set_precedence("SELECT 1 UNION SELECT 2 INTERSECT SELECT 3")]
    #[case::set_parentheses("(SELECT 1 UNION SELECT 2) INTERSECT SELECT 3")]
    #[case::set_term_limit("(SELECT 1 ORDER BY 1 LIMIT 1) UNION ALL SELECT 2")]
    #[case::values("VALUES (1, 'a'), (2, 'b')")]
    #[case::named_window("SELECT sum(a) OVER w FROM t WINDOW w AS (ORDER BY b ROWS CURRENT ROW)")]
    #[case::inherited_window("SELECT sum(a) OVER (w) FROM t WINDOW w AS (ORDER BY b)")]
    #[case::window_frame(
        "SELECT sum(a) OVER (PARTITION BY b ORDER BY c GROUPS BETWEEN 2 PRECEDING AND CURRENT ROW EXCLUDE TIES) FROM t"
    )]
    #[case::ordering_operator(
        "SELECT a FROM t ORDER BY a USING OPERATOR(pg_catalog.>) NULLS FIRST"
    )]
    #[case::nested_joins("SELECT * FROM a LEFT JOIN (b JOIN c ON b.id = c.id) ON a.id = b.id")]
    #[case::join_alias("SELECT * FROM (a JOIN b USING (id)) AS joined")]
    #[case::using_alias("SELECT * FROM a JOIN b USING (id) AS keys")]
    #[case::typed_table_function("SELECT * FROM f() AS (id integer, name text)")]
    #[case::named_typed_table_function("SELECT * FROM f() AS record(id integer, name text)")]
    #[case::rows_from("SELECT * FROM ROWS FROM (f() AS (id integer), g()) WITH ORDINALITY AS r")]
    #[case::lateral("SELECT * FROM a, LATERAL (SELECT a.id) AS b")]
    #[case::sample("SELECT * FROM a TABLESAMPLE system(10) REPEATABLE (2)")]
    #[case::grouping_sets(
        "SELECT a, b, GROUPING(a, b) FROM t GROUP BY GROUPING SETS ((a, b), (a), ())"
    )]
    #[case::rollup_cube("SELECT a, b FROM t GROUP BY ROLLUP(a), CUBE(b)")]
    #[case::materialized_cte("WITH q AS NOT MATERIALIZED (SELECT 1 AS a) SELECT a FROM q")]
    #[case::search_cycle(
        "WITH RECURSIVE q(a) AS (SELECT 1 UNION ALL SELECT a + 1 FROM q) SEARCH DEPTH FIRST BY a SET path CYCLE a SET cyclic TO true DEFAULT false USING cycle_path SELECT * FROM q"
    )]
    #[case::fetch_ties("SELECT a FROM t ORDER BY a OFFSET 1 ROWS FETCH FIRST 2 ROWS WITH TIES")]
    #[case::row_lock("SELECT * FROM t FOR NO KEY UPDATE OF t SKIP LOCKED")]
    fn preserves_postgres_query_structure(#[case] sql: &str) {
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let emitted = statement.deparse_sql(Dialect::postgres()).unwrap();
        let original = pg_query::parse(sql, ::pg_query::ParserOptions::default())
            .unwrap()
            .deparse(::pg_query::DeparseOptions::default())
            .unwrap();
        let reparsed = pg_query::parse(&emitted, ::pg_query::ParserOptions::default())
            .unwrap()
            .deparse(::pg_query::DeparseOptions::default())
            .unwrap();
        assert_eq!(original, reparsed, "emitted SQL: {emitted}");
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::mixed_compounds("SELECT 1 UNION SELECT 2 INTERSECT SELECT 3 EXCEPT SELECT 4")]
    #[case::compound_order("SELECT 1 UNION ALL SELECT 2 ORDER BY 1 DESC LIMIT 1 OFFSET 1")]
    #[case::named_window("SELECT sum(a) OVER w FROM t WINDOW w AS (ORDER BY b ROWS CURRENT ROW)")]
    #[case::inherited_window("SELECT sum(a) OVER (w) FROM t WINDOW w AS (ORDER BY b)")]
    #[case::frame_exclusion(
        "SELECT sum(a) OVER (ORDER BY b ROWS BETWEEN 2 PRECEDING AND 1 FOLLOWING EXCLUDE CURRENT ROW) FROM t"
    )]
    #[case::nested_joins("SELECT * FROM a LEFT JOIN (b JOIN c ON b.id = c.id) ON a.id = b.id")]
    #[case::grouped_join_alias("SELECT * FROM (a JOIN b USING (id)) AS joined")]
    #[case::index_hint("SELECT * FROM t AS x INDEXED BY idx")]
    #[case::no_index("SELECT * FROM t NOT INDEXED")]
    #[case::table_function("SELECT * FROM json_each('[1, 2]') AS j")]
    #[case::materialized_cte("WITH q AS MATERIALIZED (SELECT 1 AS a) SELECT a FROM q")]
    fn sqlite_query_rendering_is_stable(#[case] sql: &str) {
        let statement = parse_statements(sql, Dialect::sqlite())
            .next()
            .unwrap()
            .unwrap();
        let emitted = statement.deparse_sql(Dialect::sqlite()).unwrap();
        let reparsed = parse_statements(&emitted, Dialect::sqlite())
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(emitted, reparsed.deparse_sql(Dialect::sqlite()).unwrap());
    }
}
impl SqlEmit for QueryBody {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Select(v) => v.emit(w),
            Self::Set(v) => v.emit(w),
            Self::Values(rows) => Self::emit_values(rows, w),
        }
    }
}
impl QueryBody {
    fn emit_values(rows: &[Vec<crate::ast::Expr>], w: &mut SqlWriter) -> Result<(), DeparseError> {
        if rows.is_empty() {
            return w.invalid("VALUES", "VALUES requires a row");
        }
        w.sql("VALUES ");
        for (i, row) in rows.iter().enumerate() {
            if row.is_empty() {
                return w.invalid("VALUES", "VALUES rows cannot be empty");
            }
            if i > 0 {
                w.sql(", ");
            }
            w.sql("(");
            w.list(row, ", ")?;
            w.sql(")");
        }
        Ok(())
    }
}

impl SqlEmit for SetQuery {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if !w.is_postgres() {
            self.operation.validate_sqlite_all(self.all, w)?;
            self.left.validate_sqlite_compound_term(false, w)?;
            self.right.validate_sqlite_compound_term(true, w)?;
        }
        if w.is_postgres() {
            w.sql("(");
            self.left.emit(w)?;
            w.sql(")");
        } else {
            self.left.emit(w)?;
        }
        w.sql(" ");
        self.operation.emit(w)?;
        if self.all {
            w.sql(" ALL");
        }
        w.sql(" ");
        if w.is_postgres() {
            w.sql("(");
            self.right.emit(w)?;
            w.sql(")");
        } else {
            self.right.emit(w)?;
        }
        Ok(())
    }
}
impl SqlEmit for SetOperation {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql(match self {
            Self::Union => "UNION",
            Self::Intersect => "INTERSECT",
            Self::Except => "EXCEPT",
        });
        Ok(())
    }
}
impl SetOperation {
    fn validate_sqlite_all(&self, all: bool, writer: &SqlWriter) -> Result<(), DeparseError> {
        if all && !matches!(self, Self::Union) {
            writer.invalid("compound SELECT", "SQLite supports ALL only with UNION")
        } else {
            Ok(())
        }
    }
}
impl Query {
    fn validate_sqlite_compound_term(
        &self,
        right: bool,
        writer: &SqlWriter,
    ) -> Result<(), DeparseError> {
        if !self.with.entries.is_empty()
            || !self.order.is_empty()
            || self.limit.is_some()
            || self.offset.is_some()
            || self.with_ties
            || !self.locks.is_empty()
        {
            return writer.invalid("compound SELECT", "SQLite compound terms cannot contain independent WITH, ordering, limits, or locking clauses");
        }
        if right {
            self.body.validate_sqlite_right_term(writer)?;
        }
        Ok(())
    }
}
impl QueryBody {
    fn validate_sqlite_right_term(&self, writer: &SqlWriter) -> Result<(), DeparseError> {
        if matches!(self, Self::Set(_)) {
            writer.invalid(
                "compound SELECT",
                "SQLite compound SELECT operations associate left to right",
            )
        } else {
            Ok(())
        }
    }
}
impl SqlEmit for Select {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if !w.is_postgres() && self.targets.is_empty() {
            return w.invalid("SELECT", "SQLite SELECT requires a result expression");
        }
        w.sql("SELECT ");
        if let Some(distinct) = &self.distinct {
            w.sql("DISTINCT ");
            if !distinct.is_empty() {
                w.postgres_only("DISTINCT ON")?;
                w.sql("ON (");
                w.list(distinct, ", ")?;
                w.sql(") ");
            }
        }
        w.list(&self.targets, ", ")?;
        if let Some(into) = &self.into {
            w.postgres_only("SELECT INTO")?;
            w.sql(" INTO ");
            into.emit(w)?;
        }
        if !self.from.is_empty() {
            w.sql(" FROM ");
            w.list(&self.from, ", ")?;
        }
        if let Some(predicate) = &self.predicate {
            w.sql(" WHERE ");
            predicate.emit(w)?;
        }
        if !self.group.is_empty() {
            w.sql(" GROUP BY ");
            if self.group_distinct {
                w.postgres_only("GROUP BY DISTINCT")?;
                w.sql("DISTINCT ");
            }
            w.list(&self.group, ", ")?;
        }
        if let Some(having) = &self.having {
            w.sql(" HAVING ");
            having.emit(w)?;
        }
        if !self.windows.is_empty() {
            w.sql(" WINDOW ");
            for (i, window) in self.windows.iter().enumerate() {
                if i > 0 {
                    w.sql(", ");
                }
                let Some(name) = &window.name else {
                    return w.invalid("WINDOW", "definition requires a name");
                };
                w.ident(name)?;
                w.sql(" AS ");
                window.emit_definition(w)?;
            }
        }
        Ok(())
    }
}
impl SqlEmit for Target {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        self.expression.emit(w)?;
        if let Some(alias) = &self.alias {
            w.sql(" AS ");
            w.ident(alias)?;
        }
        Ok(())
    }
}
impl SqlEmit for Order {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        self.expression.emit(w)?;
        if let Some(using) = &self.using {
            w.postgres_only("ORDER USING")?;
            w.sql(" USING ");
            using.emit_operator(w)?;
        } else if let Some(desc) = self.descending {
            w.sql(if desc { " DESC" } else { " ASC" });
        }
        if let Some(first) = self.nulls_first {
            w.sql(if first { " NULLS FIRST" } else { " NULLS LAST" });
        }
        Ok(())
    }
}
impl SqlEmit for With {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.entries.is_empty() {
            return Ok(());
        }
        w.sql("WITH ");
        if self.recursive {
            w.sql("RECURSIVE ");
        }
        w.list(&self.entries, ", ")?;
        w.sql(" ");
        Ok(())
    }
}
impl SqlEmit for Cte {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.ident(&self.name)?;
        if !self.columns.is_empty() {
            w.sql("(");
            w.list(&self.columns, ", ")?;
            w.sql(")");
        }
        w.sql(" AS ");
        if let Some(materialized) = self.materialized {
            w.sql(if materialized {
                "MATERIALIZED "
            } else {
                "NOT MATERIALIZED "
            });
        }
        w.sql("(");
        self.body.emit(w)?;
        w.sql(")");
        if let Some(search) = &self.search {
            search.emit(w)?;
        }
        if let Some(cycle) = &self.cycle {
            cycle.emit(w)?;
        }
        Ok(())
    }
}
impl SqlEmit for CteColumn {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.collation.is_some() || self.descending.is_some() {
            return w.invalid(
                "CTE column",
                "CTE declarations accept column names, without collation or sorting",
            );
        }
        w.ident(&self.name)
    }
}
impl SqlEmit for CteBody {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Query(v) => v.emit(w),
            Self::Insert(v) => {
                w.postgres_only("data-modifying CTE")?;
                v.emit(w)
            }
            Self::Update(v) => {
                w.postgres_only("data-modifying CTE")?;
                v.emit(w)
            }
            Self::Delete(v) => {
                w.postgres_only("data-modifying CTE")?;
                v.emit(w)
            }
            Self::Merge(v) => {
                w.postgres_only("MERGE CTE")?;
                v.emit(w)
            }
        }
    }
}
impl SqlEmit for Search {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.postgres_only("SEARCH")?;
        w.sql(if self.breadth_first {
            " SEARCH BREADTH FIRST BY "
        } else {
            " SEARCH DEPTH FIRST BY "
        });
        w.names(&self.columns, ", ")?;
        w.sql(" SET ");
        w.ident(&self.sequence)
    }
}
impl SqlEmit for Cycle {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.postgres_only("CYCLE")?;
        w.sql(" CYCLE ");
        w.names(&self.columns, ", ")?;
        w.sql(" SET ");
        w.ident(&self.mark)?;
        if let Some(value) = &self.value {
            w.sql(" TO ");
            value.emit(w)?;
            let Some(default) = &self.default else {
                return w.invalid("CYCLE", "TO requires DEFAULT");
            };
            w.sql(" DEFAULT ");
            default.emit(w)?;
        }
        w.sql(" USING ");
        w.ident(&self.path)
    }
}
impl SqlEmit for Alias {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql(" AS ");
        w.ident(&self.name)?;
        if !self.columns.is_empty() {
            w.sql("(");
            w.names(&self.columns, ", ")?;
            w.sql(")");
        }
        Ok(())
    }
}
impl Alias {
    fn validate_typed_columns(&self, writer: &SqlWriter) -> Result<(), DeparseError> {
        if self.columns.is_empty() {
            Ok(())
        } else {
            writer.invalid(
                "typed table function",
                "typed column definitions cannot follow an alias column list",
            )
        }
    }
}
impl SqlEmit for FromItem {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        crate::sem::with_stack(|| match self {
            Self::Relation(v) => v.emit(w),
            Self::Query(v) => v.emit(w),
            Self::Join(v) => v.emit(w),
            Self::Functions(v) => v.emit(w),
            Self::Group(v) => v.emit(w),
            Self::Sample(v) => v.emit(w),
            Self::Xml(v) => v.emit(w),
            Self::Json(v) => v.emit(w),
        })
    }
}
impl FromItem {
    fn emits_unaliased_group(&self) -> bool {
        match self {
            Self::Join(value) => value.alias.is_none(),
            Self::Group(value) => value.alias.is_none(),
            _ => false,
        }
    }
}
impl SqlEmit for Relation {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if !self.inherit && w.is_postgres() {
            w.sql("ONLY ");
        }
        self.name.emit(w)?;
        if let Some(alias) = &self.alias {
            alias.emit(w)?;
        }
        if let Some(index) = &self.indexed {
            if w.is_postgres() {
                return w.invalid("INDEXED BY", "SQLite-only clause");
            }
            w.sql(" INDEXED BY ");
            w.ident(index)?;
        }
        if self.not_indexed {
            if w.is_postgres() {
                return w.invalid("NOT INDEXED", "SQLite-only clause");
            }
            w.sql(" NOT INDEXED");
        }
        Ok(())
    }
}
impl SqlEmit for DerivedTable {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.lateral {
            w.postgres_only("LATERAL")?;
            w.sql("LATERAL ");
        }
        w.sql("(");
        self.query.emit(w)?;
        w.sql(")");
        if let Some(alias) = &self.alias {
            alias.emit(w)?;
        }
        Ok(())
    }
}
impl SqlEmit for FromGroup {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.items.is_empty() {
            return w.invalid("FROM group", "FROM group requires a table expression");
        }
        if let [item] = self.items.as_slice()
            && item.emits_unaliased_group()
        {
            item.emit(w)?;
            if let Some(alias) = &self.alias {
                alias.emit(w)?;
            }
            return Ok(());
        }
        w.sql("(");
        w.list(&self.items, ", ")?;
        w.sql(")");
        if let Some(alias) = &self.alias {
            alias.emit(w)?;
        }
        Ok(())
    }
}
impl SqlEmit for Join {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql("(");
        self.left.emit(w)?;
        w.sql(" ");
        if self.natural {
            w.sql("NATURAL ");
        }
        self.kind.emit(w)?;
        w.sql(" JOIN ");
        self.right.emit(w)?;
        if let Some(condition) = &self.condition {
            w.sql(" ON ");
            condition.emit(w)?;
        }
        if !self.using.is_empty() {
            w.sql(" USING (");
            w.names(&self.using, ", ")?;
            w.sql(")");
            if let Some(alias) = &self.using_alias {
                w.postgres_only("JOIN USING alias")?;
                alias.emit(w)?;
            }
        }
        w.sql(")");
        if let Some(alias) = &self.alias {
            alias.emit(w)?;
        }
        Ok(())
    }
}
impl SqlEmit for JoinKind {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql(match self {
            Self::Inner => "INNER",
            Self::Left => "LEFT",
            Self::Right => "RIGHT",
            Self::Full => "FULL",
            Self::Cross => "CROSS",
        });
        Ok(())
    }
}
impl SqlEmit for TableFunctions {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.calls.is_empty() || (!self.rows_from && self.calls.len() != 1) {
            return w.invalid("table function", "multiple calls require ROWS FROM");
        }
        if self.lateral {
            w.postgres_only("LATERAL")?;
            w.sql("LATERAL ");
        }
        if self.rows_from {
            w.postgres_only("ROWS FROM")?;
            w.sql("ROWS FROM (");
        }
        w.list(&self.calls, ", ")?;
        if self.rows_from {
            w.sql(")");
        }
        if self.ordinality {
            w.postgres_only("WITH ORDINALITY")?;
            w.sql(" WITH ORDINALITY");
        }
        if let Some(alias) = &self.alias {
            alias.emit(w)?;
        }
        if !self.columns.is_empty() {
            w.postgres_only("typed table function columns")?;
            if let Some(alias) = &self.alias {
                alias.validate_typed_columns(w)?;
            } else {
                w.sql(" AS ");
            }
            w.sql("(");
            w.list(&self.columns, ", ")?;
            w.sql(")");
        }
        Ok(())
    }
}
impl SqlEmit for TableFunction {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        self.expression.emit(w)?;
        if !self.columns.is_empty() {
            w.postgres_only("typed table function")?;
            w.sql(" AS (");
            w.list(&self.columns, ", ")?;
            w.sql(")");
        }
        Ok(())
    }
}
impl SqlEmit for TypedColumn {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.ident(&self.name)?;
        w.sql(" ");
        self.type_name.emit(w)?;
        if let Some(collation) = &self.collation {
            w.sql(" COLLATE ");
            collation.emit(w)?;
        }
        Ok(())
    }
}
impl SqlEmit for TableSample {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.postgres_only("TABLESAMPLE")?;
        self.relation.emit(w)?;
        w.sql(" TABLESAMPLE ");
        self.method.emit(w)?;
        w.sql("(");
        w.list(&self.arguments, ", ")?;
        w.sql(")");
        if let Some(repeat) = &self.repeatable {
            w.sql(" REPEATABLE (");
            repeat.emit(w)?;
            w.sql(")");
        }
        Ok(())
    }
}
impl SqlEmit for Window {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if let Some(name) = &self.name {
            return w.ident(name);
        }
        if self.reference_only {
            let Some(reference) = &self.reference else {
                return w.invalid("OVER", "named window reference is missing its name");
            };
            if !self.partition.is_empty() || !self.order.is_empty() || self.frame.is_some() {
                return w.invalid(
                    "OVER",
                    "named window reference cannot contain a window specification",
                );
            }
            return w.ident(reference);
        }
        self.emit_definition(w)
    }
}
impl Window {
    pub(in crate::ast) fn emit_definition(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql("(");
        if let Some(reference) = &self.reference {
            w.ident(reference)?;
            w.sql(" ");
        }
        if !self.partition.is_empty() {
            w.sql("PARTITION BY ");
            w.list(&self.partition, ", ")?;
            w.sql(" ");
        }
        if !self.order.is_empty() {
            w.sql("ORDER BY ");
            w.list(&self.order, ", ")?;
            w.sql(" ");
        }
        if let Some(frame) = &self.frame {
            frame.emit(w)?;
        }
        w.sql(")");
        Ok(())
    }
}
impl SqlEmit for Frame {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        self.mode.emit(w)?;
        w.sql(" ");
        if let Some(end) = &self.end {
            w.sql("BETWEEN ");
            self.start.emit(w)?;
            w.sql(" AND ");
            end.emit(w)?;
        } else {
            self.start.emit(w)?;
        }
        if let Some(exclusion) = &self.exclusion {
            w.sql(" EXCLUDE ");
            exclusion.emit(w)?;
        }
        Ok(())
    }
}
impl SqlEmit for FrameMode {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql(match self {
            Self::Rows => "ROWS",
            Self::Range => "RANGE",
            Self::Groups => "GROUPS",
        });
        Ok(())
    }
}
impl SqlEmit for FrameBound {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::UnboundedPreceding => w.sql("UNBOUNDED PRECEDING"),
            Self::CurrentRow => w.sql("CURRENT ROW"),
            Self::UnboundedFollowing => w.sql("UNBOUNDED FOLLOWING"),
            Self::Preceding(v) => {
                v.emit(w)?;
                w.sql(" PRECEDING");
            }
            Self::Following(v) => {
                v.emit(w)?;
                w.sql(" FOLLOWING");
            }
        }
        Ok(())
    }
}
impl SqlEmit for FrameExclusion {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql(match self {
            Self::CurrentRow => "CURRENT ROW",
            Self::Group => "GROUP",
            Self::Ties => "TIES",
            Self::NoOthers => "NO OTHERS",
        });
        Ok(())
    }
}
impl SqlEmit for Lock {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.postgres_only("locking clause")?;
        w.sql(" FOR ");
        if let Some(strength) = &self.strength {
            strength.emit(w)?;
        } else {
            return w.invalid("locking clause", "missing strength");
        }
        if !self.relations.is_empty() {
            w.sql(" OF ");
            w.list(&self.relations, ", ")?;
        }
        if let Some(wait) = &self.wait {
            wait.emit(w)?;
        }
        Ok(())
    }
}
impl SqlEmit for LockStrength {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql(match self {
            Self::None => return w.invalid("lock strength", "no lock strength"),
            Self::KeyShare => "KEY SHARE",
            Self::Share => "SHARE",
            Self::NoKeyUpdate => "NO KEY UPDATE",
            Self::Update => "UPDATE",
        });
        Ok(())
    }
}
impl SqlEmit for LockWait {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql(match self {
            Self::Block => "",
            Self::Skip => " SKIP LOCKED",
            Self::Error => " NOWAIT",
        });
        Ok(())
    }
}
impl SqlEmit for Into {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        match self.relation.persistence.as_str() {
            "t" => w.sql("TEMPORARY "),
            "u" => w.sql("UNLOGGED "),
            _ => {}
        }
        self.emit_without_persistence(w)
    }
}
impl Into {
    pub(in crate::ast) fn emit_without_persistence(
        &self,
        w: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        self.relation.name.emit(w)?;
        if !self.columns.is_empty() {
            w.sql("(");
            w.names(&self.columns, ", ")?;
            w.sql(")");
        }
        if let Some(method) = &self.access_method {
            w.sql(" USING ");
            w.ident(method)?;
        }
        if !self.options.is_empty() {
            w.sql(" WITH (");
            w.list(&self.options, ", ")?;
            w.sql(")");
        }
        if let Some(commit) = &self.on_commit {
            commit.emit(w)?;
        }
        if let Some(tablespace) = &self.tablespace {
            w.sql(" TABLESPACE ");
            w.ident(tablespace)?;
        }
        Ok(())
    }
}
impl SqlEmit for CommitAction {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql(match self {
            Self::None => "",
            Self::PreserveRows => " ON COMMIT PRESERVE ROWS",
            Self::DeleteRows => " ON COMMIT DELETE ROWS",
            Self::Drop => " ON COMMIT DROP",
        });
        Ok(())
    }
}
impl SqlEmit for IndexExpression {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if let Some(name) = &self.name {
            w.ident(name)?;
        } else if let Some(expr) = &self.expression {
            w.sql("(");
            expr.emit(w)?;
            w.sql(")");
        } else {
            return w.invalid("index expression", "missing expression and name");
        }
        if let Some(alias) = &self.alias {
            w.sql(" AS ");
            w.ident(alias)?;
        }
        if !self.collation.is_empty() {
            w.sql(" COLLATE ");
            self.collation.emit(w)?;
        }
        if !self.operator_class.is_empty() {
            w.postgres_only("operator class")?;
            w.sql(" ");
            self.operator_class.emit(w)?;
            if !self.options.is_empty() {
                w.sql("(");
                w.list(&self.options, ", ")?;
                w.sql(")");
            }
        }
        if let Some(desc) = self.descending {
            w.sql(if desc { " DESC" } else { " ASC" });
        }
        if let Some(first) = self.nulls_first {
            w.sql(if first { " NULLS FIRST" } else { " NULLS LAST" });
        }
        Ok(())
    }
}
impl Name {
    pub(in crate::ast) const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
impl SqlEmit for SyntaxOption {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if let Some(ns) = &self.namespace {
            w.ident(ns)?;
            w.sql(".");
        }
        w.ident(&self.name)?;
        if let Some(value) = &self.value {
            w.sql(" = ");
            value.emit(w)?;
        }
        Ok(())
    }
}
impl SqlEmit for SyntaxValue {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Expression(v) => v.emit(w),
            Self::Type(v) => v.emit(w),
        }
    }
}
