//! Associate optional frontend type locations with owned SQLite syntax.
//!
//! The parser supplies types in normalized native-tree order: nested CAST
//! arguments precede their target type, and LIMIT count precedes OFFSET even
//! for the comma form. Every event must match one owned TypeName. Absolute
//! boundaries select only that type's spelling; no statement source is retained.

use crate::StringSpan;
use crate::ast::document::DocumentExpr;
use crate::ast::{
    Access, AlterCommand, AlterRelation, Assignment, BooleanExpr, Call, Case, CaseArm, Cast,
    Collate, Column, ColumnConstraint, Conflict, Cte, CteBody, Delete, DerivedTable, Expr, Frame,
    FrameBound, FromGroup, FromItem, Grouping, InTable, IndexExpression, Indirection, Insert, Join,
    NamedArgument, NullTest, OperatorExpr, Order, Query, QueryBody, Raise, Select, SetQuery, Slice,
    Statement, StatementNode, Subquery, Table, TableConstraint, TableFunction, TableFunctions,
    Target, TruthTest, TypeName, TypedColumn, Update, View, Window, With,
};
use crate::sem::AnalysisError;
use crate::sqlite_parser::TypeSpan;

impl Statement {
    pub(crate) fn retain_sqlite_type_syntax(
        &mut self,
        input: &str,
        spans: &[TypeSpan],
    ) -> Result<(), AnalysisError> {
        let mut retainer = SqliteTypeRetainer {
            input,
            statement_span: *self.span(),
            spans: spans.iter(),
        };
        self.node.retain_types(&mut retainer)?;
        if retainer.spans.next().is_some() {
            return Err(AnalysisError::unexpected_input_error(
                "unconsumed SQLite type source locations",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_statement_type_retention {
    use crate::ast::Dialect;
    use crate::parse_statements;

    use ::rusqlite::types::Value;

    #[rstest::rstest]
    #[case::query_clauses(
        r#"
        WITH source(v) AS (
            VALUES (CAST('2' AS "BLOB" INTEGER)), (CAST('3' AS [BLOB] INTEGER))
        )
        SELECT
            CAST(CASE CAST(v AS INTEGER)
                WHEN CAST('2' AS INTEGER) THEN CAST(v AS "BLOB" TEXT)
                ELSE CAST(v AS DOUBLE "PRECISION"(3)) END AS TEXT),
            CAST(sum(CAST(v AS NUMERIC)) FILTER (WHERE CAST(v AS INTEGER) > 0)
                OVER (PARTITION BY CAST(1 AS INTEGER) ORDER BY CAST(v AS NUMERIC)
                    ROWS BETWEEN CAST(1 AS INTEGER) PRECEDING
                        AND CAST(1 AS INTEGER) FOLLOWING) AS TEXT)
        FROM source JOIN (SELECT CAST(1 AS INTEGER) AS x)
            ON CAST(x AS INTEGER) = 1
        WHERE CAST(v AS INTEGER) > 0
        GROUP BY CAST(v AS INTEGER) HAVING CAST(v AS INTEGER) > 0
        WINDOW w AS (PARTITION BY CAST(1 AS NUMERIC) ORDER BY CAST(v AS INTEGER)
            ROWS CAST(1 AS INTEGER) PRECEDING)
        ORDER BY CAST(v AS INTEGER)
        LIMIT CAST(0 AS INTEGER), CAST(1 AS INTEGER)
    "#
    )]
    #[case::nested_and_comma_limit(
        r#"
        SELECT typeof(CAST(CAST('12.5' AS "BLOB" INTEGER) AS [BLOB] TEXT)),
            typeof(CAST('12.5' AS BLOB/*TEXT*/REAL)),
            typeof(CAST('12.5' AS NUMERIC(/*INT*/10, -2)))
        UNION ALL SELECT 'text', 'text', 'text'
        LIMIT CAST(0 AS "INTEGER" /*BLOB*/ NUMERIC), CAST(1 AS [INTEGER] /*TEXT*/ NUMERIC)
    "#
    )]
    fn preserves_cast_results_across_query_clauses(#[case] sql: &str) {
        let rendered = render(sql);
        assert_eq!(query_rows(sql), query_rows(&rendered), "{rendered}");
    }

    #[test]
    fn retains_column_types_and_shared_assignment_sources_across_statements() {
        let sql = r#"
            CREATE TABLE t (
                a INTEGER PRIMARY KEY,
                b [BLOB] TEXT,
                c NUMERIC(/*INT*/10, -2) DEFAULT (CAST('3' AS [BLOB] INT)),
                CHECK (CAST(a AS INTEGER) >= 0)
            );
            INSERT INTO t(a, b) SELECT CAST('7' AS INTEGER), CAST('8' AS TEXT)
                ON CONFLICT(a) WHERE CAST(a AS INTEGER) > 0 DO UPDATE
                SET (b, c) = (SELECT CAST('9' AS TEXT), CAST('10' AS NUMERIC))
                WHERE CAST(t.c AS NUMERIC) > 0
                RETURNING CAST(c AS TEXT);
            UPDATE t SET (a, b) = (SELECT CAST('11' AS INTEGER), CAST('12' AS TEXT)),
                c = CAST('13' AS INTEGER)
                WHERE CAST(a AS NUMERIC) > 0 RETURNING CAST(c AS TEXT);
            DELETE FROM t WHERE CAST(a AS NUMERIC) < 0 RETURNING CAST(b AS TEXT);
            ALTER TABLE t ADD COLUMN d BLOB/*TEXT*/REAL DEFAULT 14;
            CREATE VIEW v AS SELECT CAST(b AS TEXT) AS b, CAST(c AS INTEGER) AS c FROM t;
        "#;
        let rendered = render(sql);
        let query = "SELECT a, b, c, d, typeof(b), typeof(c), typeof(d) FROM t";
        assert_eq!(
            script_rows(sql, query),
            script_rows(&rendered, query),
            "{rendered}"
        );
        assert_eq!(
            script_rows(sql, "SELECT name, type FROM pragma_table_info('t')"),
            script_rows(&rendered, "SELECT name, type FROM pragma_table_info('t')"),
            "{rendered}",
        );
        assert_eq!(
            script_rows(sql, "SELECT * FROM v"),
            script_rows(&rendered, "SELECT * FROM v"),
            "{rendered}",
        );
    }

    fn render(sql: &str) -> String {
        let mut rendered = String::new();
        for statement in parse_statements(sql, Dialect::sqlite()) {
            let statement = statement.expect("common SQLite statement");
            rendered.push_str(
                &statement
                    .deparse_sql(Dialect::sqlite())
                    .expect("render retained types"),
            );
            rendered.push(';');
        }
        rendered
    }

    fn query_rows(sql: &str) -> Vec<Vec<Value>> {
        let connection = ::rusqlite::Connection::open_in_memory().unwrap();
        rows(&connection, sql)
    }

    fn script_rows(script: &str, sql: &str) -> Vec<Vec<Value>> {
        let connection = ::rusqlite::Connection::open_in_memory().unwrap();
        connection.execute_batch(script).expect("SQLite script");
        rows(&connection, sql)
    }

    fn rows(connection: &::rusqlite::Connection, sql: &str) -> Vec<Vec<Value>> {
        let mut statement = connection.prepare(sql).expect("SQLite query");
        let columns = statement.column_count();
        let mut query = statement.query([]).unwrap();
        let mut values = Vec::new();
        while let Some(row) = query.next().unwrap() {
            let mut cells = Vec::new();
            for column in 0..columns {
                cells.push(row.get(column).unwrap());
            }
            values.push(cells);
        }
        values
    }
}

struct SqliteTypeRetainer<'a> {
    input: &'a str,
    statement_span: StringSpan,
    spans: std::slice::Iter<'a, TypeSpan>,
}

