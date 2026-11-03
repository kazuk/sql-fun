pub(crate) fn postgres_drop_syntax(sql: &str) -> ::pg_query::protobuf::DropStmt {
    let result =
        ::pg_query::parse(sql, ::pg_query::ParserOptions::default()).expect("DROP syntax fixture");
    let node = result
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
        ::pg_query::protobuf::node::Node::DropStmt(value) => value,
        _ => panic!("DROP statement fixture"),
    }
}
