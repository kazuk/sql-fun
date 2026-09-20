//! `resqlite` code experiments
#![allow(dead_code)]
#![allow(unused_variables)]

use std::sync::{Arc, Mutex, mpsc::Receiver};

use rusqlite::Connection;

#[derive(derive_builder::Builder)]
#[expect(dead_code)]
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
    Sqlite(#[from] rusqlite::Error),

    #[error(transparent)]
    UserRow(#[from] UserRowBuilderError),

    #[error("interrupt disconnected")]
    InterruptDisconnected,
}

impl QueryError {
    const fn interrupt_disconnected() -> Self {
        Self::InterruptDisconnected
    }

    fn interrupt_handler_cleanup_error<T>(
        _e: rusqlite::Error,
        query_result: Result<T, Self>,
    ) -> Result<T, Self> {
        query_result
    }
}

// #[sql_fun::sql_query_one( sql="select id, first_name, last_name from users where id=${id}") ]
fn find_user_by_id(connection: &Connection, id: i64) -> Result<UserRow, QueryError> {
    const QUERY: &str = r"select id, first_name, last_name
           from users
           where id = ?1";
    const PARAMETER_COUNT: usize = 1;

    let mut stmt = connection.prepare(QUERY)?;
    let params: [&dyn ::rusqlite::ToSql; PARAMETER_COUNT] = [&id];

    let mut rows = stmt.query(params)?;
    let Some(row) = rows.next()? else {
        return Err(::rusqlite::Error::QueryReturnedNoRows.into());
    };
    let column_id: i64 = row.get(0)?;
    let column_first_name: String = row.get(1)?;
    let column_last_name: String = row.get(2)?;

    if rows.next()?.is_some() {
        Err(::rusqlite::Error::QueryReturnedMoreThanOneRow.into())
    } else {
        let result_row = UserRow::builder()
            .id(column_id)
            .first_name(column_first_name)
            .last_name(column_last_name)
            .build()?;

        Ok(result_row)
    }
}

fn find_user_by_id_with_interrupt(
    connection: &Connection,
    id: i64,
    interrupt: Arc<Mutex<Receiver<QueryError>>>,
) -> Result<UserRow, QueryError> {
    use std::sync::{Arc, Mutex, mpsc::TryRecvError};

    let interrupt_message: Arc<Mutex<Option<QueryError>>> = Arc::new(Mutex::new(None));
    let read_message = interrupt_message.clone();
    connection.progress_handler(
        1000,
        Some(
            move || match interrupt.lock().expect("mutex lockable").try_recv() {
                Ok(msg) => {
                    let mut msg_store = interrupt_message.lock().expect("mutex lockable");
                    *msg_store = Some(msg);
                    true
                }
                Err(TryRecvError::Empty) => false,
                Err(TryRecvError::Disconnected) => {
                    let mut msg_store = interrupt_message.lock().expect("mutex lockable");
                    *msg_store = Some(QueryError::interrupt_disconnected());
                    true
                }
            },
        ),
    )?;

    let result = find_user_by_id(connection, id);

    match connection.progress_handler(1000, None::<fn() -> bool>) {
        Ok(()) => {
            let mut msg_store = read_message.lock().expect("mutex lockable");
            let int_msg = msg_store.take();
            if let Some(interrupt_msg) = int_msg {
                return Err(interrupt_msg);
            }

            result
        }
        Err(e) => QueryError::interrupt_handler_cleanup_error(e, result),
    }
}
