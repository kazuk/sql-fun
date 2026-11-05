use crate::{
    ast::{CreateRule, CreateTableMode, Dialect, StatementKind, StatementNode},
    context::{BaseParseContext as _, FrozenBaseContext, MutSourcePathWithSpan, SchemaFileContext},
    names::{ColumnName, FullName, OperatorName, SchemaName, TableName},
    parse_statements,
    sem::{
        AnalysisError, BinaryOperatorDefinition, CastContext, CastDefinition, ColumnDefinition,
        CreateTable, CreateView, OperatorDefinition, PgBuiltInType, SemAst, ViewLike,
    },
    test_helpers::{
        TestParseContext, create_cache_context, source_span,
        statements::resolve_postgres as resolve,
    },
};

pub(crate) fn catalog_version(version: &str) -> TestParseContext {
    let metadata = ::serde_json::from_value(::serde_json::json!({
        "engine": "postgres", "dialect": "postgres", "version": version, "search_path": ["public"]
    }))
    .expect("explicit rule fixture version");
    catalog().with_metadata(&metadata)
}
pub(crate) fn catalog() -> TestParseContext {
    let mut context = TestParseContext::default().with_dialect(Dialect::postgres());
    context.set_get_search_path_result(&[SchemaName::from("public")]);
    MutSourcePathWithSpan::set_source_path_with_span(&mut context, &source_span());
    let int = PgBuiltInType::int4();
    context.set_get_implicit_cast_result(
        &int,
        &int,
        Some(CastDefinition::new(CastContext::NoConversion)),
    );
    let equality = OperatorDefinition::binary(&[BinaryOperatorDefinition::new(
        &int,
        &int,
        &PgBuiltInType::bool(),
        true,
    )]);
    context.set_get_operator_op_result(
        &OperatorName::from_parts(None, "="),
        true,
        true,
        Some(&equality),
    );
    for name in ["items", "sink"] {
        add_table(&mut context, &FullName::with_schema("public", name));
    }
    let query = resolve("SELECT id FROM items", &mut context)
        .unwrap()
        .into_select_statemtnt()
        .unwrap();
    let name = FullName::with_schema("public", "item_view");
    let view = CreateView::new(&name, &query, &[], &None, false);
    context.set_get_view_impl_result(&name, &Some(ViewLike::from(&view)));
    context
}
pub(crate) fn add_table(context: &mut TestParseContext, name: &FullName) {
    add_table_column(context, name, "id");
}
pub(crate) fn add_table_column(context: &mut TestParseContext, name: &FullName, column: &str) {
    let column = ColumnDefinition::new(
        &Some(ColumnName::from(column)),
        Some(&PgBuiltInType::int4()),
        Some(false),
        &[],
    );
    let table = CreateTable::new(
        &TableName::from(name.clone()),
        vec![column],
        CreateTableMode::from_if_not_exists(false),
        false,
        &source_span(),
    );
    context.set_get_table_impl_result(name, &Some(table));
}

pub(crate) fn layer(base: &FrozenBaseContext) -> SchemaFileContext {
    SchemaFileContext::new(
        std::path::Path::new("rule.sql"),
        base,
        base.get_args(),
        base.metadata(),
    )
}
pub(crate) fn apply(
    sql: &str,
    context: SchemaFileContext,
) -> Result<SchemaFileContext, AnalysisError> {
    Ok(SemAst::apply_ast_node(
        &parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .into_test_node(),
        context,
        &mut create_cache_context(),
    )?
    .into_context())
}

pub(crate) fn rule(sql: &str) -> CreateRule {
    let statement = parse_statements(sql, Dialect::postgres())
        .next()
        .expect("statement")
        .expect("context-free conversion");
    assert_eq!(statement.kind(), StatementKind::create_rule());
    match statement.into_test_node() {
        StatementNode::CreateRule(value) => value,
        _ => panic!("CREATE RULE node"),
    }
}

pub(crate) fn postgres_rule_syntax(sql: &str) -> ::pg_query::protobuf::RuleStmt {
    let node = ::pg_query::parse(sql, ::pg_query::ParserOptions::default())
        .expect("rule fixture")
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
        ::pg_query::protobuf::node::Node::RuleStmt(value) => *value,
        _ => panic!("RuleStmt fixture"),
    }
}
