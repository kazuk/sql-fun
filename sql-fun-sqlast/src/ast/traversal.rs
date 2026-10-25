//! Mutable traversal of common SQL query syntax, independent of analysis passes.
use crate::ast::projection::ProjectionScope;
use crate::ast::{
    Access, Assignment, BooleanExpr, Call, Case, CaseArm, Cast, Collate, Conflict, Cte, CteBody,
    Cycle, Delete, DerivedTable, Expr, Frame, FrameBound, FromGroup, FromItem, Grouping, InTable,
    IndexExpression, Indirection, Insert, Into, Join, NamedArgument, NullTest, OperatorExpr, Order,
    Query, QueryBody, Raise, Select, SetQuery, Slice, Subquery, SyntaxOption, SyntaxValue,
    TableFunction, TableFunctions, TableSample, Target, TruthTest, TypeName, TypedColumn, Update,
    Window, With,
};

use crate::ast::StatementNode;

pub(in crate::ast) trait AstVisitorMut {
    fn enter_query(&mut self, _query: &mut Query) {}
    fn leave_query(&mut self, _query: &mut Query) {}
    fn expression(&mut self, _expression: &mut Expr) {}
    fn source(&mut self, _source: &mut FromItem) {}
    fn target(&mut self, _target: &mut Target) {}
    fn enter_projection(&mut self, _scope: &mut ProjectionScope<'_>) {}
    fn leave_projection(&mut self, _scope: &mut ProjectionScope<'_>) {}
}
/// Visits each projection collection, including query-bearing expression edges.
pub(in crate::ast) trait WalkSqlMut {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut);
}
impl<T: WalkSqlMut> WalkSqlMut for Vec<T> {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        for item in self {
            item.walk_sql_mut(visitor);
        }
    }
}
impl<T: WalkSqlMut> WalkSqlMut for Option<T> {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        if let Some(item) = self {
            item.walk_sql_mut(visitor);
        }
    }
}
impl<T: WalkSqlMut> WalkSqlMut for Box<T> {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        self.as_mut().walk_sql_mut(visitor);
    }
}
macro_rules! walk_fields {
    ($($type:ty => $($field:ident),+;)+) => {
        $(impl WalkSqlMut for $type {
            fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
                crate::stack::with_stack(|| { $(self.$field.walk_sql_mut(visitor);)+ });
            }
        })+
    };
}
walk_fields! {


    SetQuery => left, right;

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
    With => entries;
}

impl WalkSqlMut for Select {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        visitor.enter_projection(&mut ProjectionScope::new(
            &mut self.targets,
            &mut self.from,
            false,
        ));
        self.into.walk_sql_mut(visitor);
        self.distinct.walk_sql_mut(visitor);
        self.targets.walk_sql_mut(visitor);
        self.from.walk_sql_mut(visitor);
        self.predicate.walk_sql_mut(visitor);
        self.group.walk_sql_mut(visitor);
        self.having.walk_sql_mut(visitor);
        self.windows.walk_sql_mut(visitor);
        visitor.leave_projection(&mut ProjectionScope::new(
            &mut self.targets,
            &mut self.from,
            false,
        ));
    }
}
impl WalkSqlMut for Insert {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        self.with.walk_sql_mut(visitor);
        visitor.enter_projection(&mut ProjectionScope::new(
            &mut self.returning,
            &mut [],
            true,
        ));
        self.columns.walk_sql_mut(visitor);
        self.source.walk_sql_mut(visitor);
        self.conflict.walk_sql_mut(visitor);
        self.returning.walk_sql_mut(visitor);
        visitor.leave_projection(&mut ProjectionScope::new(
            &mut self.returning,
            &mut [],
            true,
        ));
    }
}
impl WalkSqlMut for Update {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        self.with.walk_sql_mut(visitor);
        visitor.enter_projection(&mut ProjectionScope::new(
            &mut self.returning,
            &mut self.from,
            true,
        ));
        self.assignments.walk_sql_mut(visitor);
        self.predicate.walk_sql_mut(visitor);
        self.returning.walk_sql_mut(visitor);
        self.from.walk_sql_mut(visitor);
        visitor.leave_projection(&mut ProjectionScope::new(
            &mut self.returning,
            &mut self.from,
            true,
        ));
    }
}
impl WalkSqlMut for Delete {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        self.with.walk_sql_mut(visitor);
        visitor.enter_projection(&mut ProjectionScope::new(
            &mut self.returning,
            &mut self.using,
            true,
        ));
        self.predicate.walk_sql_mut(visitor);
        self.returning.walk_sql_mut(visitor);
        self.using.walk_sql_mut(visitor);
        visitor.leave_projection(&mut ProjectionScope::new(
            &mut self.returning,
            &mut self.using,
            true,
        ));
    }
}
impl WalkSqlMut for StatementNode {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        match self {
            Self::Query(node) => node.walk_sql_mut(visitor),
            Self::Insert(node) => node.walk_sql_mut(visitor),
            Self::Update(node) => node.walk_sql_mut(visitor),
            Self::Delete(node) => node.walk_sql_mut(visitor),
            Self::Merge(node) => node.walk_sql_mut(visitor),
            _ => {}
        }
    }
}

