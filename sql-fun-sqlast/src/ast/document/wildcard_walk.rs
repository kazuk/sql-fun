//! Query-bearing document edges for wildcard rewriting.
use crate::ast::document::{
    DocumentExpr, JsonAggregate, JsonArgument, JsonArray, JsonArrayAggregate, JsonArrayQuery,
    JsonBehavior, JsonColumn, JsonFunction, JsonKeyValue, JsonObject, JsonObjectAggregate,
    JsonOutput, JsonParse, JsonPath, JsonPredicate, JsonScalar, JsonSerialize, JsonTable,
    JsonValue, Merge, MergeWhen, MultiAssignment, Xml, XmlColumn, XmlSerialize, XmlTable,
};
use crate::ast::wildcard::VisitTargets;
use crate::ast::{FromItem, Target};
macro_rules! walk_fields {
    ($($type:ty => $($field:ident),+;)+) => {
        $(impl VisitTargets for $type {
            fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
                crate::sem::with_stack(|| { $(self.$field.visit_targets(visitor);)+ });
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
impl VisitTargets for DocumentExpr {
    fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
        match self {
            Self::JsonFunction(node) => node.visit_targets(visitor),
            Self::JsonPredicate(node) => node.visit_targets(visitor),
            Self::JsonParse(node) => node.visit_targets(visitor),
            Self::JsonScalar(node) => node.visit_targets(visitor),
            Self::JsonSerialize(node) => node.visit_targets(visitor),
            Self::JsonObject(node) => node.visit_targets(visitor),
            Self::JsonArray(node) => node.visit_targets(visitor),
            Self::JsonArrayQuery(node) => node.visit_targets(visitor),
            Self::JsonObjectAggregate(node) => node.visit_targets(visitor),
            Self::JsonArrayAggregate(node) => node.visit_targets(visitor),
            Self::Xml(node) => node.visit_targets(visitor),
            Self::XmlSerialize(node) => node.visit_targets(visitor),
            Self::MultiAssignment(node) => node.visit_targets(visitor),
        }
    }
}

impl VisitTargets for Merge {
    fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
        self.source_relation.visit_targets(visitor);
        self.join_condition.visit_targets(visitor);
        self.merge_when_clauses.visit_targets(visitor);
        self.returning_list.visit_targets(visitor);
        visitor(&mut self.returning_list, true, &mut []);
        self.with_clause.visit_targets(visitor);
    }
}
