//! Compile source bindings through every PostgreSQL query shape and fragment path.

use super::IdWithName;
use ::std::future::Future;
use ::tokio_postgres::Client;

struct Dto {
    id: i64,
    name: String,
}

impl Dto {
    fn id(&self) -> i64 { self.id }
    fn name(&self) -> &str { &self.name }
    // This fixture intentionally exercises the user-configurable getter prefix.
    fn get_id(&self) -> i64 { self.id }
    fn mode(&self) -> bool { self.id > 0 }
}

#[sql_fun::sql_statement(sql = "insert into users (id, name) values (${id}, ${name})")]
async fn insert(client: &Client, #[parameter_source] dto: &Dto) -> Result<u64, anyhow::Error> {}

#[sql_fun::sql_statement(sql = "insert into users (id, name) values (${id}, ${name})")]
async fn insert_fields(client: &Client, #[parameter_source(field_bind = true)] dto: Dto) -> Result<u64, anyhow::Error> {}

#[sql_fun::sql_query_one(sql = "select id, name from users where id = ${id}")]
async fn lookup(client: &Client, #[parameter_source] dto: &Dto) -> Result<IdWithName, anyhow::Error> {}

#[sql_fun::sql_query_opt(sql = "select id, name from users where id = ${id}")]
async fn lookup_opt(client: &Client, #[parameter_source(getter_prefix = "get_", field_bind = false)] dto: &Dto) -> Result<Option<IdWithName>, anyhow::Error> {}

#[sql_fun::sql_query_many(sql = "select id, name from users where id = ${id}")]
async fn lookup_many<F, Fut>(client: &Client, #[parameter_source(field_bind = false)] dto: &Dto, mut collector: Vec<IdWithName>, handler: F) -> Result<Vec<IdWithName>, anyhow::Error>
where F: Fn(Vec<IdWithName>, IdWithName) -> Fut, Fut: Future<Output = Result<Vec<IdWithName>, anyhow::Error>> {}

#[sql_fun::sql_statement(sql = "delete from users where id = ${id}")]
async fn direct_wins(client: &Client, #[parameter_source] dto: &str, id: i64) -> Result<u64, anyhow::Error> {}

#[sql_fun::sql_statement(sql = "delete from users where id = ${id}")]
async fn interrupted(client: &Client, #[parameter_source] dto: &Dto, #[interrupt_future] interrupt: &mut (impl Future<Output = tokio_postgres::Error> + Unpin)) -> Result<u64, tokio_postgres::Error> {}

#[sql_fun::sql_statement(sql = "#[choice]")]
#[sql_fragment(choice, dto.mode(), (), {
    true => "delete from users where id = ${id}",
    false => "delete from users where id = ${id}"
})]
async fn retained_source(client: &Client, #[parameter_source] dto: &Dto) -> Result<u64, anyhow::Error> {}

#[sql_fun::sql_statement(sql = "#[choice]")]
#[sql_fragment(choice, selected, (selected), {
    #[args(id: i64)]
    Some(id) => "delete from users where id = ${id}",
    None => "delete from users where id = ${id}"
})]
async fn introduced_argument(client: &Client, #[parameter_source] dto: &Dto, #[cond] selected: Option<i64>) -> Result<u64, anyhow::Error> {}

#[sql_fun::sql_statement(sql = "#[choice]")]
#[sql_fragment(choice, dto, (dto), {
    #[args(id: i64)]
    id => "delete from users where id = ${id}"
})]
async fn consumed_source(client: &Client, #[cond] #[parameter_source] dto: i64) -> Result<u64, anyhow::Error> {}