impl SqliteTypeRetainer<'_> {
    fn list<T: RetainSqliteTypes>(&mut self, nodes: &mut [T]) -> Result<(), AnalysisError> {
        for node in nodes {
            node.retain_types(self)?;
        }
        Ok(())
    }

    fn optional<T: RetainSqliteTypes>(
        &mut self,
        node: &mut Option<T>,
    ) -> Result<(), AnalysisError> {
        if let Some(node) = node {
            node.retain_types(self)?;
        }
        Ok(())
    }

    fn type_name(&mut self, type_name: &mut TypeName) -> Result<(), AnalysisError> {
        // The frontend also accepts CAST(expr AS) as an extension. Its empty
        // TypeName has no parse_type completion event; emission rejects it.
        if type_name.name.0.is_empty() {
            return Ok(());
        }
        let Some(span) = self.spans.next() else {
            return Err(AnalysisError::unexpected_input_error(
                "missing SQLite type source location",
            ));
        };
        let statement_end = self.statement_span.end_pos();
        let statement_start = statement_end - self.statement_span.len();
        if span.start() < statement_start
            || span.end() > statement_end
            || span.start() >= span.end()
        {
            return Err(AnalysisError::unexpected_input_error(
                "SQLite type source location outside its statement",
            ));
        }
        let Some(spelling) = self.input.get(span.start()..span.end()) else {
            return Err(AnalysisError::unexpected_input_error(
                "invalid SQLite type source boundary",
            ));
        };
        if !type_name.matches_sqlite_frontend_name(span.name())? {
            return Err(AnalysisError::unexpected_input_error(&format!(
                "SQLite type source association does not match {:?}",
                span.name(),
            )));
        }
        type_name.retain_sqlite_syntax(spelling)
    }

    fn assignments(&mut self, assignments: &mut [Assignment]) -> Result<(), AnalysisError> {
        let mut position = 0;
        while position < assignments.len() {
            let assignment = &mut assignments[position];
            self.list(&mut assignment.indirection)?;
            let data = if let Some(value) = assignment.value.as_mut() {
                value.multi_assignment_data_mut()
            } else {
                None
            };
            let Some((source, column, width)) = data else {
                self.optional(&mut assignment.value)?;
                position += 1;
                continue;
            };
            if column != 1 || width <= 1 {
                return Err(AnalysisError::unexpected_input_error(
                    "invalid SQLite multi-column assignment group",
                ));
            }
            source.retain_types(self)?;
            // The common converter clones one native expression for each
            // assigned column. Consume its events once, then retain identical
            // owned syntax in every copy of that same assignment source.
            let retained = source.clone();
            let width = usize::try_from(width).map_err(AnalysisError::generic_error)?;
            let end = position.checked_add(width).ok_or_else(|| {
                AnalysisError::unexpected_input_error("SQLite assignment group length")
            })?;
            let Some(group) = assignments.get_mut(position..end) else {
                return Err(AnalysisError::unexpected_input_error(
                    "incomplete SQLite multi-column assignment group",
                ));
            };
            for (offset, assignment) in group.iter_mut().enumerate().skip(1) {
                let data = if let Some(value) = assignment.value.as_mut() {
                    value.multi_assignment_data_mut()
                } else {
                    None
                };
                let Some((source, column, columns)) = data else {
                    return Err(AnalysisError::unexpected_input_error(
                        "inconsistent SQLite multi-column assignment group",
                    ));
                };
                if column as usize != offset + 1 || columns as usize != width {
                    return Err(AnalysisError::unexpected_input_error(
                        "inconsistent SQLite multi-column assignment positions",
                    ));
                }
                *source = retained.clone();
            }
            position = end;
        }
        Ok(())
    }
}

