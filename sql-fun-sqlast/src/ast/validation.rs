//! Walk every query-bearing edge when a statement becomes a nested CTE body.

use crate::ast::{
    Access, Assignment, BooleanExpr, Call, Case, CaseArm, Cast, Collate, Conflict, Cte, CteBody,
    Cycle, Delete, DerivedTable, Expr, Frame, FrameBound, FromGroup, FromItem, Grouping, InTable,
    IndexExpression, Indirection, Insert, Into, Join, NamedArgument, NullTest, OperatorExpr, Order,
    Query, QueryBody, Raise, Select, SetQuery, Slice, Subquery, SyntaxOption, SyntaxValue,
    TableFunction, TableFunctions, TableSample, Target, TruthTest, TypeName, TypedColumn, Update,
    Window, With,
};

/// Whether a subtree contains a clause forbidden inside a CTE body.
///
/// An enclosing CTE body is always nested below the main statement, so every
/// modifying WITH entry reached here violates PostgreSQL placement rules. The
/// body's own statement kind is checked separately by the mutation boundary.
/// SELECT INTO is likewise restricted to the top-level statement.
pub(super) trait ContainsInvalidCteClause {
    fn contains_invalid_cte_clause(&self) -> bool;
}

impl<T: ContainsInvalidCteClause> ContainsInvalidCteClause for Vec<T> {
    fn contains_invalid_cte_clause(&self) -> bool {
        self.iter()
            .any(ContainsInvalidCteClause::contains_invalid_cte_clause)
    }
}

impl<T: ContainsInvalidCteClause> ContainsInvalidCteClause for Option<T> {
    fn contains_invalid_cte_clause(&self) -> bool {
        self.as_ref()
            .is_some_and(ContainsInvalidCteClause::contains_invalid_cte_clause)
    }
}

impl<T: ContainsInvalidCteClause> ContainsInvalidCteClause for Box<T> {
    fn contains_invalid_cte_clause(&self) -> bool {
        self.as_ref().contains_invalid_cte_clause()
    }
}

// This traversal is intentionally about query-bearing fields, not name binding
// or SQL semantic validation. Keep the field lists alongside the enum dispatch
// so review can compare all recursive edges against the common AST definitions.
macro_rules! walk_fields {
    ($($type:ty => $($field:ident),+;)+) => {
        $(impl ContainsInvalidCteClause for $type {
            fn contains_invalid_cte_clause(&self) -> bool {
                crate::sem::with_stack(|| false $(|| self.$field.contains_invalid_cte_clause())+)
            }
        })+
    };
}

walk_fields! {
    Query => with, body, order, limit, offset;
    SetQuery => left, right;
    Select => into, distinct, targets, from, predicate, group, having, windows;
    Target => expression;
    Order => expression;
    Cte => body, cycle;
    Cycle => value, default;
    TypeName => modifiers;
    TypedColumn => type_name;
    DerivedTable => query;
    FromGroup => items;
    Join => left, right, condition;
    TableFunctions => calls, columns;
    TableFunction => expression, columns;
    TableSample => relation, arguments, repeatable;
    Window => partition, order, frame;
    Frame => start, end;
    Assignment => indirection, value;
    Insert => with, columns, source, conflict, returning;
    Update => with, assignments, from, predicate, returning;
    Delete => with, using, predicate, returning;
    Conflict => inference, inference_predicate, assignments, predicate, next;
    IndexExpression => expression, options;
    SyntaxOption => value;
    InTable => argument, arguments;
    OperatorExpr => left, right, escape;
    BooleanExpr => arguments;
    NullTest => argument;
    TruthTest => argument;
    Cast => argument, target;
    Collate => argument;
    Call => arguments, within_order, order, filter, over;
    Case => argument, arms, otherwise;
    CaseArm => condition, result;
    Subquery => query, test;
    Indirection => argument, items;
    Slice => lower, upper;
    NamedArgument => argument;
    Raise => message;
}

impl ContainsInvalidCteClause for With {
    fn contains_invalid_cte_clause(&self) -> bool {
        for entry in &self.entries {
            if entry.body.is_data_modifying() || entry.contains_invalid_cte_clause() {
                return true;
            }
        }
        false
    }
}

impl ContainsInvalidCteClause for CteBody {
    fn contains_invalid_cte_clause(&self) -> bool {
        match self {
            Self::Query(node) => node.contains_invalid_cte_clause(),
            Self::Insert(node) => node.contains_invalid_cte_clause(),
            Self::Update(node) => node.contains_invalid_cte_clause(),
            Self::Delete(node) => node.contains_invalid_cte_clause(),
            Self::Merge(node) => node.contains_invalid_cte_clause(),
        }
    }
}

impl ContainsInvalidCteClause for QueryBody {
    fn contains_invalid_cte_clause(&self) -> bool {
        match self {
            Self::Select(node) => node.contains_invalid_cte_clause(),
            Self::Values(rows) => rows.contains_invalid_cte_clause(),
            Self::Set(node) => node.contains_invalid_cte_clause(),
        }
    }
}

