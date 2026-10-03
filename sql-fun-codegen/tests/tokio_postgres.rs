//!
//! `tokio-postgres` code generation experiment
//!
#![allow(dead_code)]
#![allow(unused_variables)]

use std::time::Duration;

#[derive(derive_builder::Builder)]
struct UserRow {
    id: i64,
    first_name: String,
    last_name: String,
}

impl UserRow {
    fn builder() -> UserRowBuilder {
        UserRowBuilder::default()
    }
}

#[derive(thiserror::Error, Debug)]
enum QueryError {
    #[error(transparent)]
    Postgres(#[from] tokio_postgres::Error),
    #[error(transparent)]
    UserRow(#[from] UserRowBuilderError),

    #[error("query timeout")]
    Timeout(),
}
impl QueryError {
    #[expect(
        clippy::missing_const_for_fn,
        reason = "Fixture helper constness is outside the identifier-hygiene fix."
    )]
    fn timeout() -> Self {
        Self::Timeout()
    }
}

///
/// sql-fun::sql_query_one macro output source code like a bellow
///
#[expect(dead_code)]
async fn find_user_by_id<C>(client: &C, id: i64) -> Result<UserRow, QueryError>
where
    C: tokio_postgres::GenericClient + Sync,
{
    let result = client
        .query_one(
            "select id, first_name, last_name from users where id=1",
            &[&id],
        )
        .await?;

    let mut builder = UserRow::builder();
    builder.id(result.try_get(0)?);
    builder.first_name(result.try_get(1)?);
    builder.last_name(result.try_get(2)?);

    Ok(builder.build()?)
}

async fn find_user_by_id_with_interrupt<C, Fut>(
    client: &C,
    id: i64,
    interrupt: &mut Fut,
) -> Result<UserRow, QueryError>
where
    C: tokio_postgres::GenericClient + Sync,
    Fut: Future<Output = QueryError> + Unpin,
{
    tokio::select! {
        // run query
        res = find_user_by_id( client, id ) => res ,
        // accept interrupt
        interrupt_result = interrupt => Err(interrupt_result)
    }
}

async fn example_caller(client: tokio_postgres::Client) -> Result<(), QueryError> {
    let mut interrupt = Box::pin(async {
        tokio::time::sleep(Duration::from_secs(30)).await;
        QueryError::timeout()
    });

    let user_one = find_user_by_id_with_interrupt(&client, 1, &mut interrupt).await?;
    let user_two = find_user_by_id_with_interrupt(&client, 2, &mut interrupt).await?;
    Ok(())
}

// #[sql-fun::prepare_query_one(
//   sql="select id, first_name, last_name
//          from users
//          where first_name=${first_name:&str}
//          and last_name=${last_name:&str}"
//   execute_result = Result<UserRow, QueryError>
// )]
#[expect(dead_code)]
struct SelectUserByName {
    statement: tokio_postgres::Statement,
}

#[expect(dead_code)]
impl SelectUserByName {
    pub async fn prepare<C>(client: &C) -> Result<Self, tokio_postgres::Error>
    where
        C: tokio_postgres::GenericClient + Sync,
    {
        let statement = client
            .prepare(
                "select id, first_name, last_name from users where first_name=$1 and last_name=$2",
            )
            .await?;
        Ok(Self { statement })
    }

    pub async fn execute<C>(
        &self,
        client: &C,
        first_name: &str,
        last_name: &str,
    ) -> Result<UserRow, QueryError>
    where
        C: tokio_postgres::GenericClient + Sync,
    {
        let result = client
            .query_one(&self.statement, &[&first_name, &last_name])
            .await?;

        let column_id: i64 = result.try_get(0)?;
        let column_first_name: String = result.try_get(1)?;
        let column_last_name: String = result.try_get(2)?;

        let mut builder = UserRow::builder();
        Ok(builder
            .id(column_id)
            .first_name(column_first_name)
            .last_name(column_last_name)
            .build()?)
    }
}
