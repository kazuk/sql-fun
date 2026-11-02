//! Run checked, generated SQL functions against a disposable PostgreSQL database.

mod inventory;

use ::tokio_postgres::{Client, NoTls};

#[derive(Debug, PartialEq, bon::Builder)]
struct Item {
    id: i64,
    name: String,
}

#[sql_fun::sql_query_one(
    sql = "INSERT INTO e2e_items (id, name) VALUES (${id:bigint}, ${name:text}) RETURNING id, name",
    builder_setter = "consuming",
    builder_build_fallible = false
)]
async fn insert_item(client: &Client, id: i64, name: &str) -> Result<Item, tokio_postgres::Error> {}

#[sql_fun::sql_query_one(
    sql = "SELECT id, name FROM e2e_items WHERE id = ${id:bigint}",
    builder_setter = "consuming",
    builder_build_fallible = false
)]
async fn select_item(client: &Client, id: i64) -> Result<Item, tokio_postgres::Error> {}

#[sql_fun::sql_query_one(
    sql = "UPDATE e2e_items SET id = id, name = ${name:text} WHERE id = ${id:bigint} RETURNING id, name",
    builder_setter = "consuming",
    builder_build_fallible = false
)]
async fn update_item(client: &Client, id: i64, name: &str) -> Result<Item, tokio_postgres::Error> {}

#[sql_fun::sql_query_one(
    sql = "DELETE FROM e2e_items WHERE id = ${id:bigint} RETURNING id, name",
    builder_setter = "consuming",
    builder_build_fallible = false
)]
async fn delete_item(client: &Client, id: i64) -> Result<Item, tokio_postgres::Error> {}

#[sql_fun::sql_query_opt(
    sql = "SELECT id, name FROM e2e_items WHERE id = ${id:bigint}",
    builder_setter = "consuming",
    builder_build_fallible = false
)]
async fn find_item(client: &Client, id: i64) -> Result<Option<Item>, tokio_postgres::Error> {}

#[sql_fun::sql_query_one(
    sql = "SELECT a.id, b.name FROM e2e_items a JOIN e2e_items b ON a.id = b.id WHERE a.id = ${id:bigint}",
    builder_setter = "consuming",
    builder_build_fallible = false
)]
async fn joined_item(client: &Client, id: i64) -> Result<Item, tokio_postgres::Error> {}

#[sql_fun::sql_query_one(
    sql = "WITH chosen AS (SELECT id, name FROM e2e_items) SELECT id, name FROM chosen WHERE id = ${id:bigint}",
    builder_setter = "consuming",
    builder_build_fallible = false
)]
async fn cte_item(client: &Client, id: i64) -> Result<Item, tokio_postgres::Error> {}

#[sql_fun::sql_query_one(
    sql = "SELECT a.id, (SELECT b.name FROM e2e_items b WHERE b.id = a.id) AS name FROM e2e_items a WHERE a.id = ${id:bigint}",
    builder_setter = "consuming",
    builder_build_fallible = false
)]
async fn nested_item(client: &Client, id: i64) -> Result<Item, tokio_postgres::Error> {}

#[sql_fun::sql_query_one(
    sql = "SELECT id, name FROM e2e_item_view WHERE id = ${id:bigint}",
    builder_setter = "consuming",
    builder_build_fallible = false
)]
async fn view_item(client: &Client, id: i64) -> Result<Item, tokio_postgres::Error> {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    inventory::verify()?;
    let connection_string = std::env::var("SQL_FUN_E2E_DATABASE_URL")?;
    let (client, connection) = tokio_postgres::connect(&connection_string, NoTls).await?;
    let connection_task = tokio::spawn(connection);

    let id = 42_i64;
    let name = "checked SQL: 'quoted' and 日本語";
    assert_eq!(insert_item(&client, id, name).await?.id, id);
    let selected = select_item(&client, id).await?;
    assert_eq!(selected.id, id);
    assert_eq!(selected.name, name);

    assert_eq!(joined_item(&client, id).await?, selected);
    assert_eq!(cte_item(&client, id).await?, selected);
    assert_eq!(nested_item(&client, id).await?, selected);
    assert_eq!(view_item(&client, id).await?, selected);

    let updated_name = "updated value";
    assert_eq!(
        update_item(&client, id, updated_name).await?.name,
        updated_name
    );
    assert_eq!(select_item(&client, id).await?.name, updated_name);
    assert_eq!(delete_item(&client, id).await?.id, id);
    assert_eq!(find_item(&client, id).await?, None);

    drop(client);
    connection_task.await??;
    Ok(())
}
