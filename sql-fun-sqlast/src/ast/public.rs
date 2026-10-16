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

    /// Analyze this common statement directly using the supplied context.
    ///
    /// No SQL reparsing or catalog-file discovery occurs. This uses the same
    /// resolver and diagnostic reporting as the normal semantic-analysis path.
    ///
    /// # Errors
    ///
    /// Returns the resolver or diagnostic reporting error.
    pub fn analyze<TContext: crate::sem::ParseContext>(
        &self,
        context: TContext,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError> {
        self.node.resolve_and_apply(context, self.dialect)
    }

    /// Inspect an existing WITH clause without exposing mutable node storage.
    pub fn with_clause(&self) -> Option<WithClause<'_>> {
        self.node
            .with_node()
            .and_then(|node| WithClause::existing(node, self.dialect))
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
        let mut context = TestParseContext::default();
        context.setup_type(PgBuiltInType::int4());
        let analyzed = base.analyze(context).unwrap();
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let column = query.result_columns().get_column_at(0).unwrap();
        assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
        assert_eq!(column.is_not_null(), &Some(true));
    }
}

/// Parser-independent statement category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatementKind {
    /// SELECT, VALUES, or a set operation.
    Query,
    /// INSERT.
    Insert,
    /// UPDATE.
    Update,
    /// DELETE.
    Delete,
    /// MERGE as a CTE body.
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
            Self::View(_) => StatementKind::view(),
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

    pub(super) const fn with_node(&self) -> Option<&With> {
        match self {
            Self::Query(node) => Some(&node.with),
            Self::Insert(node) => Some(&node.with),
            Self::Update(node) => Some(&node.with),
            Self::Delete(node) => Some(&node.with),
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
        FunctionParam, OperatorDefinition, OperatorName, OverloadVariant, PgBuiltInType,
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
            .analyze(context())
    }

    #[rstest::rstest]
    #[case::postgres("select $7 is null", Dialect::postgres())]
    #[cfg_attr(
        feature = "sqlite",
        case::sqlite("select ?7 is null", Dialect::sqlite())
    )]
    fn null_test_infers_any(#[case] sql: &str, #[case] dialect: Dialect) {
        let result = analyze(sql, dialect).unwrap();
        let inputs = result.parameter_constraints();
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
    fn conflicting_uses_return_error(#[case] sql: &str) {
        let error = analyze(sql, Dialect::postgres())
            .err()
            .expect("conflicting SQL inputs");
        assert_matches::assert_matches!(error, crate::sem::AnalysisError::ParameterConstraint(_));
        assert!(error.to_string().contains("$1"));
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
        let error = parse_statements("select $1 + 1", Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .analyze(context)
            .err()
            .expect("ambiguous overloads must fail");
        assert_matches::assert_matches!(error, crate::sem::AnalysisError::ParameterConstraint(_));
    }

    #[test]
    fn distinct_inputs_and_gaps_remain_independent() {
        let result = analyze(
            "select $7 + 1 as n, $2 is null as missing, $7 is null as repeated",
            Dialect::postgres(),
        )
        .unwrap();
        let inputs = result.parameter_constraints();
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
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
                .is_any()
        );
        assert_eq!(
            result
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
}

#[cfg(test)]
mod test_parameter_casts {
    use crate::ast::Dialect;
    use crate::parse_statements;
    use crate::sem::PgBuiltInType;
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
        let mut context = TestParseContext::default();
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
