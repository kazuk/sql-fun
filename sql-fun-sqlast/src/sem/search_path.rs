use crate::sem::{BaseParseContext, FullName, SchemaName};

pub fn expand_search_path<'context, TBaseParseContext, TResult, F>(
    context: &'context TBaseParseContext,
    key: &FullName,
    getter: F,
) -> Option<&'context TResult>
where
    TBaseParseContext: BaseParseContext + ?Sized,
    F: Fn(&FullName) -> Option<&'context TResult>,
{
    if let Some(_schema) = key.schema() {
        getter(key)
    } else {
        let search_path = context.get_search_path();
        let pg_catalog_schema = SchemaName::from("pg_catalog");
        if !search_path.contains(&pg_catalog_schema) {
            let name = key.replace_schema(&pg_catalog_schema);
            if let Some(td) = getter(&name) {
                return Some(td);
            }
        }

        for schema in search_path {
            let name = key.replace_schema(schema);
            if let Some(td) = getter(&name) {
                return Some(td);
            }
        }
        None
    }
}

pub fn expand_search_path_inst<TBaseParseContext, TResult, F>(
    context: &TBaseParseContext,
    key: &FullName,
    getter: F,
) -> Option<TResult>
where
    TBaseParseContext: BaseParseContext + ?Sized,
    F: Fn(&FullName) -> Option<TResult>,
{
    if let Some(_schema) = key.schema() {
        getter(key)
    } else {
        let search_path = context.get_search_path();
        let pg_catalog_schema = SchemaName::from("pg_catalog");
        if !search_path.contains(&pg_catalog_schema) {
            let name = key.replace_schema(&pg_catalog_schema);
            if let Some(td) = getter(&name) {
                return Some(td);
            }
        }

        for schema in search_path {
            let name = key.replace_schema(schema);
            if let Some(td) = getter(&name) {
                return Some(td);
            }
        }
        None
    }
}
