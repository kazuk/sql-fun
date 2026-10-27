use ::sql_fun_client::{
    Collection,
    payloads::{AnalyzeQueryItem, Nullability, ParameterDescribe, ResultSetColumn, TypeReference},
};

pub(super) fn statement(
    complete: bool,
    parameters: &[(u32, &str)],
    columns: &[(u32, Option<TypeReference>)],
) -> AnalyzeQueryItem {
    let mut bindings = Vec::new();
    for &(index, name) in parameters {
        bindings.push(ParameterDescribe::new(
            name,
            index,
            Some(TypeReference::new("pg_catalog", "int4", false)),
            Vec::new(),
        ));
    }
    let mut outputs = Vec::new();
    for (ordinal, sql_type) in columns {
        let sql_type = sql_type.as_ref().map(copy_type_reference);
        outputs.push(ResultSetColumn::new(
            *ordinal,
            "column",
            sql_type,
            Nullability::from_not_null(None),
        ));
    }
    AnalyzeQueryItem::new(
        None,
        None,
        Some("SELECT $3, $1".to_owned()),
        complete,
        Collection::completed(Vec::new()),
        Collection::completed(bindings),
        Collection::completed(outputs),
        Collection::completed(Vec::new()),
        Collection::completed(Vec::new()),
    )
}

fn copy_type_reference(value: &TypeReference) -> TypeReference {
    TypeReference::new(value.schema_name(), value.local_name(), *value.is_array())
}
