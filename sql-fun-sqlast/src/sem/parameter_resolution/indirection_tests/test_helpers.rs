use crate::{
    StringSpan,
    ast::Dialect,
    context::{BaseParseContext, MutSourcePathWithSpan, SchemaFileContext},
    names::SchemaName,
    parse_statements,
    sem::{
        CastContext, CastDefinition, InputTypeConstraint, ParseContext, PgBuiltInType,
        SourcePathWithSpan,
    },
    test_helpers::{TestParseContext, create_cache_context},
};

/// An in-memory base with four scalar types, one assignment cast, and integer
/// addition. No builtin catalog or function/operator metadata is loaded.
pub(super) fn context() -> SchemaFileContext {
    let mut base = TestParseContext::default().with_dialect(Dialect::postgres());
    base.set_get_search_path_result(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    for ty in [
        PgBuiltInType::int4(),
        PgBuiltInType::numeric(),
        PgBuiltInType::bool(),
        PgBuiltInType::text(),
    ] {
        base.setup_type(ty);
    }
    base.set_get_implicit_cast_result(
        &PgBuiltInType::numeric(),
        &PgBuiltInType::int4(),
        Some(CastDefinition::new(CastContext::assignment())),
    );
    base.setup_binary("+", "int4", "int4", "int4", true);
    let args = base.get_args().clone();
    let metadata = base.metadata().clone();
    SchemaFileContext::new(
        std::path::Path::new("query.sql"),
        &base.freeze(),
        &args,
        &metadata,
    )
}

pub(super) fn schema(
    sql: &str,
    frozen: bool,
) -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
    let mut cache_context = create_cache_context();
    let mut context = context();
    context.set_source_path_with_span(&SourcePathWithSpan::new(
        "schema.sql",
        &StringSpan::new(0, sql.len()),
    ));
    for statement in parse_statements(sql, Dialect::postgres()) {
        context = statement?
            .analyze(context, &mut cache_context)?
            .into_context();
    }
    if frozen {
        let args = context.get_args().clone();
        let metadata = context.metadata().clone();
        context = SchemaFileContext::new(
            std::path::Path::new("query.sql"),
            &context.freeze(),
            &args,
            &metadata,
        );
    }
    Ok(context)
}

pub(super) fn serialized_parameter_domains(
    value: &::serde_json::Value,
) -> Vec<InputTypeConstraint> {
    let mut domains = Vec::new();
    match value {
        ::serde_json::Value::Object(fields) => {
            if let Some(parameter) = fields.get("Param") {
                domains.push(
                    ::serde_json::from_value(parameter["constraint"].clone())
                        .expect("serialized input domain"),
                );
            }
            for child in fields.values() {
                domains.extend(serialized_parameter_domains(child));
            }
        }
        ::serde_json::Value::Array(values) => {
            for child in values {
                domains.extend(serialized_parameter_domains(child));
            }
        }
        _ => {}
    }
    domains
}
