use ::sql_fun_client::{
    Collection,
    payloads::{
        AnalyzeQueryItem, AnalyzeQueryResponse, ColumnAccess, Nullability, QueryKind,
        ResultSetColumn, TypeReference,
    },
};

pub(super) fn column_access_response() -> ::testresult::TestResult<AnalyzeQueryResponse> {
    let access: ColumnAccess = ::serde_json::from_value(::serde_json::json!({
        "relation":{"catalog":null,"schema":"public","name":"events"},
        "relation_kind":"Table", "column_name":"id", "access_kind":"SelectOutput"
    }))?;
    let columns = vec![
        ResultSetColumn::new(
            1,
            "ids",
            Some(TypeReference::new("pg_catalog", "int4", true)),
            Nullability::from_not_null(Some(false)),
        ),
        ResultSetColumn::new(0, "id", None, Nullability::from_not_null(None)),
    ];
    Ok(AnalyzeQueryResponse::new(Collection::completed(vec![
        AnalyzeQueryItem::new(
            None,
            Some(QueryKind::Select),
            None,
            false,
            Collection::completed(Vec::new()),
            Collection::completed(Vec::new()),
            Collection::completed(columns),
            Collection::completed(vec![access]),
        ),
    ])))
}
