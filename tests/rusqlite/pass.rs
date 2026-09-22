mod parameter_source;

use rusqlite::Connection;

#[derive(derive_builder::Builder)]
struct User {
    id: i64,
    name: String,
}

impl User {
    fn builder() -> UserBuilder {
        UserBuilder::default()
    }
}

#[sql_fun::sql_query_one(datasource = "sqlite", sql = "select id, name from users where id = ${id}")]
fn find_user(connection: &Connection, id: i64) -> Result<User, anyhow::Error> {}

#[sql_fun::sql_query_opt(datasource = "sqlite", sql = "select id, name from users where id = ${id}")]
fn find_user_opt(connection: &Connection, id: i64) -> Result<Option<User>, anyhow::Error> {}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "select id, name from users")]
fn find_users<F>(
    connection: &Connection,
    mut collector: Vec<User>,
    handler: F,
) -> Result<Vec<User>, anyhow::Error>
where
    F: Fn(Vec<User>, User) -> Result<Vec<User>, anyhow::Error>,
{
}

#[sql_fun::sql_statement(datasource = "sqlite", sql = "delete from users where id = ${id}")]
fn delete_user(connection: &Connection, id: i64) -> Result<usize, anyhow::Error> {}



#[path = "../sql_functions/builders.rs"]
mod common_builders;
mod builders;
use self::builders::{MutableFallibleRow, QueryError};
use self::common_builders::{BonRow, ConsumingRow, MutableRow};
use std::sync::{Arc, Mutex, mpsc};


#[sql_fun::sql_query_one(datasource = "sqlite", sql = "select id, name from users", builder_setter = "mutable", builder_build_fallible = true)]
fn mutable_true_one_false(connection: &Connection) -> Result<MutableFallibleRow, QueryError>  {}

#[sql_fun::sql_query_opt(datasource = "sqlite", sql = "select id, name from users", builder_setter = "mutable", builder_build_fallible = true)]
fn mutable_true_opt_false(connection: &Connection) -> Result<Option<MutableFallibleRow>, QueryError>  {}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "select id, name from users", builder_setter = "mutable", builder_build_fallible = true)]
fn mutable_true_many_false<F>(connection: &Connection, mut collector: Vec<MutableFallibleRow>, handler: F) -> Result<Vec<MutableFallibleRow>, QueryError> where F: Fn(Vec<MutableFallibleRow>, MutableFallibleRow) -> Result<Vec<MutableFallibleRow>, QueryError> {}

