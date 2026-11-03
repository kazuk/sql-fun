//! Focused generated-code error and interruption contracts.

use crate::{
    builders::{MutableFallibleRow, QueryError},
    find_users,
};
use ::rusqlite::{Connection, Statement};
use std::{
    cell::Cell,
    sync::{Arc, Mutex, mpsc},
};

// The real driver cannot deterministically fail progress-handler cleanup.
// This small driver fixture evaluates the registered callback once and controls
// registration/cleanup errors without sleeps or a large running query.
struct InterruptConnection {
    database: Connection,
    registration_fails: bool,
    cleanup_fails: bool,
    interrupted: Cell<bool>,
    cleanups: Cell<usize>,
}

impl InterruptConnection {
    fn new(registration_fails: bool, cleanup_fails: bool) -> Self {
        let database = Connection::open_in_memory().unwrap();
        database.execute_batch("CREATE TABLE users (id INTEGER, name TEXT); INSERT INTO users VALUES (1, 'first'), (2, 'second');").unwrap();
        Self {
            database,
            registration_fails,
            cleanup_fails,
            interrupted: Cell::new(false),
            cleanups: Cell::new(0),
        }
    }
}

impl InterruptConnection {
    fn progress_handler<F: FnMut() -> bool + Send + 'static>(
        &self,
        _steps: i32,
        handler: Option<F>,
    ) -> Result<(), rusqlite::Error> {
        if let Some(mut handler) = handler {
            if self.registration_fails {
                return Err(rusqlite::Error::InvalidQuery);
            }
            self.interrupted.set(handler());
        } else {
            self.cleanups.set(self.cleanups.get() + 1);
            if self.cleanup_fails {
                return Err(rusqlite::Error::InvalidQuery);
            }
        }
        Ok(())
    }
}

impl InterruptConnection {
    fn prepare(&self, sql: &str) -> Result<Statement<'_>, rusqlite::Error> {
        if self.interrupted.get() {
            return Err(rusqlite::Error::InvalidQuery);
        }
        self.database.prepare(sql)
    }
}

#[sql_fun::sql_query_one(
    datasource = "sqlite",
    sql = "select id, name from users where id = ${id}"
)]
fn interrupted_row(
    connection: &InterruptConnection,
    id: i64,
    #[interrupt_channel] interrupt: Arc<Mutex<mpsc::Receiver<QueryError>>>,
) -> Result<MutableFallibleRow, QueryError> {
}

fn channel() -> (
    mpsc::Sender<QueryError>,
    Arc<Mutex<mpsc::Receiver<QueryError>>>,
) {
    let (sender, receiver) = mpsc::channel();
    (sender, Arc::new(Mutex::new(receiver)))
}

fn verify_interruption() {
    let connection = InterruptConnection::new(false, false);
    let (sender, receiver) = channel();
    assert!(interrupted_row(&connection, 1, receiver).is_ok());
    assert_eq!(connection.cleanups.get(), 1);
    // Keep sender alive to distinguish an empty channel from disconnection.
    drop(sender);

    let (sender, receiver) = channel();
    sender
        .send(QueryError::from(std::io::Error::other("cancelled")))
        .unwrap();
    assert_eq!(
        interrupted_row(&connection, 1, receiver)
            .unwrap_err()
            .to_string(),
        "cancelled"
    );
    assert_eq!(connection.cleanups.get(), 2);

    let (sender, receiver) = channel();
    drop(sender);
    assert_eq!(
        interrupted_row(&connection, 1, receiver)
            .unwrap_err()
            .to_string(),
        "interrupt disconnected"
    );
    assert_eq!(connection.cleanups.get(), 3);

    let (_sender, receiver) = channel();
    let error = interrupted_row(&connection, 9, receiver).unwrap_err();
    assert_eq!(
        error.to_string(),
        rusqlite::Error::QueryReturnedNoRows.to_string()
    );
    assert_eq!(connection.cleanups.get(), 4);

    let registration_failure = InterruptConnection::new(true, false);
    let (_sender, receiver) = channel();
    assert_eq!(
        interrupted_row(&registration_failure, 1, receiver)
            .unwrap_err()
            .to_string(),
        rusqlite::Error::InvalidQuery.to_string()
    );
    assert_eq!(registration_failure.cleanups.get(), 0);

    let cleanup_failure = InterruptConnection::new(false, true);
    let (_sender, receiver) = channel();
    assert_eq!(
        interrupted_row(&cleanup_failure, 1, receiver)
            .unwrap_err()
            .to_string(),
        rusqlite::Error::InvalidQuery.to_string()
    );
    assert_eq!(cleanup_failure.cleanups.get(), 1);
}

pub(super) fn verify() {
    if std::env::var("SQL_FUN_MODE").as_deref() == Ok("CHECK") {
        return;
    }
    verify_interruption();
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE users (id INTEGER, name TEXT); INSERT INTO users VALUES (1, 'first'), (2, 'second');").unwrap();
    let error = find_users(&connection, Vec::new(), |_, _| {
        Err(anyhow::anyhow!("handler failure"))
    })
    .err()
    .unwrap();
    assert_eq!(error.to_string(), "handler failure");
    let rows = find_users(&connection, Vec::new(), |mut rows, row| {
        rows.push(row);
        Ok(rows)
    })
    .unwrap();
    assert_eq!(rows.len(), 2);
}
