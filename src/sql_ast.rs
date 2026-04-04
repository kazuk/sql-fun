use std::clone::Clone;

use pg_query::{
    Node, NodeEnum, NodeRef,
    protobuf::{ColumnRef, ResTarget},
};

use crate::errors::SqlFunError;

fn get_res_target_column_name(res_target: &ResTarget) -> Option<String> {
    if res_target.name.is_empty() {
        let column_ref = get_column_ref_from_res_target(res_target)?;
        let last_field = column_ref.fields.last().map(|v| &v.node)?;
        let NodeEnum::String(string) = last_field.as_ref()? else {
            return None;
        };
        Some(string.sval.clone())
    } else {
        Some(res_target.name.clone())
    }
}

fn get_column_ref_from_res_target(res_target: &ResTarget) -> Option<ColumnRef> {
    let val = res_target.val.as_ref().map(|v| &v.node);
    let Some(NodeEnum::ColumnRef(column_ref)) = val.and_then(Clone::clone) else {
        return None;
    };
    Some(column_ref)
}

fn get_field_name_from_res_target(res_target: &ResTarget) -> Result<String, SqlFunError> {
    get_res_target_column_name(res_target).ok_or_else(SqlFunError::unnamed_column_in_result_rowset)
}

pub fn node_list_to_res_target_list(node_list: &Vec<Node>) -> Vec<ResTarget> {
    let mut res_target_list: Vec<ResTarget> = Vec::new();
    for node in node_list {
        let Some(NodeEnum::ResTarget(ref res_target)) = node.node else {
            continue;
        };
        res_target_list.push(*res_target.clone());
    }
    res_target_list
}

pub fn get_returning_res_target(sql_ast: &NodeRef<'_>) -> Vec<ResTarget> {
    match sql_ast {
        NodeRef::SelectStmt(select_stmt) => {
            node_list_to_res_target_list(select_stmt.target_list.as_ref())
        }
        NodeRef::UpdateStmt(update_stmt) => {
            node_list_to_res_target_list(update_stmt.returning_list.as_ref())
        }
        NodeRef::InsertStmt(insert_stmt) => {
            node_list_to_res_target_list(insert_stmt.returning_list.as_ref())
        }
        NodeRef::DeleteStmt(delete_stmt) => {
            node_list_to_res_target_list(delete_stmt.returning_list.as_ref())
        }
        _ => Vec::new(),
    }
}

pub fn get_result_columns(sql_ast: &NodeRef<'_>) -> Result<Vec<String>, SqlFunError> {
    let res_targets = get_returning_res_target(sql_ast);
    let mut columns = Vec::new();
    for res_target in res_targets {
        let field_name = get_field_name_from_res_target(&res_target)?;
        columns.push(field_name);
    }
    Ok(columns)
}

pub fn get_result_set_column_names(
    sql_ast: &pg_query::ParseResult,
) -> Result<Vec<String>, SqlFunError> {
    if let Some((node_ref, _, _, _)) = sql_ast.protobuf.nodes().into_iter().next() {
        return get_result_columns(&node_ref);
    }
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use std::vec;

    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("select id, name from users", vec!["id","name"])]
    #[case("insert into users(name) values($1) returning id", vec!["id"])]
    #[case("UPDATE users SET name = 'Alice' WHERE id = 1 RETURNING id, name", vec!["id", "name"])]
    #[case("DELETE FROM users WHERE id = 1 RETURNING id", vec!["id"])]
    #[case("select id as id_value, name as name_value from users", vec!["id_value","name_value"])]
    #[case("insert into users(name) values($1) returning id as new_id", vec!["new_id"])]
    fn test_get_result_columns(#[case] sql: &str, #[case] expected: Vec<&str>) {
        let sql_ast = pg_query::parse(sql).unwrap();
        for (node_ref, _, _, _) in sql_ast.protobuf.nodes() {
            let result_columns = get_result_columns(&node_ref).unwrap();
            assert_eq!(result_columns, expected);
            break;
        }
    }

    #[rstest]
    #[case("select 1+1")]
    #[case("insert into users(name) values($1) returning (to_lower(name))")]
    fn test_no_named_column_exists(#[case] sql: &str) {
        let sql_ast = pg_query::parse(sql).unwrap();
        for (node_ref, _, _, _) in sql_ast.protobuf.nodes() {
            let result = get_result_columns(&node_ref);
            assert!(result.is_err());
            let message = result.unwrap_err().to_string();
            assert!(
                message.contains("Unnamed column found in result set."),
                "error message :{}",
                message
            );
            break;
        }
    }
}
