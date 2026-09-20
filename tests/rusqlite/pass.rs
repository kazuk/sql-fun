use rusqlite::Connection;

#[derive(derive_builder::Builder)]
struct User {
    id: i64,
    name: String,
}

impl User {
    fn builder() -> UserBuilder {
        UserBuilder::default()
    }
}

#[sql_fun::sql_query_one(datasource = "sqlite", sql = "select id, name from users where id = ${id}")]
fn find_user(connection: &Connection, id: i64) -> Result<User, anyhow::Error> {}

#[sql_fun::sql_query_opt(datasource = "sqlite", sql = "select id, name from users where id = ${id}")]
fn find_user_opt(connection: &Connection, id: i64) -> Result<Option<User>, anyhow::Error> {}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "select id, name from users")]
fn find_users<F>(
    connection: &Connection,
    mut collector: Vec<User>,
    handler: F,
) -> Result<Vec<User>, anyhow::Error>
where
    F: Fn(Vec<User>, User) -> Result<Vec<User>, anyhow::Error>,
{
}

#[sql_fun::sql_statement(datasource = "sqlite", sql = "delete from users where id = ${id}")]
fn delete_user(connection: &Connection, id: i64) -> Result<usize, anyhow::Error> {}

fn main() {}
