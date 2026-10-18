//! Supported inspection of the owned common AST.

use crate::StringSpan;
use crate::ast::{Cte, CteBody, Dialect, StatementNode, With};

/// An owned, unresolved statement in an explicitly selected SQL dialect.
///
/// Parsing preserves syntax without resolving names against a catalog. Private
/// nodes retain additional syntax even where no inspection method is exposed yet.
#[derive(Clone, Debug)]
pub struct Statement {
    pub(super) node: StatementNode,
    pub(super) dialect: Dialect,
    span: StringSpan,
}

impl Statement {
    pub(crate) const fn from_node(node: StatementNode, dialect: Dialect, span: StringSpan) -> Self {
        Self {
            node,
            dialect,
            span,
        }
    }

    /// Range of this statement in the original input, in UTF-8 byte offsets.
    ///
    /// The range identifies the input supplied to `parse_statements`. It is
    /// retained when the statement is cloned or edited; it does not describe
    /// rendered SQL after AST mutations.
    pub const fn span(&self) -> &StringSpan {
        &self.span
    }

    /// Dialect explicitly chosen when this statement was parsed.
    pub const fn dialect(&self) -> Dialect {
        self.dialect
    }

    /// Kind of SQL statement, independent of the parser representation.
    pub const fn kind(&self) -> StatementKind {
        self.node.kind()
    }

    /// Whether this is a query (including VALUES and set operations).
    pub const fn is_query(&self) -> bool {
        self.node.is_query()
    }

    /// Whether this is an INSERT statement.
    pub const fn is_insert(&self) -> bool {
        self.node.is_insert()
    }

    /// Whether this is an UPDATE statement.
    pub const fn is_update(&self) -> bool {
        self.node.is_update()
    }

    /// Whether this is a DELETE statement.
    pub const fn is_delete(&self) -> bool {
        self.node.is_delete()
    }

    /// Whether this is a MERGE statement.
    pub const fn is_merge(&self) -> bool {
        self.node.is_merge()
    }

