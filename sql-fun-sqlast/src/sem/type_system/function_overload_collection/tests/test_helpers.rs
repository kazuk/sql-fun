use crate::{
    AstAndContextPair,
    ast::Dialect,
    context::{BaseParseContext, SchemaFileContext},
    parse_statements,
    sem::{CastContext, CastDefinition, ParseContext, SchemaName, TypeReference},
    test_helpers::TestParseContext,
};

// Small explicit type/cast universe. No builtin metadata is loaded.
fn context() -> SchemaFileContext {
    let mut base = TestParseContext::default().with_dialect(Dialect::postgres());
    base.set_get_search_path_result(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    for name in [
        "int2",
        "int2vector",
        "oidvector",
        "oid",
        "int4",
        "int8",
        "float8",
        "text",
        "bool",
        "record",
        "name",
        "any",
        "anyelement",
        "anyarray",
        "anynonarray",
        "anycompatible",
        "anycompatiblearray",
        "anycompatiblenonarray",
    ] {
        let ty = TypeReference::from_master_type_name(name);
        base.setup_type(ty.clone());
        base.add_implicit_castable(&ty, &ty);
    }
    for (vector, element) in [("int2vector", "int2"), ("oidvector", "oid")] {
        let vector = TypeReference::from_master_type_name(vector);
        let element = TypeReference::from_master_type_name(element);
        base.set_array_element(&vector, &element);
        let array = vector.array_type();
        base.set_get_implicit_cast_result(
            &array,
            &array,
            Some(CastDefinition::new(CastContext::no_conversion())),
        );
        base.set_get_implicit_cast_result(
            &vector,
            &element.array_type(),
            Some(CastDefinition::new(CastContext::implicit())),
        );
    }
    for (source, target) in [("int2", "int4"), ("int2", "int8"), ("int4", "int8")] {
        base.add_implicit_castable(
            &TypeReference::from_master_type_name(source),
            &TypeReference::from_master_type_name(target),
        );
        base.set_get_implicit_cast_result(
            &TypeReference::from_master_type_name(source),
            &TypeReference::from_master_type_name(target),
            Some(CastDefinition::new(CastContext::implicit())),
        );
    }
    for ty in [
        TypeReference::from_master_type_name("int4").array_type(),
        TypeReference::from_master_type_name("int2").array_type(),
        TypeReference::from_full_name(crate::sem::FullName::with_schema("public", "control")),
        TypeReference::from_full_name(crate::sem::FullName::with_schema("public", "control"))
            .array_type(),
    ] {
        base.set_get_implicit_cast_result(
            &ty,
            &ty,
            Some(CastDefinition::new(CastContext::no_conversion())),
        );
    }
    base.set_get_implicit_cast_result(
        &TypeReference::from_master_type_name("int4"),
        &TypeReference::from_master_type_name("int2"),
        Some(CastDefinition::new(CastContext::assignment())),
    );
    base.set_get_implicit_cast_result(
        &TypeReference::from_master_type_name("int4"),
        &TypeReference::from_master_type_name("float8"),
        Some(CastDefinition::new(CastContext::implicit_preferred())),
    );
    let args = base.get_args().clone();
    let metadata = base.metadata().clone();
    SchemaFileContext::new(
        std::path::Path::new("query.sql"),
        &base.freeze(),
        &args,
        &metadata,
    )
}

pub(super) fn analyze(
    query: &str,
    polymorphic_parameters: &str,
    concrete_parameters: &str,
    reversed: bool,
) -> Result<AstAndContextPair<SchemaFileContext>, Box<dyn std::error::Error>> {
    let mut cache_context = crate::CacheContext::new();
    let mut context = context();
    let polymorphic = format!(
        "CREATE FUNCTION f({polymorphic_parameters}) RETURNS text LANGUAGE sql AS 'SELECT NULL::text'"
    );
    let concrete = format!(
        "CREATE FUNCTION f({concrete_parameters}) RETURNS integer LANGUAGE sql AS 'SELECT 1'"
    );
    let functions = if reversed {
        [concrete, polymorphic]
    } else {
        [polymorphic, concrete]
    };
    let schema = format!(
        "CREATE TYPE control AS (id bigint); {}; {};
         CREATE FUNCTION accept_text(text) RETURNS text LANGUAGE sql AS 'SELECT NULL::text';
         CREATE FUNCTION accept_integer(integer) RETURNS integer LANGUAGE sql AS 'SELECT 1'",
        functions[0], functions[1],
    );
    for statement in parse_statements(&schema, Dialect::postgres()) {
        context = statement?
            .analyze(context, &mut cache_context)?
            .into_context();
    }
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    Ok(parse_statements(query, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(context, &mut cache_context)?)
}

pub(super) fn analyze_vector(
    query: &str,
    parameters: &str,
    result: &str,
) -> Result<AstAndContextPair<SchemaFileContext>, Box<dyn std::error::Error>> {
    let mut cache_context = crate::CacheContext::new();
    let mut context = context();
    let definition =
        format!("CREATE FUNCTION vf({parameters}) RETURNS {result} LANGUAGE sql AS ''");
    context = parse_statements(&definition, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(context, &mut cache_context)?
        .into_context();
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    Ok(parse_statements(query, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(context, &mut cache_context)?)
}

/// A format-like builtin signature in the small explicit catalog.
pub(super) fn analyze_variadic_any(
    query: &str,
) -> Result<AstAndContextPair<SchemaFileContext>, Box<dyn std::error::Error>> {
    let mut cache_context = crate::CacheContext::new();
    let mut context = context();
    for statement in parse_statements(
        r#"CREATE FUNCTION format(text) RETURNS text LANGUAGE internal AS 'text_format';
           CREATE FUNCTION format(text, VARIADIC "any") RETURNS text LANGUAGE internal AS 'text_format';"#,
        Dialect::postgres(),
    ) {
        context = statement?
            .analyze(context, &mut cache_context)?
            .into_context();
    }
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    Ok(parse_statements(query, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(context, &mut cache_context)?)
}