/// Type-local traversal keeps recursive syntax ownership with each payload.
/// The two collection helpers avoid imposing a second AST or a generic visitor
/// interface on either parser frontend.
trait RetainSqliteTypes {
    #[inline]
    fn retain_types(&mut self, retainer: &mut SqliteTypeRetainer<'_>) -> Result<(), AnalysisError> {
        crate::sem::with_stack(|| self.retain_types_inner(retainer))
    }

    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError>;
}

impl RetainSqliteTypes for StatementNode {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Query(value) => value.retain_types(retainer),
            Self::Insert(value) => value.retain_types(retainer),
            Self::Update(value) => value.retain_types(retainer),
            Self::Delete(value) => value.retain_types(retainer),
            Self::CreateTrigger(value) => value.retain_types(retainer),
            Self::View(value) => value.retain_types(retainer),
            Self::Table(value) => value.retain_types(retainer),
            Self::AlterRelation(value) => value.retain_types(retainer),
            Self::DropTable(_) | Self::ViewWrite(_) => Ok(()),
            Self::Extension(_)
            | Self::VariableSet(_)
            | Self::EnumType(_)
            | Self::Sequence(_)
            | Self::AlterSequence(_)
            | Self::Comment(_)
            | Self::AlterOwner(_)
            | Self::CompositeType(_)
            | Self::Domain(_)
            | Self::Function(_)
            | Self::Schema(_)
            | Self::Index(_)
            | Self::TableAs(_) => Err(AnalysisError::unexpected_input_error(
                "non-SQLite statement in SQLite type retention",
            )),
        }
    }
}

