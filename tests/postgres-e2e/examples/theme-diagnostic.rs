//! Invalid SQL used only by the runner's forced-color diagnostic check.

#[sql_fun::sql_query_one(sql = "SELECT 1 AS value WHERE ${id} IS NULL")]
async fn unresolved_parameter(
    client: &::tokio_postgres::Client,
    id: i64,
) -> Result<i32, ::tokio_postgres::Error> {
}

fn main() {}
