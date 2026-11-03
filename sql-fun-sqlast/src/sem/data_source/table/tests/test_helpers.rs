use crate::{
    ast::Dialect,
    context::{BaseParseContext, MutSourcePathWithSpan, SchemaFileContext},
    names::SchemaName,
    parse_statements,
    sem::{ParseContext, SelectStatement, TypeReference},
    test_helpers::{TestParseContext, create_cache_context, source_span},
};

/// Uses only six explicit builtin types and small local DDL, without a shipped catalog.
pub(super) fn analyze(sql: &str, dialect: Dialect) -> (SelectStatement, SchemaFileContext) {
    let mut cache_context = create_cache_context();
    let mut context = TestParseContext::default().with_dialect(dialect);
    context.set_get_search_path_result(&[SchemaName::from("public")]);
    for ty in ["int4", "bool", "oid", "tid", "xid", "cid"] {
        context.setup_type(TypeReference::from_master_type_name(ty));
    }
    let args = context.get_args().clone();
    let metadata = context.metadata().clone();
    let mut context = SchemaFileContext::new(
        std::path::Path::new("query.sql"),
        &context.freeze(),
        &args,
        &metadata,
    );
    context.set_source_path_with_span(&source_span());
    for ddl in [
        "CREATE TABLE t(id integer NOT NULL)",
        "CREATE TABLE u(id integer)",
        "CREATE VIEW v AS SELECT id FROM t",
    ] {
        context = parse_statements(ddl, dialect)
            .next()
            .unwrap()
            .unwrap()
            .analyze(context, &mut cache_context)
            .unwrap()
            .into_context();
    }
    let analyzed = parse_statements(sql, dialect)
        .next()
        .unwrap()
        .unwrap()
        .analyze(context, &mut cache_context)
        .unwrap();
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    (query, analyzed.into_context())
}

pub(super) fn assert_problem(context: &SchemaFileContext, code: &str) {
    for problem in context.problems() {
        if problem.code() == code {
            return;
        }
    }
    panic!("expected {code}: {:?}", context.problems());
}
