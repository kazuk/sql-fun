use crate::IdWithName;

#[sql_fun::sql_query_many(sql = "#[choice]")]
#[sql_fragment(choice, choice, (choice), {
    (0, _, _) => "select u.id, u.name from users AS u",
    #[args(id: i64)]
    (1, id, _) => "select id, name from users where id = ${id}",
    #[args(id: i64, extra: i64)]
    (_, id, extra) => "select id, name from users where id = ${id}"
})]
async fn varying<F, Fut>(
    client: &::tokio_postgres::Client,
    #[cond] choice: (i64, i64, i64),
    mut collector: Vec<IdWithName>,
    handler: F,
) -> Result<Vec<IdWithName>, ::anyhow::Error>
where
    F: Fn(Vec<IdWithName>, IdWithName) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<IdWithName>, ::anyhow::Error>>,
{
}

#[sql_fun::sql_query_many(sql = "#[outer]#[inner]")]
#[sql_fragment(outer, choice, (choice), {
    #[args(selected: i64, extra: i64)]
    (selected, extra) => ""
})]
#[sql_fragment(inner, selected, (selected), {
    #[args(id: i64)]
    id => "select id, name from users where id = ${id}"
})]
async fn nested<F, Fut>(
    client: &::tokio_postgres::Client,
    #[cond] choice: (i64, i64),
    mut collector: Vec<IdWithName>,
    handler: F,
) -> Result<Vec<IdWithName>, ::anyhow::Error>
where
    F: Fn(Vec<IdWithName>, IdWithName) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<IdWithName>, ::anyhow::Error>>,
{
}
