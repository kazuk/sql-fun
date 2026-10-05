//! Execute generated PostgreSQL row conversion and interruption without a database server.

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

    async fn execute(
        &self,
        _sql: &str,
        _params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<u64, tokio_postgres::Error> {
        std::future::pending().await
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

#[sql_fun::sql_statement(sql = "delete from users where id = ${id}")]
async fn interrupted_statement(
    client: &Client,
    id: i64,
    #[interrupt_future] interrupt: &mut (impl std::future::Future<Output = anyhow::Error> + Unpin),
) -> Result<u64, anyhow::Error> {}

#[sql_fun::sql_statement(sql = "delete from users where id = ${id}")]
async fn native_interrupt_wrapped_error(
    client: &::tokio_postgres::Client,
    id: i64,
    #[interrupt_future] interrupt: &mut (impl std::future::Future<Output = tokio_postgres::Error> + Unpin),
) -> Result<u64, anyhow::Error> {}

struct InterruptError(std::io::Error);

impl From<&str> for InterruptError {
    fn from(message: &str) -> Self {
        Self(std::io::Error::other(message))
    }
}

impl InterruptError {
    // The generator must use the conversion trait rather than this user method.
    fn into(self) {
        panic!("inherent into must not be called");
    }
}

impl From<InterruptError> for anyhow::Error {
    fn from(error: InterruptError) -> Self {
        Self::new(error.0)
    }
}

#[sql_fun::sql_statement(sql = "delete from users where id = ${id}")]
async fn interrupted_with_inherent_into(
    client: &Client,
    id: i64,
    #[interrupt_future] interrupt: &mut (impl std::future::Future<Output = InterruptError> + Unpin),
) -> Result<u64, anyhow::Error> {}

pub(super) fn verify() {
    // Check-mode functions deliberately contain only metadata stubs.
    if std::env::var("SQL_FUN_MODE").as_deref() == Ok("CHECK") {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let supplied_error = std::io::Error::other("statement interrupted");
        let mut interrupt = std::future::ready(anyhow::Error::new(supplied_error));
        let error = interrupted_statement(&Client, 1, &mut interrupt).await.unwrap_err();
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().to_string(),
            "statement interrupted",
        );

        let mut interrupt = std::future::ready(InterruptError::from("trait conversion"));
        let error = interrupted_with_inherent_into(&Client, 1, &mut interrupt).await.unwrap_err();
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().to_string(),
            "trait conversion",
        );

        let error = rejected_row(&Client).await.unwrap_err();
        assert_eq!(error.to_string(), "row rejected");
        assert!(accepted_row(&Client).await.is_ok());
    });
}
