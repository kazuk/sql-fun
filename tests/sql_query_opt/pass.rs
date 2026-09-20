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

#[sql_fun::sql_query_opt(sql = "select u.id, u.name from users")]
async fn select_user_id_with_names(client: Client) -> Result<Option<IdWithName>, anyhow::Error> {}

fn main() {}
