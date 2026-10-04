//! Regression fixtures for generated local identifier hygiene.

use super::User;
use super::common_builders::BonRow;
use std::{
    cell::{Cell, RefCell},
    sync::{Arc, Mutex, mpsc},
};

use ::rusqlite::Connection;

/// Keeps the fixture compatible with both SQLite and derive-builder errors.
#[derive(Debug)]
struct HygieneError(anyhow::Error);

impl From<rusqlite::Error> for HygieneError {
    fn from(error: rusqlite::Error) -> Self {
        Self(error.into())
    }
}

impl From<super::UserBuilderError> for HygieneError {
    fn from(error: super::UserBuilderError) -> Self {
        Self(error.into())
    }
}

impl HygieneError {
    fn interrupt_disconnected() -> Self {
        Self(anyhow::anyhow!("interrupt disconnected"))
    }

    fn interrupt_handler_cleanup_error<T>(
        error: rusqlite::Error,
        _result: Result<T, Self>,
    ) -> Result<T, Self> {
        Err(Self::from(error))
    }
}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "select id, name from users")]
fn legacy<F>(
    params: &Connection,
    mut rows: Vec<User>,
    builder: F,
) -> Result<Vec<User>, anyhow::Error>
where
    F: Fn(Vec<User>, User) -> Result<Vec<User>, anyhow::Error>,
{
}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "select id, name from users")]
fn result_collector<F>(
    rows: &Connection,
    mut result: Vec<User>,
    statement: F,
) -> Result<Vec<User>, anyhow::Error>
where
    F: Fn(Vec<User>, User) -> Result<Vec<User>, anyhow::Error>,
{
}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "select id, name from users")]
fn builder_collector<F>(
    statement: &Connection,
    mut builder: Vec<User>,
    result: F,
) -> Result<Vec<User>, anyhow::Error>
where
    F: Fn(Vec<User>, User) -> Result<Vec<User>, anyhow::Error>,
{
}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "select id, name from users")]
fn candidate<F>(
    __sql_fun_rows_1: &Connection,
    mut __sql_fun_rows: Vec<User>,
    __sql_fun_rows_2: F,
) -> Result<Vec<User>, anyhow::Error>
where
    F: Fn(Vec<User>, User) -> Result<Vec<User>, anyhow::Error>,
{
}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "select id, name from users")]
fn raw_candidate<F>(
    params: &Connection,
    mut r#__sql_fun_rows: Vec<User>,
    __sql_fun_rows_1: F,
) -> Result<Vec<User>, anyhow::Error>
where
    F: Fn(Vec<User>, User) -> Result<Vec<User>, anyhow::Error>,
{
}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "select id, name from users")]
fn interrupted<F>(
    params: &Connection,
    #[interrupt_channel] interrupt_message: Arc<Mutex<mpsc::Receiver<HygieneError>>>,
    mut rows: Vec<User>,
    result: F,
) -> Result<Vec<User>, HygieneError>
where
    F: Fn(Vec<User>, User) -> Result<Vec<User>, HygieneError>,
{
}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "select id, name from users")]
fn wrapper<F>(
    query_result: &Connection,
    #[interrupt_channel] interrupt_message: Arc<Mutex<mpsc::Receiver<HygieneError>>>,
    mut read_message: Vec<User>,
    statement: F,
) -> Result<Vec<User>, HygieneError>
where
    F: Fn(Vec<User>, User) -> Result<Vec<User>, HygieneError>,
{
}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "select id, name from users")]
fn candidate_interrupted<F>(
    __sql_fun_read_message: &Connection,
    #[interrupt_channel] __sql_fun_interrupt_message: Arc<Mutex<mpsc::Receiver<HygieneError>>>,
    mut __sql_fun_rows: Vec<User>,
    __sql_fun_builder: F,
) -> Result<Vec<User>, HygieneError>
where
    F: Fn(Vec<User>, User) -> Result<Vec<User>, HygieneError>,
{
}

#[sql_fun::sql_query_many(
    datasource = "sqlite",
    sql = "select id, name from users",
    builder_setter = "consuming",
    builder_build_fallible = false
)]
fn consuming<F>(
    params: &Connection,
    mut builder: Vec<BonRow>,
    result: F,
) -> Result<Vec<BonRow>, anyhow::Error>
where
    F: Fn(Vec<BonRow>, BonRow) -> Result<Vec<BonRow>, anyhow::Error>,
{
}

