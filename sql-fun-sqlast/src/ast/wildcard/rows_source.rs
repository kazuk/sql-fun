//! Freeze a combined function source without changing its zip or row interface.

use crate::ast::wildcard::WildcardRewriteError;
use crate::ast::{
    Alias, Call, CallKind, Cte, CteBody, DerivedTable, Dialect, Expr, FromItem, Join, JoinKind,
    Literal, Name, OperatorExpr, OperatorKind, Order, Query, QueryBody, Relation, Select,
    Statement, StatementNode, Subquery, SubqueryKind, TableFunctions, Target, Window, With,
};
use crate::sem::WildcardRowsSource;

/// Keep each invocation once, number its rows, and zip the fixed outputs by row.
/// All helper fields stay inside the returned derived source.
pub(super) fn freeze(
    resolved: &WildcardRowsSource,
    source: TableFunctions,
) -> Result<FromItem, WildcardRewriteError> {
    if source.calls.len() != resolved.functions().len() || source.calls.is_empty() {
        return Err(WildcardRewriteError::invalid(
            "ROWS FROM output layout does not match its original function invocations",
        ));
    }
    let mut names = PrivateNames::new(&source, resolved)?;
    let ordinal = names.next();
    let mut zipped = None;
    let mut entries = Vec::new();
    let mut previous: Option<String> = None;
    let mut targets = Vec::new();
    let mut output_index = 0;
    for (index, layout) in resolved.functions().iter().enumerate() {
        let alias = names.next();
        let input_alias = if let [column] = layout.columns() {
            // A scalar invocation's default output name follows its table alias.
            // Using its known output name also works for one-column composites,
            // without an alias-column list that could relabel future columns.
            column.clone()
        } else {
            names.next()
        };
        let mut call = source.calls[index].clone();
        if source.calls.len() == 1 && !source.columns.is_empty() {
            call.columns = source.columns.clone();
        }
        let input = TableFunctions {
            calls: vec![call],
            alias: Some(Alias {
                name: input_alias.clone(),
                columns: Vec::new(),
            }),
            lateral: false,
            ordinality: false,
            rows_from: true,
            columns: Vec::new(),
        };
        let mut fields = Vec::new();
        for column in layout.columns() {
            fields.push(target(reference(&input_alias, column), column));
            let output = resolved.output_names().get(output_index).ok_or_else(|| {
                WildcardRewriteError::invalid("ROWS FROM has fewer output labels than fields")
            })?;
            targets.push(target(reference(&alias, column), output));
            output_index += 1;
        }
        fields.push(target(Call::rows_ordinal(), &ordinal));
        let mut fixed = query(fields, vec![FromItem::functions(input)]);
        if let Some(prior) = &previous {
            // FunctionScan evaluates complete invocations in source order. A hash
            // FULL JOIN may request its right input first, so force the previous
            // materialized source to completion before invoking this function.
            fixed.rows_after(prior);
        }
        entries.push(Cte {
            name: alias.clone(),
            columns: Vec::new(),
            materialized: Some(true),
            body: CteBody::query(fixed),
            search: None,
            cycle: None,
        });
        previous = Some(alias.clone());
        let individual = relation(&alias);
        zipped = Some(if let Some(left) = zipped {
            FromItem::join(Join {
                left,
                right: individual,
                kind: JoinKind::rows_full(),
                natural: false,
                condition: None,
                using: vec![ordinal.clone()],
                using_alias: None,
                alias: None,
            })
        } else {
            individual
        });
    }
    if resolved.has_ordinality() {
        let output = resolved.output_names().get(output_index).ok_or_else(|| {
            WildcardRewriteError::invalid("ROWS FROM has no output label for its ordinality")
        })?;
        targets.push(target(
            Expr::projection_column(Name::local(&ordinal)),
            output,
        ));
        output_index += 1;
    }
    if output_index != resolved.output_names().len() {
        return Err(WildcardRewriteError::invalid(
            "ROWS FROM has more output labels than resolved fields",
        ));
    }
    let mut query = query(targets, vec![zipped.expect("nonempty calls")]);
    query.with.entries = entries;
    query.order.push(Order {
        expression: Expr::projection_column(Name::local(&ordinal)),
        descending: Some(false),
        nulls_first: None,
        using: None,
    });
    Ok(derived(query, resolved.alias()))
}

#[cfg(test)]
mod tests;

