use tokio_postgres::Client;

#[sql_fun::sql_statement(sql = "delete from users where id = ${id}")]
async fn delete_user_by_id(client: Client, id: i64) -> Result<u64, anyhow::Error> {}

fn main() {}
