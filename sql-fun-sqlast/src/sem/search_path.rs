use crate::context::BaseParseContext;
use crate::sem::{FullName, SchemaName};

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
    let postgres = crate::ast::Dialect::from(context.metadata()) == crate::ast::Dialect::postgres();
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
