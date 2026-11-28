use crate::{
    ast::Dialect,
    context::{BaseParseContext, ParseContext},
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

/// Select an existing schema for a PostgreSQL declaration.
pub(crate) fn postgres_creation_schema(
    context: &impl BaseParseContext,
    requested: Option<&SchemaName>,
) -> Option<SchemaName> {
    let exists = |schema: &SchemaName| {
        matches!(
            schema.as_str(),
            "public" | "pg_catalog" | "information_schema"
        ) || context.schema_definition(schema.as_str()).is_some()
    };

    if let Some(schema) = requested {
        if exists(schema) {
            Some(schema.clone())
        } else {
            None
        }
    } else {
        context
            .get_search_path()
            .iter()
            .find(|schema| exists(schema))
            .cloned()
    }
}

pub(crate) fn search_schemas(
    context: &(impl BaseParseContext + ?Sized),
    temporary: bool,
) -> Vec<SchemaName> {
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
    operator_search_keys(context, name).iter().find_map(getter)
}

pub(crate) fn operator_search_keys(
    context: &(impl BaseParseContext + ?Sized),
    name: &OperatorName,
) -> Vec<OperatorName> {
    let Some(full_name) = name.full_name() else {
        return Vec::new();
    };
    if full_name.schema().is_some() || Dialect::from(context.metadata()) == Dialect::sqlite() {
        return vec![name.clone()];
    }
    search_schemas(context, false)
        .into_iter()
        .map(|schema| OperatorName::from_full_name(&full_name.replace_schema(&schema)))
        .collect()
}

/// Failure to select the local database namespace.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum DatabaseNameError<'a> {
    #[error("invalid qualified name")]
    InvalidName,
    #[error("cross-database reference to related database {0} is not supported")]
    Related(&'a str),
    #[error("unknown database {0}")]
    Unknown(&'a str),
}

pub(crate) fn postgres_database<'a>(
    database: Option<&'a str>,
    context: &impl ParseContext,
) -> Result<(), DatabaseNameError<'a>> {
    let Some(database) = database else {
        return Ok(());
    };
    let metadata = context.metadata();
    if metadata.database_name() == Some(database) {
        Ok(())
    } else if metadata
        .related_database_names()
        .iter()
        .any(|name| name == database)
    {
        Err(DatabaseNameError::Related(database))
    } else {
        Err(DatabaseNameError::Unknown(database))
    }
}

pub(crate) fn postgres_local_name<'a>(
    parts: &'a [String],
    context: &impl ParseContext,
) -> Result<FullName, DatabaseNameError<'a>> {
    match parts {
        [local] => Ok(FullName::from_local_name(local)),
        [schema, local] => Ok(FullName::with_schema(schema, local)),
        [database, schema, local] => {
            postgres_database(Some(database), context)?;
            Ok(FullName::with_schema(schema, local))
        }
        _ => Err(DatabaseNameError::InvalidName),
    }
}
