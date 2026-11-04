use crate::{
    ast::Dialect,
    context::BaseParseContext,
    names::{FullName, OperatorName, SchemaName},
};

pub fn expand_search_path_inst<TBaseParseContext, TResult, F>(
    context: &TBaseParseContext,
    key: &FullName,
    getter: F,
) -> Option<TResult>
where
    TBaseParseContext: BaseParseContext + ?Sized,
    F: Fn(&FullName) -> Option<TResult>,
{
    expand(context, key, getter, false, false)
}

/// Relations use temporary schemas and share one name-shadowing domain.
pub(crate) fn expand_relation_search_path<TBaseParseContext, TResult, F>(
    context: &TBaseParseContext,
    key: &FullName,
    getter: F,
) -> Option<TResult>
where
    TBaseParseContext: BaseParseContext + ?Sized,
    F: Fn(&FullName) -> Option<TResult>,
{
    expand(context, key, getter, true, true)
}

/// Types search temporary schemas, independently of relation-name occupancy.
pub(crate) fn expand_type_search_path<TBaseParseContext, TResult, F>(
    context: &TBaseParseContext,
    key: &FullName,
    getter: F,
) -> Option<TResult>
where
    TBaseParseContext: BaseParseContext + ?Sized,
    F: Fn(&FullName) -> Option<TResult>,
{
    expand(context, key, getter, true, false)
}

fn expand<TBaseParseContext, TResult, F>(
    context: &TBaseParseContext,
    key: &FullName,
    getter: F,
    temporary: bool,
    relation: bool,
) -> Option<TResult>
where
    TBaseParseContext: BaseParseContext + ?Sized,
    F: Fn(&FullName) -> Option<TResult>,
{
    if key.schema().is_some() {
        return getter(key);
    }
    for schema in search_schemas(context, temporary) {
        let name = key.replace_schema(&schema);
        let result = getter(&name);
        if result.is_some() || (relation && context.object_kind(&name).is_some()) {
            return result;
        }
    }
    None
}

fn search_schemas(context: &(impl BaseParseContext + ?Sized), temporary: bool) -> Vec<SchemaName> {
    let postgres = Dialect::from(context.metadata()) == Dialect::postgres();
    let temp = SchemaName::from(if postgres { "pg_temp" } else { "temp" });
    let catalog = SchemaName::from("pg_catalog");
    let path = context.get_search_path();
    let mut schemas = Vec::new();
    if temporary && !path.contains(&temp) {
        schemas.push(temp.clone());
    }
    if postgres && !path.contains(&catalog) {
        schemas.push(catalog);
    }
    for schema in path {
        // PostgreSQL never searches the temporary schema for routines.
        if temporary || !postgres || schema != &temp {
            schemas.push(schema.clone());
        }
    }
    schemas
}

/// Operators share routine search-path rules; SQLite operator tokens are local.
pub(crate) fn expand_operator_search_path<TContext, TResult, F>(
    context: &TContext,
    name: &OperatorName,
    getter: F,
) -> Option<TResult>
where
    TContext: BaseParseContext + ?Sized,
    F: Fn(&OperatorName) -> Option<TResult>,
{
    if !name.is_valid() {
        return None;
    }
    if Dialect::from(context.metadata()) == Dialect::sqlite() {
        return getter(name);
    }
    expand_search_path_inst(context, &name.full_name()?, |key| {
        getter(&OperatorName::from_full_name(key))
    })
}
