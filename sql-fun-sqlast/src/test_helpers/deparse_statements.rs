use crate::ast::Dialect;
use crate::parse_statements;

pub(crate) fn assert_native_deparse_round_trip(sql: &str) {
    let statement = parse_statements(sql, Dialect::postgres())
        .next()
        .expect("one statement")
        .expect(sql);
    let emitted = statement
        .deparse_sql(Dialect::postgres())
        .expect("statement emits");
    let expected = pg_query::parse(sql, ::pg_query::ParserOptions::default())
        .expect(sql)
        .deparse(::pg_query::DeparseOptions::default())
        .expect("native deparse");
    let actual = pg_query::parse(&emitted, ::pg_query::ParserOptions::default())
        .expect(&emitted)
        .deparse(::pg_query::DeparseOptions::default())
        .expect("emitted native deparse");
    assert_eq!(expected, actual, "emitted SQL: {emitted}");
}