fn target(expression: Expr, alias: &str) -> Target {
    Target {
        expression,
        alias: Some(alias.to_owned()),
        location: None,
    }
}

fn reference(alias: &str, column: &str) -> Expr {
    Expr::projection_column(Name::from_parts(&[alias.to_owned(), column.to_owned()]))
}

fn query(targets: Vec<Target>, from: Vec<FromItem>) -> Query {
    Query {
        with: With::default(),
        body: QueryBody::projection_select(Select {
            into: None,
            distinct: None,
            targets,
            from,
            predicate: None,
            group: Vec::new(),
            group_distinct: false,
            having: None,
            windows: Vec::new(),
        }),
        order: Vec::new(),
        limit: None,
        offset: None,
        with_ties: false,
        locks: Vec::new(),
    }
}

fn derived(query: Query, alias: &str) -> FromItem {
    FromItem::query(DerivedTable {
        query,
        alias: Some(Alias {
            name: alias.to_owned(),
            columns: Vec::new(),
        }),
        lateral: true,
    })
}

fn relation(name: &str) -> FromItem {
    FromItem::relation(Relation {
        name: Name::local(name),
        alias: None,
        inherit: true,
        persistence: String::new(),
        indexed: None,
        not_indexed: false,
    })
}

impl Query {
    fn rows_after(&mut self, prior: &str) {
        let mut count = Call::rows_function("count");
        count.star = true;
        count.over = None;
        let count = query(
            vec![target(Expr::projection_call(count), "count")],
            vec![relation(prior)],
        );
        let gate = Expr::rows_gate(count);
        self.body.rows_predicate(gate);
    }
}

impl QueryBody {
    fn rows_predicate(&mut self, predicate: Expr) {
        let Self::Select(select) = self else {
            unreachable!("projection query")
        };
        select.predicate = Some(predicate);
    }
}

impl Expr {
    fn rows_gate(query: Query) -> Self {
        Self::Operator(Box::new(OperatorExpr {
            name: Name::from_parts(&["pg_catalog".to_owned(), ">=".to_owned()]),
            kind: OperatorKind::ordinary(),
            left: Some(Self::Subquery(Box::new(Subquery {
                query,
                kind: SubqueryKind::rows_scalar(),
                test: None,
                operator: Name::local(""),
            }))),
            right: Some(Self::Literal(Literal::rows_zero())),
            escape: None,
        }))
    }
}

impl Literal {
    const fn rows_zero() -> Self {
        Self::Integer(0)
    }
}
impl SubqueryKind {
    const fn rows_scalar() -> Self {
        Self::Scalar
    }
}

impl JoinKind {
    const fn rows_full() -> Self {
        Self::Full
    }
}

impl Call {
    fn rows_ordinal() -> Expr {
        Expr::projection_call(Self::rows_function("row_number"))
    }

    fn rows_function(name: &str) -> Self {
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
            over: Some(Window {
                reference_only: false,
                name: None,
                reference: None,
                partition: Vec::new(),
                order: Vec::new(),
                frame: None,
            }),
            location: None,
        }
    }
}

/// Generated names are ASCII, so literal containment conservatively excludes
/// every spelling appearing in the original deparsed invocation or output names.
/// This only allocates private names; it never recovers locations or SQL facts.
struct PrivateNames {
    reserved: String,
    next: usize,
}

impl PrivateNames {
    fn new(
        source: &TableFunctions,
        resolved: &WildcardRowsSource,
    ) -> Result<Self, WildcardRewriteError> {
        let statement = Statement::from_node(
            StatementNode::query(query(Vec::new(), vec![FromItem::functions(source.clone())])),
            Dialect::postgres(),
            crate::StringSpan::default(),
        );
        let mut reserved = statement
            .deparse_sql(Dialect::postgres())
            .map_err(|error| WildcardRewriteError::invalid(&error.to_string()))?;
        for function in resolved.functions() {
            for column in function.columns() {
                reserved.push('\n');
                reserved.push_str(column);
            }
        }
        for column in resolved.output_names() {
            reserved.push('\n');
            reserved.push_str(column);
        }
        Ok(Self { reserved, next: 0 })
    }

    fn next(&mut self) -> String {
        loop {
            let candidate = format!("__sql_fun_rows_{}", self.next);
            self.next += 1;
            if !self.reserved.contains(&candidate) {
                self.reserved.push('\n');
                self.reserved.push_str(&candidate);
                return candidate;
            }
        }
    }
}
