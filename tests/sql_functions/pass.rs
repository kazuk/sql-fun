use std::future::Future;

use tokio_postgres::Client;

#[derive(derive_builder::Builder)]
struct IdWithName {
    id: i64,
    name: String,
}

impl IdWithName {
    fn builder() -> IdWithNameBuilder {
        IdWithNameBuilder::default()
    }
}

#[sql_fun::sql_query_one(sql = "select u.id, u.name from users")]
async fn select_user_id_with_names(client: Client) -> Result<IdWithName, anyhow::Error> {}

#[sql_fun::sql_query_opt(sql = "select u.id, u.name from users")]
async fn select_user_id_with_names_opt(
    client: Client,
) -> Result<Option<IdWithName>, anyhow::Error> {
}

struct Collector;

#[sql_fun::sql_query_many(sql = "select u.id, u.name from users")]
async fn select_user_id_with_names_many<F, Fut>(
    client: Client,
    mut collector: Collector,
    handler: F,
) -> Result<Collector, anyhow::Error>
where
    F: Fn(Collector, IdWithName) -> Fut,
    Fut: Future<Output = Result<Collector, anyhow::Error>>,
{
}

#[sql_fun::sql_statement(sql = "delete from users where id = ${id}")]
async fn delete_user_by_id(client: Client, id: i64) -> Result<u64, anyhow::Error> {}

fn main() {}
