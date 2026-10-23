//! Run checked, generated SQL functions against a disposable PostgreSQL database.

use ::tokio_postgres::{Client, NoTls};

#[derive(Debug, PartialEq, bon::Builder)]
struct Item {
    id: i64,
    name: String,
}

#[sql_fun::sql_statement(sql = "INSERT INTO e2e_items (id, name) VALUES (${id}, ${name})")]
async fn insert_item(client: &Client, id: i64, name: &str) -> Result<u64, tokio_postgres::Error> {}

#[sql_fun::sql_query_one(
    sql = "SELECT id, name FROM e2e_items WHERE id = ${id}",
    builder_setter = "consuming",
    builder_build_fallible = false
)]
async fn select_item(client: &Client, id: i64) -> Result<Item, tokio_postgres::Error> {}

#[sql_fun::sql_statement(sql = "UPDATE e2e_items SET name = ${name} WHERE id = ${id}")]
async fn update_item(client: &Client, id: i64, name: &str) -> Result<u64, tokio_postgres::Error> {}

#[sql_fun::sql_statement(sql = "DELETE FROM e2e_items WHERE id = ${id}")]
async fn delete_item(client: &Client, id: i64) -> Result<u64, tokio_postgres::Error> {}

#[sql_fun::sql_query_opt(
    sql = "SELECT id, name FROM e2e_items WHERE id = ${id}",
    builder_setter = "consuming",
    builder_build_fallible = false
)]
async fn find_item(client: &Client, id: i64) -> Result<Option<Item>, tokio_postgres::Error> {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let connection_string = std::env::var("SQL_FUN_E2E_DATABASE_URL")?;
    let (client, connection) = tokio_postgres::connect(&connection_string, NoTls).await?;
    let connection_task = tokio::spawn(connection);

    let id = 42_i64;
    let name = "checked SQL: 'quoted' and 日本語";
    assert_eq!(insert_item(&client, id, name).await?, 1);
    let selected = select_item(&client, id).await?;
    assert_eq!(selected.id, id);
    assert_eq!(selected.name, name);

    let updated_name = "updated value";
    assert_eq!(update_item(&client, id, updated_name).await?, 1);
    assert_eq!(select_item(&client, id).await?.name, updated_name);
    assert_eq!(delete_item(&client, id).await?, 1);
    assert_eq!(find_item(&client, id).await?, None);

    drop(client);
    connection_task.await??;
    Ok(())
}
