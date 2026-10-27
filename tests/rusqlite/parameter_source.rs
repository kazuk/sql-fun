//! Execute SQLite source bindings through every query shape and fragment path.

use super::{User, builders::QueryError};
use std::sync::{Arc, Mutex, mpsc};
use ::rusqlite::Connection;

struct Dto {
    id: i64,
    name: String,
}

impl Dto {
    fn new(id: &i64, name: &str) -> Self {
        Self { id: *id, name: name.to_owned() }
    }

    fn id(&self) -> i64 { self.id }
    fn name(&self) -> &str { &self.name }
    // This fixture intentionally exercises the user-configurable getter prefix.
    fn get_id(&self) -> i64 { self.id }
    fn mode(&self) -> bool { self.id > 0 }
}

#[sql_fun::sql_statement(datasource = "sqlite", sql = "insert into users (id, name) values (${id}, ${name})")]
fn insert(connection: &Connection, #[parameter_source] dto: &Dto) -> Result<usize, anyhow::Error> {}

#[sql_fun::sql_statement(datasource = "sqlite", sql = "insert into users (id, name) values (${id}, ${name})")]
fn insert_fields(connection: &Connection, #[parameter_source(field_bind = true)] dto: Dto) -> Result<usize, anyhow::Error> {}

#[sql_fun::sql_query_one(datasource = "sqlite", sql = "select id, name from users where id = ${id}")]
fn lookup(connection: &Connection, #[parameter_source] dto: &Dto) -> Result<User, anyhow::Error> {}

#[sql_fun::sql_query_opt(datasource = "sqlite", sql = "select id, name from users where id = ${id}")]
fn lookup_opt(connection: &Connection, #[parameter_source(getter_prefix = "get_", field_bind = false)] dto: &Dto) -> Result<Option<User>, anyhow::Error> {}

#[sql_fun::sql_query_many(datasource = "sqlite", sql = "select id, name from users where id = ${id}")]
fn lookup_many<F>(connection: &Connection, #[parameter_source(field_bind = false)] dto: &Dto, mut rows: Vec<User>, handler: F) -> Result<Vec<User>, anyhow::Error>
where F: Fn(Vec<User>, User) -> Result<Vec<User>, anyhow::Error> {}

#[sql_fun::sql_statement(datasource = "sqlite", sql = "delete from users where id = ${id}")]
fn direct_wins(connection: &Connection, #[parameter_source] dto: &str, id: i64) -> Result<usize, anyhow::Error> {}

#[sql_fun::sql_statement(datasource = "sqlite", sql = "delete from users where id = ${id}")]
fn interrupted(connection: &Connection, #[parameter_source] dto: &Dto, #[interrupt_channel] interrupt: Arc<Mutex<mpsc::Receiver<QueryError>>>) -> Result<usize, QueryError> {}

#[sql_fun::sql_statement(datasource = "sqlite", sql = "#[choice]")]
#[sql_fragment(choice, dto.mode(), (), {
    true => "delete from users where id = ${id}",
    false => "delete from users where id = ${id}"
})]
fn retained_source(connection: &Connection, #[parameter_source] dto: &Dto) -> Result<usize, anyhow::Error> {}

#[sql_fun::sql_statement(datasource = "sqlite", sql = "#[choice]")]
#[sql_fragment(choice, selected, (selected), {
    #[args(id: i64)]
    Some(id) => "delete from users where id = ${id}",
    None => "delete from users where id = ${id}"
})]
fn introduced_argument(connection: &Connection, #[parameter_source] dto: &Dto, #[cond] selected: Option<i64>) -> Result<usize, anyhow::Error> {}

#[sql_fun::sql_statement(datasource = "sqlite", sql = "#[choice]")]
#[sql_fragment(choice, dto, (dto), {
    #[args(id: i64)]
    id => "delete from users where id = ${id}"
})]
fn consumed_source(connection: &Connection, #[cond] #[parameter_source] dto: i64) -> Result<usize, anyhow::Error> {}

pub(super) fn verify() {
    if std::env::var("SQL_FUN_MODE").as_deref() == Ok("CHECK") {
        return;
    }
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE users (id INTEGER, name TEXT);").unwrap();
    let keyword_dto = KeywordDto { r#type: 1, self_: 2 };
    insert(&connection, &Dto::new(&3, "keyword getter")).unwrap();
    assert_eq!(keyword_getters(&connection, &keyword_dto).unwrap(), 1);
    insert(&connection, &Dto::new(&3, "keyword field")).unwrap();
    assert_eq!(keyword_fields(&connection, &keyword_dto).unwrap(), 1);
    insert(&connection, &Dto::new(&3, "keyword prefix")).unwrap();
    assert_eq!(keyword_prefix(&connection, &keyword_dto).unwrap(), 1);
    let dto = Dto::new(&1, "first");
    assert_eq!(insert(&connection, &dto).unwrap(), 1);
    assert_eq!(lookup(&connection, &dto).unwrap().name, "first");
    assert_eq!(lookup_opt(&connection, &dto).unwrap().unwrap().id, 1);
    let rows = lookup_many(&connection, &dto, Vec::new(), |mut collector, row| {
        collector.push(row);
        Ok(collector)
    }).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "first");

    assert_eq!(insert_fields(&connection, Dto::new(&2, "second")).unwrap(), 1);
    assert_eq!(introduced_argument(&connection, &dto, Some(2)).unwrap(), 1);
    assert!(lookup_opt(&connection, &Dto::new(&2, "unused")).unwrap().is_none());
    assert!(lookup_opt(&connection, &dto).unwrap().is_some());
    assert_eq!(introduced_argument(&connection, &dto, None).unwrap(), 1);

    insert(&connection, &dto).unwrap();
    assert_eq!(retained_source(&connection, &dto).unwrap(), 1);
    insert(&connection, &dto).unwrap();
    assert_eq!(consumed_source(&connection, 1).unwrap(), 1);
    insert(&connection, &dto).unwrap();
    assert_eq!(direct_wins(&connection, "no id getter", 1).unwrap(), 1);
    insert(&connection, &dto).unwrap();
    let (_sender, receiver) = mpsc::channel();
    assert_eq!(interrupted(&connection, &dto, Arc::new(Mutex::new(receiver))).unwrap(), 1);
}

struct KeywordDto {
    r#type: i64,
    self_: i64,
}

impl KeywordDto {
    fn r#type(&self) -> i64 { self.r#type }
    fn self_(&self) -> i64 { self.self_ }
    // Exercise a configured prefix before keyword conversion.
    fn get_type(&self) -> i64 { self.r#type }
    fn get_self(&self) -> i64 { self.self_ }
}

#[sql_fun::sql_statement(datasource = "sqlite", sql = "delete from users where id = ${type} + ${self}")]
fn keyword_getters(connection: &Connection, #[parameter_source] dto: &KeywordDto) -> Result<usize, anyhow::Error> {}

#[sql_fun::sql_statement(datasource = "sqlite", sql = "delete from users where id = ${type} + ${self}")]
fn keyword_fields(connection: &Connection, #[parameter_source(field_bind = true)] dto: &KeywordDto) -> Result<usize, anyhow::Error> {}

#[sql_fun::sql_statement(datasource = "sqlite", sql = "delete from users where id = ${type} + ${self}")]
fn keyword_prefix(connection: &Connection, #[parameter_source(getter_prefix = "get_")] dto: &KeywordDto) -> Result<usize, anyhow::Error> {}
