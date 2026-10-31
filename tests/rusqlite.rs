//! rusqlite compilation test

use self::test_helpers::parenthesized_alias_database;

#[rstest::rstest]
#[case("select q.ax from (a) as q", true)]
#[case("select a.ax from (a) as q", false)]
#[case("select q.ax from ((a)) as q", true)]
#[case("select r.ax from ((a) as q) as r", true)]
#[case("select q.ax from ((a) as q) as r", false)]
#[case("select q.ax, q.bx from (a join b) as q", true)]
#[case("select a.ax, b.bx from (a join b) as q", true)]
#[case("select q.bx from (a, b) as q", true)]
#[case("select q.cx from (a join b join c) as q", true)]
#[case("select q.bx from ((a join b)) as q", true)]
#[case("select q.id from (a join b) as q", true)]
#[case("select q.id from (a join b using(id)) as q", true)]
#[case("select q.absent from (a join b) as q", false)]
#[case("select id from (a join b) as q", false)]
#[case("select q.* from (a join b) as q", false)]
fn bundled_sqlite_parenthesized_alias_visibility(
    #[case] sql: &str,
    #[case] valid: bool,
) -> Result<(), ::rusqlite::Error> {
    let database = parenthesized_alias_database()?;
    assert_eq!(database.prepare(sql).is_ok(), valid, "{sql}");
    Ok(())
}

#[test]
fn bundled_sqlite_group_alias_uses_first_matching_column_and_preserves_outer_join()
-> Result<(), ::rusqlite::Error> {
    let database = parenthesized_alias_database()?;
    let id: i64 = database.query_row("select q.id from (a join b) as q", [], |row| row.get(0))?;
    assert_eq!(id, 1);
    let right: Option<String> = database.query_row(
        "select q.bx from (a left join b on false) as q",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(right, None);
    Ok(())
}

#[rstest::rstest]
#[case("select q.id from (a right join b using(id)) as q order by b.id", true)]
#[case("select q.id from (a natural right join b) as q order by b.id", true)]
#[case(
    "select q.id from ((a join c using(id)) right join b using(id)) as q order by b.id",
    true
)]
#[case("select q.id from (a left join b using(id)) as q order by a.id", false)]
#[case("select q.id from (a natural left join b) as q order by a.id", false)]
fn bundled_sqlite_group_alias_preserves_join_columns(
    #[case] sql: &str,
    #[case] right: bool,
) -> Result<(), ::rusqlite::Error> {
    let database = parenthesized_alias_database()?;
    database
        .execute_batch("insert into a values(2, 8); insert into b values('3', 'unmatched');")?;
    let values = database
        .prepare(sql)?
        .query_map([], |row| row.get::<_, ::rusqlite::types::Value>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let expected = if right {
        vec![
            ::rusqlite::types::Value::from("1".to_string()),
            ::rusqlite::types::Value::from("3".to_string()),
        ]
    } else {
        vec![
            ::rusqlite::types::Value::from(1),
            ::rusqlite::types::Value::from(2),
        ]
    };
    assert_eq!(values, expected, "{sql}");
    Ok(())
}

#[test]
fn bundled_sqlite_group_alias_right_join_on_keeps_left_column() -> Result<(), ::rusqlite::Error> {
    let database = parenthesized_alias_database()?;
    database.execute_batch("insert into b values('3', 'unmatched');")?;
    let values = database
        .prepare("select q.id from (a right join b on a.id = b.id) as q order by b.id")?
        .query_map([], |row| row.get::<_, Option<i64>>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(values, vec![Some(1), None]);
    Ok(())
}

mod test_helpers {
    pub(super) fn parenthesized_alias_database() -> Result<::rusqlite::Connection, ::rusqlite::Error>
    {
        let database = ::rusqlite::Connection::open_in_memory()?;
        database.execute_batch(
            "create table a(id integer not null, ax integer not null);
             create table b(id text not null, bx text not null);
             create table c(id text not null, cx text not null);
             insert into a values(1, 7);
             insert into b values('1', 'right');
             insert into c values('1', 'third');",
        )?;
        Ok(database)
    }
}

// Timed-out test_rusqlite and test_rusqlite_check were deleted in caca9608;
// recover their implementations from that commit's parent.
