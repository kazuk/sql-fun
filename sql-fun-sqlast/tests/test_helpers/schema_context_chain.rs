use crate::test_helpers::heavy;

use ::sql_fun_sqlast::{
    ParsedSchemaDump, PostgresDialect,
    context::{BaseParseContext, FrozenBaseContext, ParseContext, SchemaFileContext, ViewInfoRead},
    names::{ColumnName, FullName, SchemaName, TableName},
    sem::{AnalysisProblem, TypeReference, ViewLike, problem::NameResolution},
};

pub(crate) const EXTENSION_A: &str = "CREATE SCHEMA a; CREATE TYPE a.state AS ENUM ('ready'); CREATE DOMAIN a.label AS text; \
     CREATE TYPE a.pair AS (key integer, value text); \
     CREATE TABLE a.items (id integer, label a.label); \
     CREATE FUNCTION a.choose(x integer) RETURNS integer LANGUAGE sql STRICT AS 'SELECT x'; \
     CREATE VIEW a.answers AS SELECT a.choose(t.id) AS answer, t.label FROM a.items t; \
     CREATE SEQUENCE a.item_seq; CREATE INDEX items_id_idx ON a.items (id);";

pub(crate) const EXTENSION_B: &str = "CREATE SCHEMA b; CREATE DOMAIN b.state_alias AS a.state; CREATE DOMAIN b.label_alias AS a.label; \
     CREATE TABLE b.items (id integer, label a.label, state b.state_alias); \
     CREATE FUNCTION a.choose(x text) RETURNS text LANGUAGE sql STRICT AS 'SELECT x'; \
     CREATE VIEW b.answers AS SELECT a.choose(t.id) AS answer, t.label FROM a.items t; \
     CREATE MATERIALIZED VIEW b.cached_answers AS SELECT answer FROM b.answers; \
     CREATE EXTENSION tablefunc;";

pub(crate) fn extension_sources(count: usize) -> &'static [&'static str] {
    &[EXTENSION_A, EXTENSION_B][..count]
}

pub(crate) fn dependent_dump(count: usize) -> &'static str {
    match count {
        0 => {
            "CREATE TABLE public.final_items (id integer); \
              CREATE VIEW public.final_answers AS SELECT abs(CAST(id AS double precision)) AS answer FROM public.final_items;"
        }
        1 => {
            "CREATE DOMAIN public.final_label AS a.label; \
              CREATE VIEW public.final_answers AS SELECT answer, label FROM a.answers;"
        }
        2 => {
            "CREATE DOMAIN public.final_state AS b.state_alias; \
              CREATE VIEW public.final_answers AS SELECT answer, label FROM b.answers;"
        }
        _ => panic!("unsupported fixture extension count"),
    }
}

pub(crate) fn schema_builder(
    base: &FrozenBaseContext,
) -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
    let (args, selected) = heavy::environment("17")?;
    Ok(SchemaFileContext::new(
        std::path::Path::new("array.sql"),
        base,
        &args,
        &selected,
    ))
}

pub(crate) fn load_schema(
    base: &FrozenBaseContext,
    sql: &str,
) -> Result<ParsedSchemaDump, Box<dyn std::error::Error>> {
    let file = ::tempfile::NamedTempFile::new()?;
    std::fs::write(file.path(), sql)?;
    Ok({
        let (args, selected) = heavy::environment("17")?;
        let analysis_base = base;
        PostgresDialect::parse_schema_file(file.path(), analysis_base, &mut None, &args, &selected)
    }?)
}

pub(crate) fn query(
    base: &FrozenBaseContext,
    sql: &str,
) -> Result<ViewLike, Box<dyn std::error::Error>> {
    let parsed = load_schema(
        base,
        &format!("CREATE VIEW public.acceptance_query AS {sql};"),
    )?;
    assert!(parsed.problems().is_empty());
    Ok(parsed
        .get_view_impl(&FullName::with_schema("public", "acceptance_query"))
        .expect("analyzed query view")
        .clone())
}

pub(crate) fn assert_query_type(query: &ViewLike, column: &str, expected: &TypeReference) {
    assert_eq!(
        query
            .get_column_def(&ColumnName::from(column))
            .expect("resolved query column")
            .get_type(),
        Some(expected)
    );
}

pub(crate) fn assert_type_exists(context: &impl BaseParseContext, name: &str) {
    let full_name = name.parse::<FullName>().expect("fixture type name");
    assert_eq!(
        context
            .get_type(&full_name)
            .expect("visible named type")
            .full_name(),
        Some(&full_name)
    );
}

pub(crate) fn assert_table_exists(context: &impl BaseParseContext, name: &str) {
    let table_name = TableName::from(name.parse::<FullName>().expect("fixture table name"));
    assert!(context.get_table(&table_name).is_some());
}

pub(crate) fn assert_missing_domain_base(problems: &[AnalysisProblem], domain: &str, base: &str) {
    let domain_name = domain.parse::<FullName>().expect("fixture domain name");
    assert!(problems.iter().any(|problem| match problem {
        AnalysisProblem::NameResolution(NameResolution::DomainTypeNameNotFound(problem)) =>
            problem.domain_type_name().as_str() == domain_name.to_string()
                && problem.base_type_name() == base,
        _ => false,
    }));
}

pub(crate) fn assert_missing_relation(problems: &[AnalysisProblem], relation: &str) {
    assert!(problems.iter().any(|problem| matches!(
        problem,
        AnalysisProblem::NameResolution(NameResolution::RelationNotFound(name))
            if name == relation
    )));
}

pub(crate) fn load_layer(
    base: &FrozenBaseContext,
    search_path: &[&str],
    sql: &str,
) -> Result<ParsedSchemaDump, Box<dyn std::error::Error>> {
    let file = ::tempfile::NamedTempFile::new()?;
    std::fs::write(file.path(), sql)?;
    let mut context = {
        let (args, selected) = heavy::environment("17")?;
        let analysis_base = base;
        SchemaFileContext::new(file.path(), analysis_base, &args, &selected)
    };
    let mut schemas = Vec::new();
    for schema in search_path {
        schemas.push(SchemaName::from(schema));
    }
    context.set_search_path(&schemas);
    Ok({
        let (args, selected) = heavy::environment("17")?;
        let analysis_base = &context.freeze();
        PostgresDialect::parse_schema_file(file.path(), analysis_base, &mut None, &args, &selected)
    }?)
}

pub(crate) fn assert_primary_key(context: &impl BaseParseContext, table: &str, key: Option<&str>) {
    let table_name = TableName::from(table.parse::<FullName>().expect("fixture table name"));
    let table = context.get_table(&table_name).expect("resolved table");
    if let Some(key) = key {
        let expected = ColumnName::from(key);
        assert_eq!(table.primary_key_columns(), vec![&expected]);
    } else {
        assert_eq!(table.primary_key_columns(), Vec::<&ColumnName>::new());
    }
}
