use crate::ast::Dialect;
use crate::context::BaseParseContext;
use crate::test_helpers::{TestParseContext, source_span};
use crate::{
    context::MutSourcePathWithSpan,
    sem::{ParseContext, SchemaFileContext, SchemaName, SelectStatement, TypeReference},
};

/// Uses only six explicit builtin types and small local DDL, without a shipped catalog.
pub(super) fn analyze(sql: &str, dialect: Dialect) -> (SelectStatement, SchemaFileContext) {
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
        context = crate::parse_statements(ddl, dialect)
            .next()
            .unwrap()
            .unwrap()
            .analyze(context)
            .unwrap()
            .into_context();
    }
    let analyzed = crate::parse_statements(sql, dialect)
        .next()
        .unwrap()
        .unwrap()
        .analyze(context)
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
