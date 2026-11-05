//! Query-bearing document and MERGE edges for CTE placement validation.

use crate::ast::{
    document::{
        DocumentExpr, JsonAggregate, JsonArgument, JsonArray, JsonArrayAggregate, JsonArrayQuery,
        JsonBehavior, JsonColumn, JsonFunction, JsonKeyValue, JsonObject, JsonObjectAggregate,
        JsonOutput, JsonParse, JsonPath, JsonPredicate, JsonScalar, JsonSerialize, JsonTable,
        JsonValue, Merge, MergeWhen, MultiAssignment, Xml, XmlColumn, XmlSerialize, XmlTable,
    },
    validation::ContainsInvalidCteClause,
};

macro_rules! walk_fields {
    ($($type:ty => $($field:ident),+;)+) => {
        $(impl ContainsInvalidCteClause for $type {
            fn contains_invalid_cte_clause(&self) -> bool {
                crate::stack::with_stack(|| false $(|| self.$field.contains_invalid_cte_clause())+)
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
    Merge => source_relation, join_condition, merge_when_clauses, returning_list, with_clause;
    MergeWhen => condition, target_list, values;
}

impl ContainsInvalidCteClause for DocumentExpr {
    fn contains_invalid_cte_clause(&self) -> bool {
        match self {
            Self::JsonFunction(node) => node.contains_invalid_cte_clause(),
            Self::JsonPredicate(node) => node.contains_invalid_cte_clause(),
            Self::JsonParse(node) => node.contains_invalid_cte_clause(),
            Self::JsonScalar(node) => node.contains_invalid_cte_clause(),
            Self::JsonSerialize(node) => node.contains_invalid_cte_clause(),
            Self::JsonObject(node) => node.contains_invalid_cte_clause(),
            Self::JsonArray(node) => node.contains_invalid_cte_clause(),
            Self::JsonArrayQuery(node) => node.contains_invalid_cte_clause(),
            Self::JsonObjectAggregate(node) => node.contains_invalid_cte_clause(),
            Self::JsonArrayAggregate(node) => node.contains_invalid_cte_clause(),
            Self::Xml(node) => node.contains_invalid_cte_clause(),
            Self::XmlSerialize(node) => node.contains_invalid_cte_clause(),
            Self::MultiAssignment(node) => node.contains_invalid_cte_clause(),
        }
    }
}

#[cfg(test)]
mod test_document_placement {
    use crate::{
        ast::{CteDefinition, Dialect},
        parse_statements,
    };

    #[rstest::rstest]
    #[case::json_array(
        "SELECT JSON_ARRAY(WITH changed AS (DELETE FROM items RETURNING id) SELECT id FROM changed)"
    )]
    #[case::json_scalar(
        "SELECT JSON_SCALAR((WITH changed AS (DELETE FROM items RETURNING id) SELECT id FROM changed))"
    )]
    #[case::xml_parse(
        "SELECT XMLPARSE(DOCUMENT (WITH changed AS (DELETE FROM items RETURNING id) SELECT id FROM changed))"
    )]
    fn document_expressions_preserve_cte_placement_checks(#[case] sql: &str) {
        let body = parse_statements(sql, Dialect::postgres())
            .next()
            .expect("statement")
            .expect("context-free AST");
        assert!(CteDefinition::new("catalog", &body).is_err());
    }

    #[test]
    fn ordinary_with_remains_legal_inside_document_query() {
        let body = parse_statements(
            "SELECT JSON_ARRAY(WITH ordinary AS (SELECT 1 AS id) SELECT id FROM ordinary)",
            Dialect::postgres(),
        )
        .next()
        .expect("statement")
        .expect("context-free AST");
        assert!(CteDefinition::new("catalog", &body).is_ok());
    }
}