impl RetainSqliteTypes for TypeName {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        retainer.list(&mut self.modifiers)?;
        retainer.type_name(self)
    }
}

impl RetainSqliteTypes for Table {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        retainer.list(&mut self.columns)?;
        retainer.list(&mut self.constraints)?;
        retainer.optional(&mut self.query)
    }
}

impl RetainSqliteTypes for Column {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        retainer.optional(&mut self.type_name)?;
        retainer.list(&mut self.constraints)
    }
}

impl RetainSqliteTypes for TableConstraint {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.constraint.retain_types(retainer)?;
        retainer.list(&mut self.order)
    }
}

impl RetainSqliteTypes for ColumnConstraint {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Default(value) | Self::Check(value) | Self::Generated(value, _) => {
                value.retain_types(retainer)
            }
            Self::Nullability(..)
            | Self::PrimaryKey(..)
            | Self::Unique(_)
            | Self::Identity(_)
            | Self::ForeignKey(_)
            | Self::Exclusion
            | Self::Deferrability(_)
            | Self::Deferred
            | Self::Immediate
            | Self::Collate(_) => Ok(()),
        }
    }
}

impl RetainSqliteTypes for AlterRelation {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        retainer.list(&mut self.commands)
    }
}

impl RetainSqliteTypes for AlterCommand {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::AddColumn(value) | Self::AlterColumn(_, value) => value.retain_types(retainer),
            Self::Default(_, value) => value.retain_types(retainer),
            Self::Constraint(value) => value.retain_types(retainer),
            Self::Owner(_)
            | Self::Cluster(_)
            | Self::Rename(_)
            | Self::RenameColumn(..)
            | Self::DropColumn(_) => Ok(()),
            Self::Postgres(_) | Self::Unsupported(_) => Err(AnalysisError::unexpected_input_error(
                "non-SQLite ALTER command in SQLite type retention",
            )),
        }
    }
}

impl RetainSqliteTypes for View {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.query.retain_types(retainer)
    }
}

impl RetainSqliteTypes for Expr {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Operator(value) => value.retain_types(retainer),
            Self::Boolean(value) => value.retain_types(retainer),
            Self::NullTest(value) => value.retain_types(retainer),
            Self::TruthTest(value) => value.retain_types(retainer),
            Self::Cast(value) => value.retain_types(retainer),
            Self::Collate(value) => value.retain_types(retainer),
            Self::Call(value) => value.retain_types(retainer),
            Self::Case(value) => value.retain_types(retainer),
            Self::Subquery(value) => value.retain_types(retainer),
            Self::Array(values)
            | Self::Row(values)
            | Self::List(values)
            | Self::Parenthesized(values)
            | Self::Grouping(values) => retainer.list(values),
            Self::Indirection(value) => value.retain_types(retainer),
            Self::NamedArgument(value) => value.retain_types(retainer),
            Self::Raise(value) => value.retain_types(retainer),
            Self::InTable(value) => value.retain_types(retainer),
            Self::Document(value) => value.retain_types(retainer),
            Self::Literal(_)
            | Self::Column(_)
            | Self::Star(_)
            | Self::Parameter(_)
            | Self::SqlValue(_)
            | Self::Default
            | Self::CurrentOf(_)
            | Self::MergeAction => Ok(()),
        }
    }
}

impl RetainSqliteTypes for OperatorExpr {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        retainer.optional(&mut self.left)?;
        retainer.optional(&mut self.right)?;
        retainer.optional(&mut self.escape)
    }
}

