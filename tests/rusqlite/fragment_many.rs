use super::User;

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "#[choice]")]
#[sql_fragment(choice, choice, (choice), {
    (0, _, _) => "select id, name from users",
    #[args(id: i64)]
    (1, id, _) => "select id, name from users where id = ${id}",
    #[args(id: i64, extra: i64)]
    (_, id, extra) => "select id, name from users where id = ${id}"
})]
fn varying<F>(
    connection: &::rusqlite::Connection,
    #[cond] choice: (i64, i64, i64),
    mut collector: Vec<User>,
    handler: F,
) -> Result<Vec<User>, ::anyhow::Error>
where
    F: Fn(Vec<User>, User) -> Result<Vec<User>, ::anyhow::Error>,
{
}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "#[outer]#[inner]")]
#[sql_fragment(outer, choice, (choice), {
    #[args(selected: i64, extra: i64)]
    (selected, extra) => ""
})]
#[sql_fragment(inner, selected, (selected), {
    #[args(id: i64)]
    id => "select id, name from users where id = ${id}"
})]
fn nested<F>(
    connection: &::rusqlite::Connection,
    #[cond] choice: (i64, i64),
    mut collector: Vec<User>,
    handler: F,
) -> Result<Vec<User>, ::anyhow::Error>
where
    F: Fn(Vec<User>, User) -> Result<Vec<User>, ::anyhow::Error>,
{
}

pub(super) fn verify() {
    if std::env::var("SQL_FUN_MODE").as_deref() == Ok("CHECK") {
        return;
    }
    let connection = ::rusqlite::Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE users (id INTEGER, name TEXT); INSERT INTO users VALUES (1, 'first'), (2, 'second');").unwrap();
    verify_collection(
        |rows, handler| varying(&connection, (0, 0, 0), rows, handler),
        &[1, 2],
    );
    verify_collection(
        |rows, handler| varying(&connection, (1, 2, 0), rows, handler),
        &[2],
    );
    verify_collection(
        |rows, handler| varying(&connection, (2, 1, 99), rows, handler),
        &[1],
    );
    verify_collection(
        |rows, handler| nested(&connection, (2, 99), rows, handler),
        &[2],
    );
    verify_collection(
        |rows, handler| nested(&connection, (3, 99), rows, handler),
        &[],
    );
}

fn verify_collection(
    query: impl FnOnce(
        Vec<User>,
        &dyn Fn(Vec<User>, User) -> Result<Vec<User>, ::anyhow::Error>,
    ) -> Result<Vec<User>, ::anyhow::Error>,
    expected: &[i64],
) {
    let observed = std::cell::RefCell::new(Vec::new());
    let initial = User::builder()
        .id(0)
        .name("initial".to_owned())
        .build()
        .unwrap();
    let rows = query(vec![initial], &|mut rows, row| {
        observed.borrow_mut().push(row.id);
        rows.push(row);
        Ok(rows)
    })
    .unwrap();
    assert_eq!(rows[0].name, "initial");
    let mut ids: Vec<_> = rows.into_iter().skip(1).map(|row| row.id).collect();
    assert_eq!(ids, *observed.borrow());
    ids.sort();
    assert_eq!(ids, expected);
}