#[sql_fun::sql_query_one(datasource = "sqlite", sql = "select id, name from users", builder_setter = "mutable", builder_build_fallible = true)]
fn mutable_true_one_true(connection: &Connection, #[interrupt_channel] interrupt: Arc<Mutex<mpsc::Receiver<QueryError>>>) -> Result<MutableFallibleRow, QueryError>  {}

#[sql_fun::sql_query_opt(datasource = "sqlite", sql = "select id, name from users", builder_setter = "mutable", builder_build_fallible = true)]
fn mutable_true_opt_true(connection: &Connection, #[interrupt_channel] interrupt: Arc<Mutex<mpsc::Receiver<QueryError>>>) -> Result<Option<MutableFallibleRow>, QueryError>  {}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "select id, name from users", builder_setter = "mutable", builder_build_fallible = true)]
fn mutable_true_many_true<F>(connection: &Connection, #[interrupt_channel] interrupt: Arc<Mutex<mpsc::Receiver<QueryError>>>, mut collector: Vec<MutableFallibleRow>, handler: F) -> Result<Vec<MutableFallibleRow>, QueryError> where F: Fn(Vec<MutableFallibleRow>, MutableFallibleRow) -> Result<Vec<MutableFallibleRow>, QueryError> {}

#[sql_fun::sql_query_one(datasource = "sqlite", sql = "select id, name from users", builder_setter = "mutable", builder_build_fallible = false)]
fn mutable_false_one_false(connection: &Connection) -> Result<MutableRow, QueryError>  {}

#[sql_fun::sql_query_opt(datasource = "sqlite", sql = "select id, name from users", builder_setter = "mutable", builder_build_fallible = false)]
fn mutable_false_opt_false(connection: &Connection) -> Result<Option<MutableRow>, QueryError>  {}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "select id, name from users", builder_setter = "mutable", builder_build_fallible = false)]
fn mutable_false_many_false<F>(connection: &Connection, mut collector: Vec<MutableRow>, handler: F) -> Result<Vec<MutableRow>, QueryError> where F: Fn(Vec<MutableRow>, MutableRow) -> Result<Vec<MutableRow>, QueryError> {}

#[sql_fun::sql_query_one(datasource = "sqlite", sql = "select id, name from users", builder_setter = "mutable", builder_build_fallible = false)]
fn mutable_false_one_true(connection: &Connection, #[interrupt_channel] interrupt: Arc<Mutex<mpsc::Receiver<QueryError>>>) -> Result<MutableRow, QueryError>  {}

#[sql_fun::sql_query_opt(datasource = "sqlite", sql = "select id, name from users", builder_setter = "mutable", builder_build_fallible = false)]
fn mutable_false_opt_true(connection: &Connection, #[interrupt_channel] interrupt: Arc<Mutex<mpsc::Receiver<QueryError>>>) -> Result<Option<MutableRow>, QueryError>  {}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "select id, name from users", builder_setter = "mutable", builder_build_fallible = false)]
fn mutable_false_many_true<F>(connection: &Connection, #[interrupt_channel] interrupt: Arc<Mutex<mpsc::Receiver<QueryError>>>, mut collector: Vec<MutableRow>, handler: F) -> Result<Vec<MutableRow>, QueryError> where F: Fn(Vec<MutableRow>, MutableRow) -> Result<Vec<MutableRow>, QueryError> {}

#[sql_fun::sql_query_one(datasource = "sqlite", sql = "select id, name from users", builder_setter = "consuming", builder_build_fallible = true)]
fn consuming_true_one_false(connection: &Connection) -> Result<ConsumingRow, QueryError>  {}

#[sql_fun::sql_query_opt(datasource = "sqlite", sql = "select id, name from users", builder_setter = "consuming", builder_build_fallible = true)]
fn consuming_true_opt_false(connection: &Connection) -> Result<Option<ConsumingRow>, QueryError>  {}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "select id, name from users", builder_setter = "consuming", builder_build_fallible = true)]
fn consuming_true_many_false<F>(connection: &Connection, mut collector: Vec<ConsumingRow>, handler: F) -> Result<Vec<ConsumingRow>, QueryError> where F: Fn(Vec<ConsumingRow>, ConsumingRow) -> Result<Vec<ConsumingRow>, QueryError> {}

#[sql_fun::sql_query_one(datasource = "sqlite", sql = "select id, name from users", builder_setter = "consuming", builder_build_fallible = true)]
fn consuming_true_one_true(connection: &Connection, #[interrupt_channel] interrupt: Arc<Mutex<mpsc::Receiver<QueryError>>>) -> Result<ConsumingRow, QueryError>  {}

#[sql_fun::sql_query_opt(datasource = "sqlite", sql = "select id, name from users", builder_setter = "consuming", builder_build_fallible = true)]
fn consuming_true_opt_true(connection: &Connection, #[interrupt_channel] interrupt: Arc<Mutex<mpsc::Receiver<QueryError>>>) -> Result<Option<ConsumingRow>, QueryError>  {}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "select id, name from users", builder_setter = "consuming", builder_build_fallible = true)]
fn consuming_true_many_true<F>(connection: &Connection, #[interrupt_channel] interrupt: Arc<Mutex<mpsc::Receiver<QueryError>>>, mut collector: Vec<ConsumingRow>, handler: F) -> Result<Vec<ConsumingRow>, QueryError> where F: Fn(Vec<ConsumingRow>, ConsumingRow) -> Result<Vec<ConsumingRow>, QueryError> {}

#[sql_fun::sql_query_one(datasource = "sqlite", sql = "select id, name from users", builder_setter = "consuming", builder_build_fallible = false)]
fn consuming_false_one_false(connection: &Connection) -> Result<BonRow, QueryError>  {}

#[sql_fun::sql_query_opt(datasource = "sqlite", sql = "select id, name from users", builder_setter = "consuming", builder_build_fallible = false)]
fn consuming_false_opt_false(connection: &Connection) -> Result<Option<BonRow>, QueryError>  {}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "select id, name from users", builder_setter = "consuming", builder_build_fallible = false)]
fn consuming_false_many_false<F>(connection: &Connection, mut collector: Vec<BonRow>, handler: F) -> Result<Vec<BonRow>, QueryError> where F: Fn(Vec<BonRow>, BonRow) -> Result<Vec<BonRow>, QueryError> {}

#[sql_fun::sql_query_one(datasource = "sqlite", sql = "select id, name from users", builder_setter = "consuming", builder_build_fallible = false)]
fn consuming_false_one_true(connection: &Connection, #[interrupt_channel] interrupt: Arc<Mutex<mpsc::Receiver<QueryError>>>) -> Result<BonRow, QueryError>  {}

#[sql_fun::sql_query_opt(datasource = "sqlite", sql = "select id, name from users", builder_setter = "consuming", builder_build_fallible = false)]
fn consuming_false_opt_true(connection: &Connection, #[interrupt_channel] interrupt: Arc<Mutex<mpsc::Receiver<QueryError>>>) -> Result<Option<BonRow>, QueryError>  {}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "select id, name from users", builder_setter = "consuming", builder_build_fallible = false)]
fn consuming_false_many_true<F>(connection: &Connection, #[interrupt_channel] interrupt: Arc<Mutex<mpsc::Receiver<QueryError>>>, mut collector: Vec<BonRow>, handler: F) -> Result<Vec<BonRow>, QueryError> where F: Fn(Vec<BonRow>, BonRow) -> Result<Vec<BonRow>, QueryError> {}

#[sql_fun::sql_query_one(datasource = "sqlite", sql = "#[choice]", builder_setter = "consuming", builder_build_fallible = false)]
#[sql_fragment(choice, choice, (choice), {
    true => "select id, name from users",
    false => "select id, name from users"
})]
fn conditional_bon(connection: &Connection, #[cond] choice: bool) -> Result<BonRow, QueryError> {}

fn main() {
    parameter_source::verify();
    if std::env::var("SQL_FUN_MODE").as_deref() == Ok("CHECK") {
        return;
    }
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE users (id INTEGER, name TEXT); INSERT INTO users VALUES (1, 'accepted');").unwrap();
    assert!(mutable_true_one_false(&connection).is_ok());
    assert!(mutable_true_opt_false(&connection).is_ok());
    assert!(mutable_true_many_false(&connection, Vec::new(), |mut rows, row| { rows.push(row); Ok(rows) }).is_ok());
    let (_sender, receiver) = mpsc::channel();
    assert!(mutable_true_one_true(&connection, Arc::new(Mutex::new(receiver))).is_ok());
    let (_sender, receiver) = mpsc::channel();
    assert!(mutable_true_opt_true(&connection, Arc::new(Mutex::new(receiver))).is_ok());
    let (_sender, receiver) = mpsc::channel();
    assert!(mutable_true_many_true(&connection, Arc::new(Mutex::new(receiver)), Vec::new(), |mut rows, row| { rows.push(row); Ok(rows) }).is_ok());
    assert!(mutable_false_one_false(&connection).is_ok());
    assert!(mutable_false_opt_false(&connection).is_ok());
    assert!(mutable_false_many_false(&connection, Vec::new(), |mut rows, row| { rows.push(row); Ok(rows) }).is_ok());
    let (_sender, receiver) = mpsc::channel();
    assert!(mutable_false_one_true(&connection, Arc::new(Mutex::new(receiver))).is_ok());
    let (_sender, receiver) = mpsc::channel();
    assert!(mutable_false_opt_true(&connection, Arc::new(Mutex::new(receiver))).is_ok());
    let (_sender, receiver) = mpsc::channel();
    assert!(mutable_false_many_true(&connection, Arc::new(Mutex::new(receiver)), Vec::new(), |mut rows, row| { rows.push(row); Ok(rows) }).is_ok());
    assert!(consuming_true_one_false(&connection).is_ok());
    assert!(consuming_true_opt_false(&connection).is_ok());
    assert!(consuming_true_many_false(&connection, Vec::new(), |mut rows, row| { rows.push(row); Ok(rows) }).is_ok());
    let (_sender, receiver) = mpsc::channel();
    assert!(consuming_true_one_true(&connection, Arc::new(Mutex::new(receiver))).is_ok());
    let (_sender, receiver) = mpsc::channel();
    assert!(consuming_true_opt_true(&connection, Arc::new(Mutex::new(receiver))).is_ok());
    let (_sender, receiver) = mpsc::channel();
    assert!(consuming_true_many_true(&connection, Arc::new(Mutex::new(receiver)), Vec::new(), |mut rows, row| { rows.push(row); Ok(rows) }).is_ok());
    assert!(consuming_false_one_false(&connection).is_ok());
    assert!(consuming_false_opt_false(&connection).is_ok());
    assert!(consuming_false_many_false(&connection, Vec::new(), |mut rows, row| { rows.push(row); Ok(rows) }).is_ok());
    let (_sender, receiver) = mpsc::channel();
    assert!(consuming_false_one_true(&connection, Arc::new(Mutex::new(receiver))).is_ok());
    let (_sender, receiver) = mpsc::channel();
    assert!(consuming_false_opt_true(&connection, Arc::new(Mutex::new(receiver))).is_ok());
    let (_sender, receiver) = mpsc::channel();
    assert!(consuming_false_many_true(&connection, Arc::new(Mutex::new(receiver)), Vec::new(), |mut rows, row| { rows.push(row); Ok(rows) }).is_ok());

    assert!(conditional_bon(&connection, true).is_ok());
    assert!(conditional_bon(&connection, false).is_ok());
    connection.execute_batch("UPDATE users SET name = 'reject';").unwrap();
    assert_eq!(consuming_true_one_false(&connection).unwrap_err().to_string(), "row rejected");
    assert_eq!(mutable_true_one_false(&connection).unwrap_err().to_string(), "row rejected");
    assert!(consuming_false_one_false(&connection).is_ok());
    connection.execute_batch("DELETE FROM users;").unwrap();
    assert!(consuming_false_one_false(&connection).is_err());
    assert!(consuming_false_opt_false(&connection).unwrap().is_none());
    connection.execute_batch("INSERT INTO users VALUES (1, 'a'), (2, 'b');").unwrap();
    assert!(consuming_false_one_false(&connection).is_err());
    assert!(consuming_false_opt_false(&connection).is_err());
}
