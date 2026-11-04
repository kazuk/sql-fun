//! Execute generated PostgreSQL row conversion and interruption without a database server.

use crate::builders::{BonRow, ConsumingRow, QueryError};

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
    sql = "select u.id, u.name from users AS u",
    builder_setter = "consuming",
    builder_build_fallible = true
)]
async fn rejected_row(client: &Client) -> Result<ConsumingRow, QueryError> {}

#[sql_fun::sql_query_one(
    sql = "select u.id, u.name from users AS u",
    builder_setter = "consuming",
    builder_build_fallible = false
)]
async fn accepted_row(client: &Client) -> Result<BonRow, QueryError> {}

#[sql_fun::sql_statement(sql = "delete from users where id = ${id}")]
async fn interrupted_statement(
    client: &Client,
    id: i64,
    #[interrupt_future] interrupt: &mut (impl std::future::Future<Output = anyhow::Error> + Unpin),
) -> Result<u64, anyhow::Error> {
}

#[sql_fun::sql_statement(sql = "delete from users where id = ${id}")]
async fn native_interrupt_wrapped_error(
    client: &::tokio_postgres::Client,
    id: i64,
    #[interrupt_future] interrupt: &mut (
             impl std::future::Future<Output = tokio_postgres::Error> + Unpin
         ),
) -> Result<u64, anyhow::Error> {
}

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
) -> Result<u64, anyhow::Error> {
}

// A deterministic driver fixture exercises the emitted optional/stream/execute paths.
struct RowsClient {
    count: usize,
    fail: bool,
}

impl RowsClient {
    fn new(count: usize, fail: bool) -> Self {
        Self { count, fail }
    }
}

impl RowsClient {
    async fn query_opt(
        &self,
        _sql: &str,
        _params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<Option<Row>, std::io::Error> {
        if self.fail {
            Err(std::io::Error::other("driver failure"))
        } else if self.count == 0 {
            Ok(None)
        } else {
            Ok(Some(Row))
        }
    }
}

impl RowsClient {
    async fn query_raw<'a>(
        &self,
        _sql: &str,
        _params: impl IntoIterator<Item = &'a (dyn tokio_postgres::types::ToSql + Sync)>,
    ) -> Result<impl futures_util::Stream<Item = Result<Row, std::io::Error>>, std::io::Error> {
        if self.fail {
            Err(std::io::Error::other("driver failure"))
        } else {
            Ok(futures_util::stream::iter((0..self.count).map(|_| Ok(Row))))
        }
    }
}

impl RowsClient {
    async fn execute(
        &self,
        _sql: &str,
        _params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<u64, std::io::Error> {
        if self.fail {
            Err(std::io::Error::other("driver failure"))
        } else {
            Ok(self.count as u64)
        }
    }
}

#[sql_fun::sql_query_opt(
    sql = "select u.id, u.name from users AS u",
    builder_setter = "consuming",
    builder_build_fallible = false
)]
async fn optional_row(client: &RowsClient) -> Result<Option<BonRow>, QueryError> {}

#[sql_fun::sql_query_many(
    sql = "select u.id, u.name from users AS u",
    builder_setter = "consuming",
    builder_build_fallible = false
)]
async fn count_rows<F, Fut>(
    client: &RowsClient,
    mut count: usize,
    handler: F,
) -> Result<usize, QueryError>
where
    F: Fn(usize, BonRow) -> Fut,
    Fut: Future<Output = Result<usize, QueryError>>,
{
}

#[sql_fun::sql_statement(sql = "delete from users where id = ${id}")]
async fn affected_rows(client: &RowsClient, id: i64) -> Result<u64, QueryError> {}

async fn verify_optional_and_streaming() {
    let empty = RowsClient::new(0, false);
    let rows = RowsClient::new(2, false);
    let failing = RowsClient::new(0, true);
    assert!(optional_row(&empty).await.unwrap().is_none());
    assert!(optional_row(&rows).await.unwrap().is_some());
    assert_eq!(
        optional_row(&failing).await.unwrap_err().to_string(),
        "driver failure"
    );
    assert_eq!(
        count_rows(&empty, 7, |_, _| async {
            panic!("empty stream called handler")
        })
        .await
        .unwrap(),
        7
    );
    assert_eq!(
        count_rows(&rows, 7, |count, _| async move { Ok(count + 1) })
            .await
            .unwrap(),
        9
    );
    assert_eq!(
        count_rows(&rows, 0, |_, _| async {
            Err(QueryError::from(std::io::Error::other("handler failure")))
        })
        .await
        .unwrap_err()
        .to_string(),
        "handler failure"
    );
    assert_eq!(
        count_rows(&failing, 0, |_, _| async {
            panic!("failed query called handler")
        })
        .await
        .unwrap_err()
        .to_string(),
        "driver failure"
    );
    assert_eq!(affected_rows(&rows, 1).await.unwrap(), 2);
    assert_eq!(
        affected_rows(&failing, 1).await.unwrap_err().to_string(),
        "driver failure"
    );
}

pub(super) fn verify() {
    // Check-mode functions deliberately contain only metadata stubs.
    if std::env::var("SQL_FUN_MODE").as_deref() == Ok("CHECK") {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        verify_optional_and_streaming().await;
        let supplied_error = std::io::Error::other("statement interrupted");
        let mut interrupt = std::future::ready(anyhow::Error::new(supplied_error));
        let error = interrupted_statement(&Client, 1, &mut interrupt)
            .await
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().to_string(),
            "statement interrupted",
        );

        let mut interrupt = std::future::ready(InterruptError::from("trait conversion"));
        let error = interrupted_with_inherent_into(&Client, 1, &mut interrupt)
            .await
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().to_string(),
            "trait conversion",
        );

        let error = rejected_row(&Client).await.unwrap_err();
        assert_eq!(error.to_string(), "row rejected");
        assert!(accepted_row(&Client).await.is_ok());
    });
}