    /// Analyze this common statement directly using the supplied context.
    ///
    /// No SQL reparsing or catalog-file discovery occurs. This uses the same
    /// resolver and diagnostic reporting as the normal semantic-analysis path.
    ///
    /// # Panics
    ///
    /// CREATE TRIGGER analysis is not implemented yet (#602 / #555).
    ///
    /// # Errors
    ///
    /// Returns a dialect mismatch, resolver, or diagnostic reporting error.
    /// The statement dialect must match the supplied context metadata.
    pub fn analyze<TContext: crate::sem::ParseContext>(
        &self,
        context: TContext,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError> {
        crate::sem::AnalysisError::check_dialect(self.dialect, context.dialect())?;
        self.node.resolve_and_apply(context)
    }

    /// Inspect an existing WITH clause without exposing mutable node storage.
    pub const fn with_clause(&self) -> Option<WithClause<'_>> {
        if let Some(node) = self.node.with_node() {
            WithClause::existing(node, self.dialect)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test_statement_analyze {
    use crate::ast::{CteDefinition, Dialect, HaveWithClause, Statement};
    use crate::parse_statements;
    use crate::sem::PgBuiltInType;
    use crate::test_helpers::TestParseContext;

    fn statement(sql: &str, dialect: Dialect) -> Statement {
        parse_statements(sql, dialect).next().unwrap().unwrap()
    }

    #[rstest::rstest]
    #[case::postgres(Dialect::postgres())]
    #[cfg_attr(feature = "sqlite", case::sqlite(Dialect::sqlite()))]
    fn analyzes_structurally_inserted_dependencies(#[case] dialect: Dialect) {
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
        let analyzed = base.analyze(context).unwrap();
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let column = query.result_columns().get_column_at(0).unwrap();
        assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
        assert_eq!(column.is_not_null(), &Some(true));
    }
}

/// Parser-independent statement category.
///
/// Trigger and rule statements have dedicated categories; the former aggregate
/// `ViewWrite` category has been removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatementKind {
    /// An unresolved PostgreSQL DROP RULE statement. Analysis awaits #605.
    DropRule,
    /// An unresolved CREATE TRIGGER definition. Analysis awaits #602 / #555.
    CreateTrigger,
    /// An unresolved PostgreSQL CREATE RULE definition. Analysis awaits #604.
    CreateRule,
    /// An unresolved DROP TRIGGER statement. Analysis awaits #555.
    DropTrigger,
    /// SELECT, VALUES, or a set operation.
    Query,
    /// INSERT.
    Insert,
    /// UPDATE.
    Update,
    /// DELETE.
    Delete,
    /// MERGE.
    Merge,
    /// A view definition.
    View,
    /// An extension definition.
    Extension,
    /// A variable assignment.
    VariableSet,
    /// An enum type definition.
    EnumType,
    /// A sequence definition.
    Sequence,
    /// DROP TABLE.
    DropTable,
    /// ALTER SEQUENCE.
    AlterSequence,
    /// COMMENT.
    Comment,
    /// An ownership change.
    AlterOwner,
    /// A composite type definition.
    CompositeType,
    /// A domain definition.
    Domain,
    /// A function definition.
    Function,
    /// A schema definition.
    Schema,
    /// An index definition.
    Index,
    /// CREATE TABLE AS.
    TableAs,
    /// A table definition.
    Table,
    /// ALTER TABLE or a related relation change.
    AlterRelation,
}

impl StatementKind {
    /// The CREATE TRIGGER category.
    pub const fn create_trigger() -> Self {
        Self::CreateTrigger
    }

    /// The PostgreSQL CREATE RULE category.
    pub const fn create_rule() -> Self {
        Self::CreateRule
    }

    /// The DROP TRIGGER category.
    pub const fn drop_trigger() -> Self {
        Self::DropTrigger
    }

    /// The PostgreSQL DROP RULE category.
    pub const fn drop_rule() -> Self {
        Self::DropRule
    }
    /// The query category.
    pub const fn query() -> Self {
        Self::Query
    }

    /// The insert category.
    pub const fn insert() -> Self {
        Self::Insert
    }

    /// The update category.
    pub const fn update() -> Self {
        Self::Update
    }

    /// The delete category.
    pub const fn delete() -> Self {
        Self::Delete
    }

    /// The merge category.
    pub const fn merge() -> Self {
        Self::Merge
    }

    /// The view category.
    pub const fn view() -> Self {
        Self::View
    }

    /// The extension category.
    pub const fn extension() -> Self {
        Self::Extension
    }

    /// The variable set category.
    pub const fn variable_set() -> Self {
        Self::VariableSet
    }

    /// The enum type category.
    pub const fn enum_type() -> Self {
        Self::EnumType
    }

    /// The sequence category.
    pub const fn sequence() -> Self {
        Self::Sequence
    }

    /// The drop table category.
    pub const fn drop_table() -> Self {
        Self::DropTable
    }

    /// The alter sequence category.
    pub const fn alter_sequence() -> Self {
        Self::AlterSequence
    }

    /// The comment category.
    pub const fn comment() -> Self {
        Self::Comment
    }

    /// The alter owner category.
    pub const fn alter_owner() -> Self {
        Self::AlterOwner
    }

    /// The composite type category.
    pub const fn composite_type() -> Self {
        Self::CompositeType
    }

    /// The domain category.
    pub const fn domain() -> Self {
        Self::Domain
    }

    /// The function category.
    pub const fn function() -> Self {
        Self::Function
    }

    /// The schema category.
    pub const fn schema() -> Self {
        Self::Schema
    }

    /// The index category.
    pub const fn index() -> Self {
        Self::Index
    }

    /// The table as category.
    pub const fn table_as() -> Self {
        Self::TableAs
    }

    /// The table category.
    pub const fn table() -> Self {
        Self::Table
    }

    /// The alter relation category.
    pub const fn alter_relation() -> Self {
        Self::AlterRelation
    }
}

impl StatementNode {
    pub(super) const fn kind(&self) -> StatementKind {
        match self {
            Self::Query(_) => StatementKind::query(),
            Self::Insert(_) => StatementKind::insert(),
            Self::Update(_) => StatementKind::update(),
            Self::Delete(_) => StatementKind::delete(),
            Self::Merge(_) => StatementKind::merge(),
            Self::View(_) => StatementKind::view(),
            Self::CreateTrigger(_) => StatementKind::create_trigger(),
            Self::CreateRule(_) => StatementKind::create_rule(),
            Self::DropTrigger(_) => StatementKind::drop_trigger(),
            Self::DropRule(_) => StatementKind::drop_rule(),
            Self::Extension(_) => StatementKind::extension(),
            Self::VariableSet(_) => StatementKind::variable_set(),
            Self::EnumType(_) => StatementKind::enum_type(),
            Self::Sequence(_) => StatementKind::sequence(),
            Self::DropTable(_) => StatementKind::drop_table(),
            Self::AlterSequence(_) => StatementKind::alter_sequence(),
            Self::Comment(_) => StatementKind::comment(),
            Self::AlterOwner(_) => StatementKind::alter_owner(),
            Self::CompositeType(_) => StatementKind::composite_type(),
            Self::Domain(_) => StatementKind::domain(),
            Self::Function(_) => StatementKind::function(),
            Self::Schema(_) => StatementKind::schema(),
            Self::Index(_) => StatementKind::index(),
            Self::TableAs(_) => StatementKind::table_as(),
            Self::Table(_) => StatementKind::table(),
            Self::AlterRelation(_) => StatementKind::alter_relation(),
        }
    }

    const fn is_query(&self) -> bool {
        matches!(self, Self::Query(_))
    }

    const fn is_delete(&self) -> bool {
        matches!(self, Self::Delete(_))
    }

    const fn is_update(&self) -> bool {
        matches!(self, Self::Update(_))
    }

    const fn is_insert(&self) -> bool {
        matches!(self, Self::Insert(_))
    }

    const fn is_merge(&self) -> bool {
        matches!(self, Self::Merge(_))
    }

    pub(super) const fn with_node(&self) -> Option<&With> {
        match self {
            Self::Query(node) => Some(&node.with),
            Self::Insert(node) => Some(&node.with),
            Self::Update(node) => Some(&node.with),
            Self::Delete(node) => Some(&node.with),
            Self::Merge(node) => Some(node.with_node()),
            _ => None,
        }
    }
}

/// A read-only view of a WITH clause and its ordered CTE collection.
#[derive(Clone, Copy, Debug)]
pub struct WithClause<'a> {
    pub(super) node: &'a With,
    pub(super) dialect: Dialect,
}

impl<'a> WithClause<'a> {
    pub(super) const fn existing(node: &'a With, dialect: Dialect) -> Option<Self> {
        if node.entries.is_empty() {
            None
        } else {
            Some(Self { node, dialect })
        }
    }

