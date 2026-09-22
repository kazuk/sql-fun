//! Execute generated PostgreSQL row conversion without a database server.

use super::builders::{BonRow, ConsumingRow, QueryError};

struct Client;
struct Row;

trait Column {
    fn value() -> Self;
}

impl Column for i64 {
    fn value() -> Self {
        1
    }
}

impl Column for String {
    fn value() -> Self {
        "reject".to_owned()
    }
}

impl Row {
    fn try_get<T: Column>(&self, _index: usize) -> Result<T, tokio_postgres::Error> {
        Ok(T::value())
    }
}

impl Client {
    async fn query_one(
        &self,
        _sql: &str,
        _params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<Row, tokio_postgres::Error> {
        Ok(Row)
    }
}

#[sql_fun::sql_query_one(
    sql = "select u.id, u.name from users",
    builder_setter = "consuming",
    builder_build_fallible = true
)]
async fn rejected_row(client: &Client) -> Result<ConsumingRow, QueryError> {}

#[sql_fun::sql_query_one(
    sql = "select u.id, u.name from users",
    builder_setter = "consuming",
    builder_build_fallible = false
)]
async fn accepted_row(client: &Client) -> Result<BonRow, QueryError> {}

pub(super) fn verify() {
    // Check-mode functions deliberately contain only metadata stubs.
    if std::env::var("SQL_FUN_MODE").as_deref() == Ok("CHECK") {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let error = rejected_row(&Client).await.unwrap_err();
        assert_eq!(error.to_string(), "row rejected");
        assert!(accepted_row(&Client).await.is_ok());
    });
}