impl ContainsInvalidCteClause for Expr {
    fn contains_invalid_cte_clause(&self) -> bool {
        crate::sem::with_stack(|| self.walk_invalid_cte_clause())
    }
}

impl Expr {
    fn walk_invalid_cte_clause(&self) -> bool {
        match self {
            Self::Literal(_)
            | Self::Column(_)
            | Self::Star(_)
            | Self::Parameter(_)
            | Self::SqlValue(_)
            | Self::Default
            | Self::CurrentOf(_)
            | Self::MergeAction => false,
            Self::Operator(node) => node.contains_invalid_cte_clause(),
            Self::Boolean(node) => node.contains_invalid_cte_clause(),
            Self::NullTest(node) => node.contains_invalid_cte_clause(),
            Self::TruthTest(node) => node.contains_invalid_cte_clause(),
            Self::Cast(node) => node.contains_invalid_cte_clause(),
            Self::Collate(node) => node.contains_invalid_cte_clause(),
            Self::Call(node) => node.contains_invalid_cte_clause(),
            Self::Case(node) => node.contains_invalid_cte_clause(),
            Self::Subquery(node) => node.contains_invalid_cte_clause(),
            Self::Array(nodes)
            | Self::Row(nodes)
            | Self::List(nodes)
            | Self::Parenthesized(nodes)
            | Self::Grouping(nodes) => nodes.contains_invalid_cte_clause(),
            Self::Indirection(node) => node.contains_invalid_cte_clause(),
            Self::NamedArgument(node) => node.contains_invalid_cte_clause(),
            Self::Raise(node) => node.contains_invalid_cte_clause(),
            Self::InTable(node) => node.contains_invalid_cte_clause(),
            Self::Document(node) => node.contains_invalid_cte_clause(),
        }
    }
}

#[cfg(test)]
mod test_expression_placement {
    use crate::ast::{CteDefinition, Dialect};
    use crate::parse_statements;

    #[rstest::rstest]
    #[case::array(
        "SELECT ARRAY[(WITH changed AS (DELETE FROM items RETURNING id) SELECT id FROM changed)]"
    )]
    #[case::ordering(
        "SELECT 1 ORDER BY (WITH changed AS (DELETE FROM items RETURNING id) SELECT id FROM changed)"
    )]
    #[case::window(
        "SELECT sum(1) OVER (ORDER BY (WITH changed AS (DELETE FROM items RETURNING id) SELECT id FROM changed))"
    )]
    fn modifying_with_cannot_hide_in_expression_clauses(#[case] sql: &str) {
        let body = parse_statements(sql, Dialect::postgres())
            .next()
            .expect("statement")
            .expect("context-free AST");
        assert!(CteDefinition::new("catalog", &body).is_err());
    }
}

impl ContainsInvalidCteClause for FromItem {
    fn contains_invalid_cte_clause(&self) -> bool {
        crate::sem::with_stack(|| self.walk_invalid_cte_clause())
    }
}

impl FromItem {
    fn walk_invalid_cte_clause(&self) -> bool {
        match self {
            Self::Relation(_) => false,
            Self::Query(node) => node.contains_invalid_cte_clause(),
            Self::Join(node) => node.contains_invalid_cte_clause(),
            Self::Functions(node) => node.contains_invalid_cte_clause(),
            Self::Group(node) => node.contains_invalid_cte_clause(),
            Self::Sample(node) => node.contains_invalid_cte_clause(),
            Self::Xml(node) => node.contains_invalid_cte_clause(),
            Self::Json(node) => node.contains_invalid_cte_clause(),
        }
    }
}

impl ContainsInvalidCteClause for Grouping {
    fn contains_invalid_cte_clause(&self) -> bool {
        crate::sem::with_stack(|| match self {
            Self::Expression(node) => node.contains_invalid_cte_clause(),
            Self::Empty => false,
            Self::Rollup(nodes) | Self::Cube(nodes) | Self::Sets(nodes) | Self::Simple(nodes) => {
                nodes.contains_invalid_cte_clause()
            }
        })
    }
}

impl ContainsInvalidCteClause for Access {
    fn contains_invalid_cte_clause(&self) -> bool {
        match self {
            Self::Field(_) | Self::Star => false,
            Self::Index(node) => node.contains_invalid_cte_clause(),
            Self::Slice(node) => node.contains_invalid_cte_clause(),
        }
    }
}

impl ContainsInvalidCteClause for FrameBound {
    fn contains_invalid_cte_clause(&self) -> bool {
        match self {
            Self::Preceding(node) | Self::Following(node) => node.contains_invalid_cte_clause(),
            Self::UnboundedPreceding | Self::CurrentRow | Self::UnboundedFollowing => false,
        }
    }
}

impl ContainsInvalidCteClause for SyntaxValue {
    fn contains_invalid_cte_clause(&self) -> bool {
        match self {
            Self::Expression(node) => node.contains_invalid_cte_clause(),
            Self::Type(node) => node.contains_invalid_cte_clause(),
        }
    }
}

impl ContainsInvalidCteClause for Into {
    fn contains_invalid_cte_clause(&self) -> bool {
        true
    }
}