impl RetainSqliteTypes for BooleanExpr {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        retainer.list(&mut self.arguments)
    }
}

impl RetainSqliteTypes for NullTest {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.argument.retain_types(retainer)
    }
}

impl RetainSqliteTypes for TruthTest {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.argument.retain_types(retainer)
    }
}

impl RetainSqliteTypes for Cast {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.argument.retain_types(retainer)?;
        self.target.retain_types(retainer)
    }
}

impl RetainSqliteTypes for Collate {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.argument.retain_types(retainer)
    }
}

impl RetainSqliteTypes for Call {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        retainer.list(&mut self.arguments)?;
        retainer.list(&mut self.order)?;
        retainer.list(&mut self.within_order)?;
        retainer.optional(&mut self.filter)?;
        retainer.optional(&mut self.over)
    }
}

impl RetainSqliteTypes for Case {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        retainer.optional(&mut self.argument)?;
        retainer.list(&mut self.arms)?;
        retainer.optional(&mut self.otherwise)
    }
}

impl RetainSqliteTypes for CaseArm {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.condition.retain_types(retainer)?;
        self.result.retain_types(retainer)
    }
}

impl RetainSqliteTypes for Subquery {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        retainer.optional(&mut self.test)?;
        self.query.retain_types(retainer)
    }
}

impl RetainSqliteTypes for Indirection {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.argument.retain_types(retainer)?;
        retainer.list(&mut self.items)
    }
}

impl RetainSqliteTypes for Access {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Index(value) => value.retain_types(retainer),
            Self::Slice(value) => value.retain_types(retainer),
            Self::Field(_) | Self::Star => Ok(()),
        }
    }
}

impl RetainSqliteTypes for Slice {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        retainer.optional(&mut self.lower)?;
        retainer.optional(&mut self.upper)
    }
}

impl RetainSqliteTypes for NamedArgument {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.argument.retain_types(retainer)
    }
}

impl RetainSqliteTypes for Raise {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        retainer.optional(&mut self.message)
    }
}

impl RetainSqliteTypes for InTable {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.argument.retain_types(retainer)?;
        retainer.list(&mut self.arguments)
    }
}

impl RetainSqliteTypes for DocumentExpr {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        if let Some((source, _, _)) = self.multi_assignment_data_mut() {
            source.retain_types(retainer)
        } else {
            Err(AnalysisError::unexpected_input_error(
                "non-SQLite document expression in SQLite type retention",
            ))
        }
    }
}

impl RetainSqliteTypes for Query {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.with.retain_types(retainer)?;
        self.body.retain_types(retainer)?;
        retainer.list(&mut self.order)?;
        retainer.optional(&mut self.limit)?;
        retainer.optional(&mut self.offset)
    }
}

impl RetainSqliteTypes for QueryBody {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Select(value) => value.retain_types(retainer),
            Self::Set(value) => value.retain_types(retainer),
            Self::Values(rows) => {
                for row in rows {
                    retainer.list(row)?;
                }
                Ok(())
            }
        }
    }
}

impl RetainSqliteTypes for SetQuery {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.left.retain_types(retainer)?;
        self.right.retain_types(retainer)
    }
}

impl RetainSqliteTypes for Select {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        if let Some(distinct) = &mut self.distinct {
            retainer.list(distinct)?;
        }
        retainer.list(&mut self.targets)?;
        retainer.list(&mut self.from)?;
        retainer.optional(&mut self.predicate)?;
        retainer.list(&mut self.group)?;
        retainer.optional(&mut self.having)?;
        retainer.list(&mut self.windows)
    }
}

impl RetainSqliteTypes for Target {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.expression.retain_types(retainer)
    }
}

impl RetainSqliteTypes for Grouping {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Expression(value) => value.retain_types(retainer),
            Self::Rollup(values)
            | Self::Cube(values)
            | Self::Sets(values)
            | Self::Simple(values) => retainer.list(values),
            Self::Empty => Ok(()),
        }
    }
}

impl RetainSqliteTypes for Order {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.expression.retain_types(retainer)
    }
}

impl RetainSqliteTypes for With {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        retainer.list(&mut self.entries)
    }
}

