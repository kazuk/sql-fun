//! Small project schema and explicit lint declarations, without builtin loading.
use crate::{
    AstAndContextPair, StringSpan,
    ast::Dialect,
    context::{MutSourcePathWithSpan, SchemaFileContext},
    parse_statements,
    sem::{AnalysisError, SemAst, SourcePathWithSpan},
    test_helpers::{create_cache_context, indirection},
};
use ::sql_fun_core::LintConfigurations;

pub(crate) fn configuration(text: &str) -> LintConfigurations {
    let file = ::tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), text).unwrap();
    LintConfigurations::load_from(file.path()).unwrap()
}
pub(crate) fn schema() -> SchemaFileContext {
    indirection::schema(
        r#"CREATE TABLE public.items (id integer PRIMARY KEY, legacy text);
CREATE VIEW public.old_view AS SELECT id, legacy FROM public.items;
CREATE DOMAIN public.legacy_domain AS integer;
CREATE TYPE public.mood AS ENUM ('happy', 'sad');
CREATE FUNCTION public.legacy(integer) RETURNS integer LANGUAGE SQL AS 'SELECT $1';
CREATE FUNCTION public.legacy(text) RETURNS text LANGUAGE SQL AS 'SELECT $1';"#,
        true,
    )
    .unwrap()
}
pub(crate) fn analyze(
    sql: &str,
    declarations: &str,
) -> Result<AstAndContextPair<SchemaFileContext>, AnalysisError> {
    let mut context = schema();
    context.set_source_path_with_span(&SourcePathWithSpan::new(
        "query.sql",
        &StringSpan::new(0, sql.len()),
    ));
    context.set_lint_configuration(&configuration(declarations));
    let statement = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap();
    SemAst::from_ast(&statement, context, &mut create_cache_context())
}

pub(crate) fn target(kind: &str, name: &str, column: Option<&str>) -> String {
    let mut declaration = format!(
        r#"[[lint.deprecated]]
kind = {kind:?}
name = ["public", {name:?}]
expect = true
"#
    );
    if let Some(column) = column {
        declaration.push_str(&format!("column = {column:?}\n"));
    }
    declaration
}

pub(crate) fn duplicate_targets(first: &str, second: &str) -> String {
    let mut declarations = String::new();
    for (name, expect) in [(r#"["items"]"#, first), (r#"["public", "items"]"#, second)] {
        declarations.push_str(&format!(
            r#"[[lint.deprecated]]
kind = "table"
name = {name}
"#
        ));
        if !expect.is_empty() {
            declarations.push_str(&format!("expect = {expect}\n"));
        }
    }
    declarations
}
