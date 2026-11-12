//! Lightweight frontend fixtures for notification syntax contracts.

pub(crate) fn postgres_notify_syntax(sql: &str) -> ::pg_query::protobuf::NotifyStmt {
    let node = ::pg_query::parse(sql, ::pg_query::ParserOptions::default())
        .expect("NOTIFY fixture")
        .protobuf
        .stmts
        .into_iter()
        .next()
        .expect("statement")
        .stmt
        .expect("node")
        .node
        .expect("payload");
    match node {
        ::pg_query::protobuf::node::Node::NotifyStmt(value) => value,
        _ => panic!("NOTIFY fixture"),
    }
}