impl RetainSqliteTypes for Cte {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.body.retain_types(retainer)
    }
}

impl RetainSqliteTypes for CteBody {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Query(value) => value.retain_types(retainer),
            Self::Insert(value) => value.retain_types(retainer),
            Self::Update(value) => value.retain_types(retainer),
            Self::Delete(value) => value.retain_types(retainer),
            Self::Merge(_) => Err(AnalysisError::unexpected_input_error(
                "MERGE in SQLite type retention",
            )),
        }
    }
}

impl RetainSqliteTypes for FromItem {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Relation(_) => Ok(()),
            Self::Query(value) => value.retain_types(retainer),
            Self::Join(value) => value.retain_types(retainer),
            Self::Functions(value) => value.retain_types(retainer),
            Self::Group(value) => value.retain_types(retainer),
            Self::Sample(_) | Self::Xml(_) | Self::Json(_) => {
                Err(AnalysisError::unexpected_input_error(
                    "non-SQLite FROM item in SQLite type retention",
                ))
            }
        }
    }
}

impl RetainSqliteTypes for DerivedTable {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.query.retain_types(retainer)
    }
}

impl RetainSqliteTypes for FromGroup {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        retainer.list(&mut self.items)
    }
}

impl RetainSqliteTypes for Join {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.left.retain_types(retainer)?;
        self.right.retain_types(retainer)?;
        retainer.optional(&mut self.condition)
    }
}

impl RetainSqliteTypes for TableFunctions {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        retainer.list(&mut self.calls)?;
        retainer.list(&mut self.columns)
    }
}

impl RetainSqliteTypes for TableFunction {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.expression.retain_types(retainer)?;
        retainer.list(&mut self.columns)
    }
}

impl RetainSqliteTypes for TypedColumn {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.type_name.retain_types(retainer)
    }
}

impl RetainSqliteTypes for Window {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        retainer.list(&mut self.partition)?;
        retainer.list(&mut self.order)?;
        retainer.optional(&mut self.frame)
    }
}

impl RetainSqliteTypes for Frame {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.start.retain_types(retainer)?;
        retainer.optional(&mut self.end)
    }
}

impl RetainSqliteTypes for FrameBound {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Preceding(value) | Self::Following(value) => value.retain_types(retainer),
            Self::UnboundedPreceding | Self::CurrentRow | Self::UnboundedFollowing => Ok(()),
        }
    }
}

impl RetainSqliteTypes for Insert {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.with.retain_types(retainer)?;
        retainer.assignments(&mut self.columns)?;
        retainer.optional(&mut self.source)?;
        retainer.optional(&mut self.conflict)?;
        retainer.list(&mut self.returning)
    }
}

impl RetainSqliteTypes for Conflict {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        retainer.list(&mut self.inference)?;
        retainer.optional(&mut self.inference_predicate)?;
        retainer.assignments(&mut self.assignments)?;
        retainer.optional(&mut self.predicate)?;
        if let Some(next) = &mut self.next {
            next.retain_types(retainer)?;
        }
        Ok(())
    }
}

impl RetainSqliteTypes for IndexExpression {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        retainer.optional(&mut self.expression)
    }
}

impl RetainSqliteTypes for Update {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.with.retain_types(retainer)?;
        retainer.assignments(&mut self.assignments)?;
        retainer.list(&mut self.from)?;
        retainer.optional(&mut self.predicate)?;
        retainer.list(&mut self.returning)
    }
}

impl RetainSqliteTypes for Delete {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        self.with.retain_types(retainer)?;
        retainer.list(&mut self.using)?;
        retainer.optional(&mut self.predicate)?;
        retainer.list(&mut self.returning)
    }
}

impl RetainSqliteTypes for crate::ast::CreateTrigger {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        retainer.optional(&mut self.predicate)?;
        self.body.retain_types(retainer)
    }
}

impl RetainSqliteTypes for crate::ast::TriggerBody {
    fn retain_types_inner(
        &mut self,
        retainer: &mut SqliteTypeRetainer<'_>,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Commands(commands) => retainer.list(commands),
            Self::Function(_) => Err(AnalysisError::unexpected_input_error(
                "function trigger body in SQLite type retention",
            )),
        }
    }
}
