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

#[sql_fun::sql_query_one("select u.id, u.name from users")]
async fn select_user_id_with_names(client: Client) -> Result<IdWithName, anyhow::Error> {}

async fn connect_exec() -> Result<(), anyhow::Error> {
    let (client, connection) =
        tokio_postgres::connect("host=localhost user=postgres", tokio_postgres::NoTls).await?;
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });
    select_user_id_with_names(client).await?;
    Ok(())
}

fn main() {}
