//! Mutable traversal of query-bearing common document syntax.
use crate::ast::document::{
    DocumentExpr, JsonAggregate, JsonArgument, JsonArray, JsonArrayAggregate, JsonArrayQuery,
    JsonBehavior, JsonColumn, JsonFunction, JsonKeyValue, JsonObject, JsonObjectAggregate,
    JsonOutput, JsonParse, JsonPath, JsonPredicate, JsonScalar, JsonSerialize, JsonTable,
    JsonValue, Merge, MergeWhen, MultiAssignment, Xml, XmlColumn, XmlSerialize, XmlTable,
};
use crate::ast::projection::ProjectionScope;
use crate::ast::traversal::{AstVisitorMut, WalkSqlMut};
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

    JsonOutput => type_name;
    JsonValue => raw_expr;
    JsonArgument => val;
    JsonBehavior => expr;
    JsonPath => string;
    JsonKeyValue => key, value;
    JsonFunction => context_item, pathspec, passing, output, on_empty, on_error;
    JsonPredicate => expr;
    JsonParse => expr, output;
    JsonScalar => expr, output;
    JsonSerialize => expr, output;
    JsonObject => exprs, output;
    JsonArray => exprs, output;
    JsonArrayQuery => query, output;
    JsonAggregate => output, agg_filter, agg_order, over;
    JsonObjectAggregate => constructor, arg;
    JsonArrayAggregate => constructor, arg;
    JsonTable => context_item, pathspec, passing, columns, on_error;
    JsonColumn => type_name, pathspec, columns, on_empty, on_error;
    Xml => named_args, args;
    XmlSerialize => expr, type_name;
    XmlTable => docexpr, rowexpr, namespaces, columns;
    XmlColumn => type_name, colexpr, coldefexpr;
    MultiAssignment => source;
    MergeWhen => condition, target_list, values;
}
impl WalkSqlMut for DocumentExpr {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        match self {
            Self::JsonFunction(node) => node.walk_sql_mut(visitor),
            Self::JsonPredicate(node) => node.walk_sql_mut(visitor),
            Self::JsonParse(node) => node.walk_sql_mut(visitor),
            Self::JsonScalar(node) => node.walk_sql_mut(visitor),
            Self::JsonSerialize(node) => node.walk_sql_mut(visitor),
            Self::JsonObject(node) => node.walk_sql_mut(visitor),
            Self::JsonArray(node) => node.walk_sql_mut(visitor),
            Self::JsonArrayQuery(node) => node.walk_sql_mut(visitor),
            Self::JsonObjectAggregate(node) => node.walk_sql_mut(visitor),
            Self::JsonArrayAggregate(node) => node.walk_sql_mut(visitor),
            Self::Xml(node) => node.walk_sql_mut(visitor),
            Self::XmlSerialize(node) => node.walk_sql_mut(visitor),
            Self::MultiAssignment(node) => node.walk_sql_mut(visitor),
        }
    }
}

impl WalkSqlMut for Merge {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        self.with_clause.walk_sql_mut(visitor);
        visitor.enter_projection(&mut ProjectionScope::new(
            &mut self.returning_list,
            std::slice::from_mut(&mut self.source_relation),
            true,
        ));
        self.join_condition.walk_sql_mut(visitor);
        self.merge_when_clauses.walk_sql_mut(visitor);
        self.returning_list.walk_sql_mut(visitor);
        self.source_relation.walk_sql_mut(visitor);
        visitor.leave_projection(&mut ProjectionScope::new(
            &mut self.returning_list,
            std::slice::from_mut(&mut self.source_relation),
            true,
        ));
    }
}
impl XmlTable {
    pub(in crate::ast) fn visible_alias(&self) -> Option<&str> {
        self.alias.as_ref().map(crate::ast::Alias::visible_name)
    }
}
impl JsonTable {
    pub(in crate::ast) fn visible_alias(&self) -> Option<&str> {
        self.alias.as_ref().map(crate::ast::Alias::visible_name)
    }
}
