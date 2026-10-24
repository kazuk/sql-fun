use super::{PrivateNames, freeze, query};
use crate::ast::{Dialect, FromItem, QueryBody, Statement, StatementNode, TableFunctions};
use crate::sem::{AliasName, ColumnName, WildcardRowsFunction, WildcardRowsSource};

fn invocation(sql: &str) -> TableFunctions {
    let statement = crate::parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap();
    let StatementNode::Query(query) = statement.node else {
        panic!("query")
    };
    let QueryBody::Select(mut select) = query.body else {
        panic!("select")
    };
    let FromItem::Functions(source) = select.from.remove(0) else {
        panic!("function")
    };
    source
}

fn layout(functions: &[&[&str]], outputs: &[&str], ordinality: bool) -> WildcardRowsSource {
    let functions = functions
        .iter()
        .map(|columns| {
            let columns = columns
                .iter()
                .map(|column| ColumnName::from(*column))
                .collect::<Vec<_>>();
            WildcardRowsFunction::from_columns(&columns)
        })
        .collect::<Vec<_>>();
    let outputs = outputs
        .iter()
        .map(|column| ColumnName::from(*column))
        .collect::<Vec<_>>();
    WildcardRowsSource::new(&AliasName::from("r"), &functions, &outputs, ordinality)
}

fn rendered(source: FromItem) -> String {
    Statement::from_node(
        StatementNode::query(query(Vec::new(), vec![source])),
        Dialect::postgres(),
        crate::StringSpan::default(),
    )
    .deparse_sql(Dialect::postgres())
    .unwrap()
}

#[rstest::rstest]
#[case::plain(false)]
#[case::ordinality(true)]
fn zips_fixed_function_projections_without_exposing_helper_columns(#[case] ordinality: bool) {
    let suffix = if ordinality { " WITH ORDINALITY" } else { "" };
    let source = invocation(&format!(
        "SELECT * FROM ROWS FROM (generate_series(1,2),generate_series(10,13)){suffix} AS r"
    ));
    let outputs: &[&str] = if ordinality {
        &["id", "id", "position"]
    } else {
        &["id", "id"]
    };
    let resolved = layout(
        &[&["generate_series"], &["generate_series"]],
        outputs,
        ordinality,
    );
    let frozen = freeze(&resolved, source).unwrap();
    let FromItem::Query(derived) = &frozen else {
        panic!("fixed source")
    };
    assert_eq!(derived.alias.as_ref().unwrap().name, "r");
    let QueryBody::Select(select) = &derived.query.body else {
        panic!("fixed projection")
    };
    assert_eq!(select.targets.len(), outputs.len());
    for (target, output) in select.targets.iter().zip(outputs) {
        assert_eq!(target.alias.as_deref(), Some(*output));
    }
    let sql = rendered(frozen);
    ::pg_query::parse(&sql).unwrap();
    assert_eq!(
        sql.matches(r#""generate_series"(1, 2)"#).count(),
        1,
        "{sql}"
    );
    assert_eq!(
        sql.matches(r#""generate_series"(10, 13)"#).count(),
        1,
        "{sql}"
    );
    assert_eq!(sql.matches(r#""row_number"()"#).count(), 2, "{sql}");
    assert!(sql.contains("FULL JOIN"), "{sql}");
    assert_eq!(sql.matches("AS MATERIALIZED").count(), 2, "{sql}");
    assert_eq!(sql.matches(r#""count"(*)"#).count(), 1, "{sql}");
    assert!(sql.contains(r#"OPERATOR("pg_catalog".>=)"#), "{sql}");
    assert!(sql.contains("ORDER BY"), "{sql}");
}

#[test]
fn retains_per_function_record_definitions() {
    let source = invocation(
        r#"SELECT * FROM ROWS FROM (json_to_recordset('[{"id":1}]') AS (id int4),generate_series(1,2)) AS r"#,
    );
    let resolved = layout(&[&["id"], &["generate_series"]], &["id", "id"], false);
    let sql = rendered(freeze(&resolved, source).unwrap());
    ::pg_query::parse(&sql).unwrap();
    assert!(sql.contains(r#"AS ("id" "int4")"#), "{sql}");
}

#[test]
fn private_names_do_not_shadow_correlated_bindings_or_output_columns() {
    let source =
        invocation("SELECT * FROM ROWS FROM (generate_series(1,__sql_fun_rows_0.id)) AS r");
    let resolved = layout(&[&["__sql_fun_rows_1"]], &["__sql_fun_rows_2"], false);
    let mut names = PrivateNames::new(&source, &resolved).unwrap();
    assert_eq!(names.next(), "__sql_fun_rows_3");
    assert_eq!(names.next(), "__sql_fun_rows_4");
}

#[test]
fn mismatched_layout_rejects_before_building_a_source() {
    let source = invocation("SELECT * FROM ROWS FROM (generate_series(1,2)) AS r");
    let resolved = layout(&[], &[], false);
    assert!(
        freeze(&resolved, source)
            .unwrap_err()
            .is_invalid_expression()
    );
}