    /// Whether WITH RECURSIVE was specified.
    pub const fn is_recursive(&self) -> bool {
        self.node.recursive
    }

    /// Number of CTEs in source order.
    pub const fn len(&self) -> usize {
        self.node.entries.len()
    }

    /// Whether the clause contains no CTEs.
    pub const fn is_empty(&self) -> bool {
        self.node.entries.is_empty()
    }

    /// Inspect CTEs in their source order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = CommonTableExpression<'a>> + '_ {
        self.node.entries.iter().map(|node| CommonTableExpression {
            node,
            dialect: self.dialect,
        })
    }

    /// Find a CTE using its parsed identifier spelling.
    ///
    /// PostgreSQL spelling is already normalized by the parser. SQLite compares
    /// ASCII letters without regard to case, matching its identifier behavior.
    pub fn find(&self, name: &str) -> Option<CommonTableExpression<'a>> {
        self.node
            .entries
            .iter()
            .find(|node| self.dialect.same_identifier(&node.name, name))
            .map(|node| CommonTableExpression {
                node,
                dialect: self.dialect,
            })
    }
}

impl Dialect {
    pub(super) fn same_identifier(self, left: &str, right: &str) -> bool {
        if self == Self::sqlite() {
            left.eq_ignore_ascii_case(right)
        } else {
            left == right
        }
    }
}

