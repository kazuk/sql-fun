mod parameter_source;

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



mod builders;
use self::builders::{BonRow, ConsumingRow, MutableRow, QueryError};

#[sql_fun::sql_query_one(sql = "select u.id, u.name from users", builder_setter = "mutable", builder_build_fallible = true)]
async fn mutable_true_one_false(client: &Client) -> Result<IdWithName, anyhow::Error>  {}

#[sql_fun::sql_query_opt(sql = "select u.id, u.name from users", builder_setter = "mutable", builder_build_fallible = true)]
async fn mutable_true_opt_false(client: &Client) -> Result<Option<IdWithName>, anyhow::Error>  {}

#[sql_fun::sql_query_many(sql = "select u.id, u.name from users", builder_setter = "mutable", builder_build_fallible = true)]
async fn mutable_true_many_false<F, Fut>(client: &Client, mut collector: Vec<IdWithName>, handler: F) -> Result<Vec<IdWithName>, anyhow::Error> where F: Fn(Vec<IdWithName>, IdWithName) -> Fut, Fut: Future<Output = Result<Vec<IdWithName>, anyhow::Error>> {}

#[sql_fun::sql_query_one(sql = "select u.id, u.name from users", builder_setter = "mutable", builder_build_fallible = true)]
async fn mutable_true_one_true(client: &Client, #[interrupt_future] interrupt: &mut (impl Future<Output = anyhow::Error> + Unpin)) -> Result<IdWithName, anyhow::Error>  {}

#[sql_fun::sql_query_opt(sql = "select u.id, u.name from users", builder_setter = "mutable", builder_build_fallible = true)]
async fn mutable_true_opt_true(client: &Client, #[interrupt_future] interrupt: &mut (impl Future<Output = anyhow::Error> + Unpin)) -> Result<Option<IdWithName>, anyhow::Error>  {}

#[sql_fun::sql_query_many(sql = "select u.id, u.name from users", builder_setter = "mutable", builder_build_fallible = true)]
async fn mutable_true_many_true<F, Fut>(client: &Client, #[interrupt_future] interrupt: &mut (impl Future<Output = anyhow::Error> + Unpin), mut collector: Vec<IdWithName>, handler: F) -> Result<Vec<IdWithName>, anyhow::Error> where F: Fn(Vec<IdWithName>, IdWithName) -> Fut, Fut: Future<Output = Result<Vec<IdWithName>, anyhow::Error>> {}

#[sql_fun::sql_query_one(sql = "select u.id, u.name from users", builder_setter = "mutable", builder_build_fallible = false)]
async fn mutable_false_one_false(client: &Client) -> Result<MutableRow, QueryError>  {}

#[sql_fun::sql_query_opt(sql = "select u.id, u.name from users", builder_setter = "mutable", builder_build_fallible = false)]
async fn mutable_false_opt_false(client: &Client) -> Result<Option<MutableRow>, QueryError>  {}

#[sql_fun::sql_query_many(sql = "select u.id, u.name from users", builder_setter = "mutable", builder_build_fallible = false)]
async fn mutable_false_many_false<F, Fut>(client: &Client, mut collector: Vec<MutableRow>, handler: F) -> Result<Vec<MutableRow>, QueryError> where F: Fn(Vec<MutableRow>, MutableRow) -> Fut, Fut: Future<Output = Result<Vec<MutableRow>, QueryError>> {}

#[sql_fun::sql_query_one(sql = "select u.id, u.name from users", builder_setter = "mutable", builder_build_fallible = false)]
async fn mutable_false_one_true(client: &Client, #[interrupt_future] interrupt: &mut (impl Future<Output = QueryError> + Unpin)) -> Result<MutableRow, QueryError>  {}

#[sql_fun::sql_query_opt(sql = "select u.id, u.name from users", builder_setter = "mutable", builder_build_fallible = false)]
async fn mutable_false_opt_true(client: &Client, #[interrupt_future] interrupt: &mut (impl Future<Output = QueryError> + Unpin)) -> Result<Option<MutableRow>, QueryError>  {}

#[sql_fun::sql_query_many(sql = "select u.id, u.name from users", builder_setter = "mutable", builder_build_fallible = false)]
async fn mutable_false_many_true<F, Fut>(client: &Client, #[interrupt_future] interrupt: &mut (impl Future<Output = QueryError> + Unpin), mut collector: Vec<MutableRow>, handler: F) -> Result<Vec<MutableRow>, QueryError> where F: Fn(Vec<MutableRow>, MutableRow) -> Fut, Fut: Future<Output = Result<Vec<MutableRow>, QueryError>> {}

#[sql_fun::sql_query_one(sql = "select u.id, u.name from users", builder_setter = "consuming", builder_build_fallible = true)]
async fn consuming_true_one_false(client: &Client) -> Result<ConsumingRow, QueryError>  {}

#[sql_fun::sql_query_opt(sql = "select u.id, u.name from users", builder_setter = "consuming", builder_build_fallible = true)]
async fn consuming_true_opt_false(client: &Client) -> Result<Option<ConsumingRow>, QueryError>  {}

#[sql_fun::sql_query_many(sql = "select u.id, u.name from users", builder_setter = "consuming", builder_build_fallible = true)]
async fn consuming_true_many_false<F, Fut>(client: &Client, mut collector: Vec<ConsumingRow>, handler: F) -> Result<Vec<ConsumingRow>, QueryError> where F: Fn(Vec<ConsumingRow>, ConsumingRow) -> Fut, Fut: Future<Output = Result<Vec<ConsumingRow>, QueryError>> {}

#[sql_fun::sql_query_one(sql = "select u.id, u.name from users", builder_setter = "consuming", builder_build_fallible = true)]
async fn consuming_true_one_true(client: &Client, #[interrupt_future] interrupt: &mut (impl Future<Output = QueryError> + Unpin)) -> Result<ConsumingRow, QueryError>  {}

#[sql_fun::sql_query_opt(sql = "select u.id, u.name from users", builder_setter = "consuming", builder_build_fallible = true)]
async fn consuming_true_opt_true(client: &Client, #[interrupt_future] interrupt: &mut (impl Future<Output = QueryError> + Unpin)) -> Result<Option<ConsumingRow>, QueryError>  {}

#[sql_fun::sql_query_many(sql = "select u.id, u.name from users", builder_setter = "consuming", builder_build_fallible = true)]
async fn consuming_true_many_true<F, Fut>(client: &Client, #[interrupt_future] interrupt: &mut (impl Future<Output = QueryError> + Unpin), mut collector: Vec<ConsumingRow>, handler: F) -> Result<Vec<ConsumingRow>, QueryError> where F: Fn(Vec<ConsumingRow>, ConsumingRow) -> Fut, Fut: Future<Output = Result<Vec<ConsumingRow>, QueryError>> {}

#[sql_fun::sql_query_one(sql = "select u.id, u.name from users", builder_setter = "consuming", builder_build_fallible = false)]
async fn consuming_false_one_false(client: &Client) -> Result<BonRow, QueryError>  {}

#[sql_fun::sql_query_opt(sql = "select u.id, u.name from users", builder_setter = "consuming", builder_build_fallible = false)]
async fn consuming_false_opt_false(client: &Client) -> Result<Option<BonRow>, QueryError>  {}

#[sql_fun::sql_query_many(sql = "select u.id, u.name from users", builder_setter = "consuming", builder_build_fallible = false)]
async fn consuming_false_many_false<F, Fut>(client: &Client, mut collector: Vec<BonRow>, handler: F) -> Result<Vec<BonRow>, QueryError> where F: Fn(Vec<BonRow>, BonRow) -> Fut, Fut: Future<Output = Result<Vec<BonRow>, QueryError>> {}

#[sql_fun::sql_query_one(sql = "select u.id, u.name from users", builder_setter = "consuming", builder_build_fallible = false)]
async fn consuming_false_one_true(client: &Client, #[interrupt_future] interrupt: &mut (impl Future<Output = QueryError> + Unpin)) -> Result<BonRow, QueryError>  {}

#[sql_fun::sql_query_opt(sql = "select u.id, u.name from users", builder_setter = "consuming", builder_build_fallible = false)]
async fn consuming_false_opt_true(client: &Client, #[interrupt_future] interrupt: &mut (impl Future<Output = QueryError> + Unpin)) -> Result<Option<BonRow>, QueryError>  {}

#[sql_fun::sql_query_many(sql = "select u.id, u.name from users", builder_setter = "consuming", builder_build_fallible = false)]
async fn consuming_false_many_true<F, Fut>(client: &Client, #[interrupt_future] interrupt: &mut (impl Future<Output = QueryError> + Unpin), mut collector: Vec<BonRow>, handler: F) -> Result<Vec<BonRow>, QueryError> where F: Fn(Vec<BonRow>, BonRow) -> Fut, Fut: Future<Output = Result<Vec<BonRow>, QueryError>> {}

#[sql_fun::sql_query_one(sql = "#[choice]", builder_setter = "consuming", builder_build_fallible = false)]
#[sql_fragment(choice, choice, (choice), {
    true => "select u.id, u.name from users",
    false => "select u.id, u.name from users"
})]
async fn conditional_bon(client: &Client, #[cond] choice: bool) -> Result<BonRow, QueryError> {}

mod runtime;

fn main() { runtime::verify(); }
