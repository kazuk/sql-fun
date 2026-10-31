use crate::ast::Dialect;

fn normalize_locations(value: &mut ::serde_json::Value) {
    match value {
        ::serde_json::Value::Object(fields) => {
            for field in [
                "list_start",
                "list_end",
                "rexpr_list_start",
                "rexpr_list_end",
                "arg_location",
                "payload_location",
                "conninfo_location",
            ] {
                fields.remove(field);
            }
            fields.remove("location");
            fields.remove("name_location");
            fields.remove("stmt_location");
            fields.remove("stmt_len");
            // PostgreSQL 18 marks special SET spellings for query fingerprinting;
            // equivalent emitted SET syntax has the same execution semantics.
            fields.remove("jumble_args");
            for child in fields.values_mut() {
                normalize_locations(child);
            }
        }
        ::serde_json::Value::Array(values) => {
            for child in values {
                normalize_locations(child);
            }
        }
        _ => {}
    }
}

pub(crate) fn assert_postgres_structure(sql: &str) {
    let rendered = crate::parse_statements(sql, Dialect::postgres())
        .next()
        .expect("statement")
        .expect(sql)
        .deparse_sql(Dialect::postgres())
        .expect(sql);
    let mut original = ::serde_json::to_value(
        ::pg_query::parse(sql, ::pg_query::ParserOptions::default())
            .expect(sql)
            .protobuf,
    )
    .unwrap();
    let mut emitted = ::serde_json::to_value(
        ::pg_query::parse(&rendered, ::pg_query::ParserOptions::default())
            .expect(&rendered)
            .protobuf,
    )
    .unwrap();
    normalize_locations(&mut original);
    normalize_locations(&mut emitted);
    assert_eq!(original, emitted, "{sql}\n=> {rendered}");
}
