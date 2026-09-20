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

struct Collector;

#[sql_fun::sql_query_many(sql = "select u.id, u.name from users")]
async fn select_user_id_with_names<F, Fut>(
    client: Client,
    mut collector: Collector,
    handler: F,
) -> Result<Collector, anyhow::Error>
where
    F: Fn(Collector, IdWithName) -> Fut,
    Fut: Future<Output = Result<Collector, anyhow::Error>>,
{
}

fn main() {}
