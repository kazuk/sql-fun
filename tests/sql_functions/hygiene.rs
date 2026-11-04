//! Regression fixtures for generated local identifier hygiene.

use crate::{IdWithName, builders::BonRow};
use std::future::Future;

use ::tokio_postgres::Client;

#[sql_fun::sql_query_many(sql = "select u.id, u.name from users AS u")]
async fn legacy<F, Fut>(
    params: &Client,
    mut rows: Vec<IdWithName>,
    builder: F,
) -> Result<Vec<IdWithName>, anyhow::Error>
where
    F: Fn(Vec<IdWithName>, IdWithName) -> Fut,
    Fut: Future<Output = Result<Vec<IdWithName>, anyhow::Error>>,
{
}

#[sql_fun::sql_query_many(sql = "select u.id, u.name from users AS u")]
async fn result_collector<F, Fut>(
    rows: &Client,
    mut result: Vec<IdWithName>,
    statement: F,
) -> Result<Vec<IdWithName>, anyhow::Error>
where
    F: Fn(Vec<IdWithName>, IdWithName) -> Fut,
    Fut: Future<Output = Result<Vec<IdWithName>, anyhow::Error>>,
{
}

#[sql_fun::sql_query_many(sql = "select u.id, u.name from users AS u")]
async fn builder_collector<F, Fut>(
    statement: &Client,
    mut builder: Vec<IdWithName>,
    result: F,
) -> Result<Vec<IdWithName>, anyhow::Error>
where
    F: Fn(Vec<IdWithName>, IdWithName) -> Fut,
    Fut: Future<Output = Result<Vec<IdWithName>, anyhow::Error>>,
{
}

#[sql_fun::sql_query_many(sql = "select u.id, u.name from users AS u")]
async fn candidate<F, Fut>(
    __sql_fun_rows_1: &Client,
    mut __sql_fun_rows: Vec<IdWithName>,
    __sql_fun_rows_2: F,
) -> Result<Vec<IdWithName>, anyhow::Error>
where
    F: Fn(Vec<IdWithName>, IdWithName) -> Fut,
    Fut: Future<Output = Result<Vec<IdWithName>, anyhow::Error>>,
{
}

#[sql_fun::sql_query_many(sql = "select u.id, u.name from users AS u")]
async fn raw_candidate<F, Fut>(
    params: &Client,
    mut r#__sql_fun_rows: Vec<IdWithName>,
    __sql_fun_rows_1: F,
) -> Result<Vec<IdWithName>, anyhow::Error>
where
    F: Fn(Vec<IdWithName>, IdWithName) -> Fut,
    Fut: Future<Output = Result<Vec<IdWithName>, anyhow::Error>>,
{
}

#[sql_fun::sql_query_many(sql = "select u.id, u.name from users AS u")]
async fn interrupted<F, Fut>(
    params: &Client,
    #[interrupt_future] interrupt_message: &mut (impl Future<Output = anyhow::Error> + Unpin),
    mut rows: Vec<IdWithName>,
    result: F,
) -> Result<Vec<IdWithName>, anyhow::Error>
where
    F: Fn(Vec<IdWithName>, IdWithName) -> Fut,
    Fut: Future<Output = Result<Vec<IdWithName>, anyhow::Error>>,
{
}

#[sql_fun::sql_query_many(sql = "select u.id, u.name from users AS u")]
async fn wrapper<F, Fut>(
    query_result: &Client,
    #[interrupt_future] interrupt_message: &mut (impl Future<Output = anyhow::Error> + Unpin),
    mut read_message: Vec<IdWithName>,
    statement: F,
) -> Result<Vec<IdWithName>, anyhow::Error>
where
    F: Fn(Vec<IdWithName>, IdWithName) -> Fut,
    Fut: Future<Output = Result<Vec<IdWithName>, anyhow::Error>>,
{
}

#[sql_fun::sql_query_many(sql = "select u.id, u.name from users AS u")]
async fn candidate_interrupted<F, Fut>(
    __sql_fun_read_message: &Client,
    #[interrupt_future] __sql_fun_interrupt_message: &mut (impl Future<Output = anyhow::Error> + Unpin),
    mut __sql_fun_rows: Vec<IdWithName>,
    __sql_fun_builder: F,
) -> Result<Vec<IdWithName>, anyhow::Error>
where
    F: Fn(Vec<IdWithName>, IdWithName) -> Fut,
    Fut: Future<Output = Result<Vec<IdWithName>, anyhow::Error>>,
{
}

#[sql_fun::sql_query_many(
    sql = "select u.id, u.name from users AS u",
    builder_setter = "consuming",
    builder_build_fallible = false
)]
async fn consuming<F, Fut>(
    params: &Client,
    mut builder: Vec<BonRow>,
    result: F,
) -> Result<Vec<BonRow>, anyhow::Error>
where
    F: Fn(Vec<BonRow>, BonRow) -> Fut,
    Fut: Future<Output = Result<Vec<BonRow>, anyhow::Error>>,
{
}

#[sql_fun::sql_query_one(sql = "#[choice]")]
#[sql_fragment(choice, choice, (choice), {
    #[args(id: i64, __sql_fun_result: i64)]
    (id, __sql_fun_result) => "select id, name from users where id = ${id}"
})]
async fn fragment(
    params: &Client,
    #[cond] choice: (i64, i64),
) -> Result<IdWithName, anyhow::Error> {
}

#[sql_fun::sql_query_one(sql = "select id, name from users where id = ${id}")]
async fn one(__sql_fun_params: &Client, id: i64) -> Result<IdWithName, anyhow::Error> {}

#[sql_fun::sql_query_opt(sql = "select id, name from users where id = ${id}")]
async fn optional(params: &Client, id: i64) -> Result<Option<IdWithName>, anyhow::Error> {}

#[sql_fun::sql_statement(sql = "delete from users where id = ${id}")]
async fn statement(__sql_fun_params: &Client, id: i64) -> Result<u64, anyhow::Error> {}