/// A read-only CTE, retaining its declaration attributes and body.
#[derive(Clone, Copy, Debug)]
pub struct CommonTableExpression<'a> {
    pub(super) node: &'a Cte,
    pub(super) dialect: Dialect,
}

impl<'a> CommonTableExpression<'a> {
    /// Parsed CTE identifier spelling.
    pub fn name(&self) -> &'a str {
        &self.node.name
    }

    /// Declared column aliases in their original order.
    pub fn columns(&self) -> impl ExactSizeIterator<Item = &'a str> + '_ {
        self.node.columns.iter().map(|column| column.name.as_str())
    }

    /// Explicit materialization choice, or None when unspecified.
    pub const fn materialized(&self) -> Option<bool> {
        self.node.materialized
    }

    /// Whether a SEARCH clause is retained.
    pub const fn has_search(&self) -> bool {
        self.node.search.is_some()
    }

    /// Whether a CYCLE clause is retained.
    pub const fn has_cycle(&self) -> bool {
        self.node.cycle.is_some()
    }

    /// Kind of statement used as the CTE body.
    pub const fn body_kind(&self) -> StatementKind {
        self.node.body.kind()
    }

    /// Inspect a WITH clause belonging to the CTE body itself.
    pub const fn with_clause(&self) -> Option<WithClause<'a>> {
        WithClause::existing(self.node.body.with_node(), self.dialect)
    }
}

impl CteBody {
    pub(super) const fn kind(&self) -> StatementKind {
        match self {
            Self::Query(_) => StatementKind::query(),
            Self::Insert(_) => StatementKind::insert(),
            Self::Update(_) => StatementKind::update(),
            Self::Delete(_) => StatementKind::delete(),
            Self::Merge(_) => StatementKind::merge(),
        }
    }

    pub(super) const fn with_node(&self) -> &With {
        match self {
            Self::Query(node) => &node.with,
            Self::Insert(node) => &node.with,
            Self::Update(node) => &node.with,
            Self::Delete(node) => &node.with,
            Self::Merge(node) => node.with_node(),
        }
    }
}

#[cfg(test)]
mod test_parameter_analysis {
    use crate::ast::Dialect;
    use crate::parse_statements;
    use crate::sem::{
        BinaryOperatorDefinition, CastContext, CastDefinition, FunctionOverloadCollection,
        FunctionParam, OperatorDefinition, OperatorName, OverloadVariant, ParseContext,
        PgBuiltInType,
    };
    use crate::test_helpers::TestParseContext;
    use std::collections::BTreeMap;

    fn context() -> TestParseContext {
        let mut context = TestParseContext::default();
        let int = PgBuiltInType::int4();
        let small = PgBuiltInType::int2();
        let text = PgBuiltInType::text();
        context.setup_type(int.clone());
        context.setup_type(small.clone());
        context.setup_type(text.clone());
        context.setup_type(PgBuiltInType::bool());
        for ty in [&int, &small, &text] {
            context.set_get_implicit_cast_result(
                ty,
                ty,
                Some(CastDefinition::new(CastContext::no_conversion())),
            );
        }
        context.set_get_implicit_cast_result(
            &small,
            &int,
            Some(CastDefinition::new(CastContext::implicit())),
        );
        let add =
            OperatorDefinition::binary(&[BinaryOperatorDefinition::new(&int, &int, &int, true)]);
        context.set_get_operator_op_result(
            &OperatorName::from_parts(None, "+"),
            true,
            true,
            Some(&add),
        );
        let lower = OverloadVariant::new(
            &Some(text.clone()),
            &vec![FunctionParam::new_input_param(&None, &Some(text), &None)],
            true,
            false,
        );
        context.set_get_function_impl_result(
            &crate::sem::FullName::with_schema("pg_catalog", "lower"),
            &FunctionOverloadCollection::new(&[lower]),
        );
        context
    }