impl WalkSqlMut for CteBody {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        match self {
            Self::Query(node) => node.walk_sql_mut(visitor),
            Self::Insert(node) => node.walk_sql_mut(visitor),
            Self::Update(node) => node.walk_sql_mut(visitor),
            Self::Delete(node) => node.walk_sql_mut(visitor),
            Self::Merge(node) => node.walk_sql_mut(visitor),
        }
    }
}

impl WalkSqlMut for QueryBody {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        match self {
            Self::Select(node) => node.walk_sql_mut(visitor),
            Self::Values(rows) => rows.walk_sql_mut(visitor),
            Self::Set(node) => node.walk_sql_mut(visitor),
        }
    }
}

impl WalkSqlMut for Expr {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        visitor.expression(self);
        crate::stack::with_stack(|| match self {
            Self::Literal(_)
            | Self::Column(_)
            | Self::Star(_)
            | Self::Parameter(_)
            | Self::SqlValue(_)
            | Self::Default
            | Self::CurrentOf(_)
            | Self::MergeAction => {}
            Self::Operator(node) => node.walk_sql_mut(visitor),
            Self::Boolean(node) => node.walk_sql_mut(visitor),
            Self::NullTest(node) => node.walk_sql_mut(visitor),
            Self::TruthTest(node) => node.walk_sql_mut(visitor),
            Self::Cast(node) => node.walk_sql_mut(visitor),
            Self::Collate(node) => node.walk_sql_mut(visitor),
            Self::Call(node) => node.walk_sql_mut(visitor),
            Self::Case(node) => node.walk_sql_mut(visitor),
            Self::Subquery(node) => node.walk_sql_mut(visitor),
            Self::Array(nodes)
            | Self::Row(nodes)
            | Self::List(nodes)
            | Self::Parenthesized(nodes)
            | Self::Grouping(nodes) => nodes.walk_sql_mut(visitor),
            Self::Indirection(node) => node.walk_sql_mut(visitor),
            Self::NamedArgument(node) => node.walk_sql_mut(visitor),
            Self::Raise(node) => node.walk_sql_mut(visitor),
            Self::InTable(node) => node.walk_sql_mut(visitor),
            Self::Document(node) => node.walk_sql_mut(visitor),
        })
    }
}

impl WalkSqlMut for FromItem {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        visitor.source(self);
        crate::stack::with_stack(|| match self {
            Self::Relation(_) => {}
            Self::Query(node) => node.walk_sql_mut(visitor),
            Self::Join(node) => node.walk_sql_mut(visitor),
            Self::Functions(node) => node.walk_sql_mut(visitor),
            Self::Group(node) => node.walk_sql_mut(visitor),
            Self::Sample(node) => node.walk_sql_mut(visitor),
            Self::Xml(node) => node.walk_sql_mut(visitor),
            Self::Json(node) => node.walk_sql_mut(visitor),
        })
    }
}

impl WalkSqlMut for Grouping {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        crate::stack::with_stack(|| match self {
            Self::Expression(node) => node.walk_sql_mut(visitor),
            Self::Empty => {}
            Self::Rollup(nodes) | Self::Cube(nodes) | Self::Sets(nodes) | Self::Simple(nodes) => {
                nodes.walk_sql_mut(visitor)
            }
        })
    }
}

impl WalkSqlMut for Access {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        match self {
            Self::Field(_) | Self::Star => {}
            Self::Index(node) => node.walk_sql_mut(visitor),
            Self::Slice(node) => node.walk_sql_mut(visitor),
        }
    }
}

impl WalkSqlMut for FrameBound {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        match self {
            Self::Preceding(node) | Self::Following(node) => node.walk_sql_mut(visitor),
            Self::UnboundedPreceding | Self::CurrentRow | Self::UnboundedFollowing => {}
        }
    }
}

impl WalkSqlMut for SyntaxValue {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        match self {
            Self::Expression(node) => node.walk_sql_mut(visitor),
            Self::Type(node) => node.walk_sql_mut(visitor),
        }
    }
}

impl WalkSqlMut for Into {
    fn walk_sql_mut(&mut self, _visitor: &mut impl AstVisitorMut) {}
}

impl WalkSqlMut for Target {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        visitor.target(self);
        self.expression.walk_sql_mut(visitor);
    }
}
impl WalkSqlMut for Query {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        visitor.enter_query(self);
        self.with.walk_sql_mut(visitor);
        self.body.walk_sql_mut(visitor);
        self.order.walk_sql_mut(visitor);
        self.limit.walk_sql_mut(visitor);
        self.offset.walk_sql_mut(visitor);
        visitor.leave_query(self);
    }
}