#[sql_fun::sql_query_one(datasource = "sqlite", sql = "#[choice]")]
#[sql_fragment(choice, choice, (choice), {
    #[args(id: i64, __sql_fun_result: i64)]
    (id, __sql_fun_result) => "select id, name from users where id = ${id}"
})]
fn fragment(params: &Connection, #[cond] choice: (i64, i64)) -> Result<User, anyhow::Error> {}

fn verify_query<E: std::fmt::Debug>(
    query: impl Fn(Vec<User>, &dyn Fn(Vec<User>, User) -> Result<Vec<User>, E>) -> Result<Vec<User>, E>,
    expected_ids: &[i64],
) {
    let calls = Cell::new(0);
    let observed = RefCell::new(Vec::new());
    let initial = User::builder()
        .id(0)
        .name("initial".to_owned())
        .build()
        .unwrap();
    let rows = query(vec![initial], &|mut rows, row| {
        calls.set(calls.get() + 1);
        observed.borrow_mut().push(row.id);
        rows.push(row);
        Ok(rows)
    })
    .unwrap();
    assert_eq!(calls.get(), expected_ids.len());
    assert_eq!(rows[0].name, "initial");
    let ids: Vec<_> = rows.into_iter().map(|row| row.id).collect();
    assert_eq!(&ids[1..], observed.borrow().as_slice());
    let mut actual = ids[1..].to_vec();
    actual.sort();
    assert_eq!(actual, expected_ids);
}

pub(super) fn verify() {
    if std::env::var("SQL_FUN_MODE").as_deref() == Ok("CHECK") {
        return;
    }
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE users (id INTEGER, name TEXT); INSERT INTO users VALUES (2, 'second'), (1, 'first');").unwrap();
    verify_query(|rows, handler| legacy(&connection, rows, handler), &[1, 2]);
    verify_query(
        |rows, handler| result_collector(&connection, rows, handler),
        &[1, 2],
    );
    verify_query(
        |rows, handler| builder_collector(&connection, rows, handler),
        &[1, 2],
    );
    verify_query(
        |rows, handler| candidate(&connection, rows, handler),
        &[1, 2],
    );
    verify_query(
        |rows, handler| raw_candidate(&connection, rows, handler),
        &[1, 2],
    );
    {
        let (_sender, receiver) = mpsc::channel();
        let interrupt = Arc::new(Mutex::new(receiver));
        verify_query(
            |rows, handler| interrupted(&connection, interrupt.clone(), rows, handler),
            &[1, 2],
        );
    }
    {
        let (_sender, receiver) = mpsc::channel();
        let interrupt = Arc::new(Mutex::new(receiver));
        verify_query(
            |rows, handler| wrapper(&connection, interrupt.clone(), rows, handler),
            &[1, 2],
        );
    }
    {
        let (_sender, receiver) = mpsc::channel();
        let interrupt = Arc::new(Mutex::new(receiver));
        verify_query(
            |rows, handler| candidate_interrupted(&connection, interrupt.clone(), rows, handler),
            &[1, 2],
        );
    }
    assert_eq!(fragment(&connection, (1, 7)).unwrap().id, 1);
    assert_eq!(one(&connection, 1).unwrap().id, 1);
    assert_eq!(optional(&connection, 1).unwrap().unwrap().id, 1);
    assert_eq!(statement(&connection, 7).unwrap(), 0);
    let rows = consuming(&connection, Vec::new(), |mut rows, row| {
        rows.push(row);
        Ok(rows)
    })
    .unwrap();
    assert_eq!(rows.len(), 2);
    connection.execute_batch("DELETE FROM users;").unwrap();
    verify_query(|rows, handler| legacy(&connection, rows, handler), &[]);
}

#[sql_fun::sql_query_one(
    datasource = "sqlite",
    sql = "select id, name from users where id = ${id}"
)]
fn one(__sql_fun_params: &Connection, id: i64) -> Result<User, anyhow::Error> {}

#[sql_fun::sql_query_opt(
    datasource = "sqlite",
    sql = "select id, name from users where id = ${id}"
)]
fn optional(params: &Connection, id: i64) -> Result<Option<User>, anyhow::Error> {}

#[sql_fun::sql_statement(datasource = "sqlite", sql = "delete from users where id = ${id}")]
fn statement(__sql_fun_params: &Connection, id: i64) -> Result<usize, anyhow::Error> {}
