//! Common AST operations for fixed projections of zipped function sources.

use crate::ast::{
    Call, CallKind, Cte, CteBody, DeparseError, Dialect, Expr, FromItem, Join, JoinKind, Literal,
    Name, OperatorExpr, OperatorKind, Order, Query, QueryBody, Relation, Statement, StatementNode,
    Subquery, SubqueryKind, TableFunction, TableFunctions, Target, TypedColumn, Window, With,
};

impl TableFunctions {
    pub(in crate::ast) fn has_invocations(&self, count: usize) -> bool {
        count != 0 && self.calls.len() == count
    }

    /// Retain a single invocation and its record definitions without relabeling
    /// columns through an alias-column list.
    pub(in crate::ast) fn isolated_invocation(&self, index: usize, alias: &str) -> Option<Self> {
        let mut call = self.calls.get(index)?.clone();
        if self.calls.len() == 1 && !self.columns.is_empty() {
            call.set_column_definitions(&self.columns);
        }
        Some(Self {
            calls: vec![call],
            alias: Some(crate::ast::Alias::projection(alias)),
            lateral: false,
            ordinality: false,
            rows_from: true,
            columns: Vec::new(),
        })
    }

    pub(in crate::ast) fn rendered_invocation(&self) -> Result<String, DeparseError> {
        let query = Query::projection(&[], &[FromItem::functions(self.clone())]);
        Statement::from_node(
            StatementNode::query(query),
            Dialect::postgres(),
            crate::StringSpan::default(),
        )
        .deparse_sql(Dialect::postgres())
    }
}

impl Cte {
    pub(in crate::ast) fn materialized_query(name: &str, query: &Query) -> Self {
        Self {
            name: name.to_owned(),
            columns: Vec::new(),
            materialized: Some(true),
            body: CteBody::query(query.clone()),
            search: None,
            cycle: None,
        }
    }
}

impl Join {
    pub(in crate::ast) fn full_using(left: &FromItem, right: &FromItem, column: &str) -> Self {
        Self {
            left: left.clone(),
            right: right.clone(),
            kind: JoinKind::full(),
            natural: false,
            condition: None,
            using: vec![column.to_owned()],
            using_alias: None,
            alias: None,
        }
    }
}

impl JoinKind {
    const fn full() -> Self {
        Self::Full
    }
}

impl Relation {
    pub(in crate::ast) fn local(name: &str) -> Self {
        Self {
            name: Name::local(name),
            alias: None,
            inherit: true,
            persistence: String::new(),
            indexed: None,
            not_indexed: false,
        }
    }
}

impl Query {
    pub(in crate::ast) fn order_materialized_sources(&mut self, sources: &[Cte], ordinal: &str) {
        self.with = With::nonrecursive(sources);
        self.order.push(Order::ascending_column(ordinal));
    }

    /// Consume the preceding materialized source before invoking this source,
    /// even when a hash join requests inputs in the opposite order.
    pub(in crate::ast) fn after_materialized_source(&mut self, prior: &str) {
        let count = Target::projection(&Expr::projection_call(&Call::count_rows()), "count");
        let query = Self::projection(&[count], &[FromItem::relation(Relation::local(prior))]);
        self.body
            .set_projection_predicate(&Expr::nonnegative_subquery(&query));
    }
}

impl QueryBody {
    fn set_projection_predicate(&mut self, predicate: &Expr) {
        match self {
            Self::Select(select) => select.set_projection_predicate(predicate),
            _ => unreachable!("projection query"),
        }
    }
}

impl crate::ast::Select {
    fn set_projection_predicate(&mut self, predicate: &Expr) {
        self.predicate = Some(predicate.clone());
    }
}

impl Order {
    fn ascending_column(name: &str) -> Self {
        Self {
            expression: Expr::projection_column(&Name::local(name)),
            descending: Some(false),
            nulls_first: None,
            using: None,
        }
    }
}

impl Expr {
    fn nonnegative_subquery(query: &Query) -> Self {
        Self::Operator(Box::new(OperatorExpr::nonnegative_subquery(query)))
    }
    fn scalar_subquery(query: &Query) -> Self {
        Self::Subquery(Box::new(Subquery::from_scalar_query(query)))
    }
    fn zero() -> Self {
        Self::Literal(Literal::zero())
    }
}

impl OperatorExpr {
    fn nonnegative_subquery(query: &Query) -> Self {
        Self {
            name: Name::from_parts(&["pg_catalog".to_owned(), ">=".to_owned()]),
            kind: OperatorKind::ordinary(),
            left: Some(Expr::scalar_subquery(query)),
            right: Some(Expr::zero()),
            escape: None,
        }
    }
}

impl Subquery {
    fn from_scalar_query(query: &Query) -> Self {
        Self {
            query: query.clone(),
            kind: SubqueryKind::scalar(),
            test: None,
            operator: Name::local(""),
        }
    }
}
impl Literal {
    const fn zero() -> Self {
        Self::Integer(0)
    }
}

impl Call {
    pub(in crate::ast) fn row_number() -> Self {
        let mut call = Self::system_rows_function("row_number");
        call.over = Some(Window::unpartitioned());
        call
    }
    fn count_rows() -> Self {
        let mut call = Self::system_rows_function("count");
        call.star = true;
        call
    }
    fn system_rows_function(name: &str) -> Self {
        Self {
            sql_syntax: false,
            kind: CallKind::ordinary(),
            name: Name::from_parts(&["pg_catalog".to_owned(), name.to_owned()]),
            arguments: Vec::new(),
            argument_locations: Vec::new(),
            star: false,
            distinct: false,
            variadic: false,
            within_group: false,
            within_order: Vec::new(),
            order: Vec::new(),
            filter: None,
            over: None,
            location: None,
        }
    }
}

impl Window {
    fn unpartitioned() -> Self {
        Self {
            reference_only: false,
            name: None,
            reference: None,
            partition: Vec::new(),
            order: Vec::new(),
            frame: None,
        }
    }
}

impl TableFunction {
    fn set_column_definitions(&mut self, columns: &[TypedColumn]) {
        self.columns = columns.to_vec();
    }
}

impl With {
    fn nonrecursive(entries: &[Cte]) -> Self {
        Self {
            recursive: false,
            entries: entries.to_vec(),
        }
    }
}