    fn analyze(
        sql: &str,
        dialect: Dialect,
    ) -> Result<crate::AstAndContextPair<TestParseContext>, crate::sem::AnalysisError> {
        parse_statements(sql, dialect)
            .next()
            .unwrap()
            .unwrap()
            .analyze(context().with_dialect(dialect))
    }

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
        let rendered = problems[0]
            .render_report(&::sql_fun_core::HighlighterTheme::default(), sql.to_owned())
            .unwrap();
        assert!(rendered.contains("$1"));
        assert!(
            !analyzed
                .context()
                .parameter_constraints()
                .allows(&BTreeMap::new())
        );
    }

    #[test]
    fn tied_best_overloads_are_not_accepted_as_a_type_range() {
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
        let analyzed = parse_statements("select $1 + 1", Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .analyze(context)
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
        let first = analyze("select $7 + 1 as n", Dialect::postgres()).unwrap();
        assert!(
            first
                .context()
                .parameter_constraints()
                .parameter(7)
                .is_some()
        );
        let second = parse_statements("select 1 as n", Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .analyze(first.into_context())
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
        let first = analyze("select $7 + 1", Dialect::postgres()).unwrap();
        let plain = parse_statements("select 1", Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .analyze(first.into_context())
            .unwrap();
        let third = parse_statements("select $2 + 1", Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .analyze(plain.into_context())
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
        let before = crate::sem::parameter_resolution::inference_runs();
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
        let before = crate::sem::parameter_resolution::inference_runs();
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
        use crate::sem::{MutSourcePathWithSpan, SchemaName};
        let before = crate::sem::parameter_resolution::inference_runs();
        let mut context = crate::test_helpers::schema_layer(
            std::path::Path::new("schema.sql"),
            &context().freeze(),
        );
        context.set_source_path_with_span(&crate::test_helpers::source_span());
        context.set_search_path(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
        let statements = parse_statements(
            "create table t (n int4 default 1); create view v as select n from t group by n having true window w as (order by n) order by n limit 1 offset 0;",
            Dialect::postgres(),
        );
        for statement in statements {
            let statement = statement.unwrap();
            context.set_statement_span(*statement.span());
            context = statement.analyze(context).unwrap().into_context();
        }
        assert_eq!(crate::sem::parameter_resolution::inference_runs(), before);
        assert!(context.parameter_constraints().result_column(0).is_none());
    }

    #[test]
    fn inference_preserves_unrelated_name_resolution_diagnostics() {
        let baseline = analyze("select missing_column as absent", Dialect::postgres()).unwrap();
        let expected = baseline.context().reported_problems();
        assert!(!expected.is_empty());
        let before = crate::sem::parameter_resolution::inference_runs();
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
        let mut context = context();
        context.reject_diagnostics_after(0);
        let error = parse_statements("select $1 + 1, lower($1)", Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .analyze(context)
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
    use crate::ast::Dialect;
    use crate::parse_statements;
    use crate::sem::{ParseContext, PgBuiltInType};
    use crate::test_helpers::TestParseContext;

    #[rstest::rstest]
    #[case::boolean("select $1 as flag where $1")]
    #[case::not("select not $1 as flag")]
    fn boolean_context_constrains_input(#[case] sql: &str) {
        let result = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .analyze(TestParseContext::default())
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
        let mut context = TestParseContext::default().with_dialect(Dialect::sqlite());
        context.setup_type(crate::sem::TypeReference::from_full_name(
            crate::sem::FullName::from_local_name("integer"),
        ));
        let result = parse_statements(sql, Dialect::sqlite())
            .next()
            .unwrap()
            .unwrap()
            .analyze(context)
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
