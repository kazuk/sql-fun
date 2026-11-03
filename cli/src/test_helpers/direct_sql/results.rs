use ::sql_fun_client::{
    Collection,
    payloads::{
        AccessKind, AnalyzeQueryItem, AnalyzeQueryResponse, ColumnAccess, Nullability, ObjectKind,
        RelationName, ResultSetColumn, TypeReference,
    },
};

pub(crate) fn column_access_response() -> AnalyzeQueryResponse {
    let relation = RelationName::new(None, Some("public"), "events");
    let qualified = RelationName::new(Some("app"), Some("public"), "events");
    let accesses = vec![
        ColumnAccess::new(
            AccessKind::select_output(),
            ObjectKind::table(),
            &relation,
            Some("id"),
        ),
        ColumnAccess::new(
            AccessKind::select_filter(),
            ObjectKind::table(),
            &relation,
            Some("id"),
        ),
        ColumnAccess::new(
            AccessKind::read_rows(),
            ObjectKind::table(),
            &relation,
            None,
        ),
        ColumnAccess::new(
            AccessKind::select_output(),
            ObjectKind::table(),
            &qualified,
            Some("id"),
        ),
    ];
    let columns = vec![
        ResultSetColumn::new(
            1,
            "ids",
            Some(TypeReference::new("pg_catalog", "int4", true)),
            Nullability::from_not_null(Some(false)),
        ),
        ResultSetColumn::new(0, "id", None, Nullability::from_not_null(None)),
    ];
    AnalyzeQueryResponse::new(Collection::completed(vec![
        AnalyzeQueryItem::new(
            None,
            None,
            None,
            false,
            Collection::completed(Vec::new()),
            Collection::completed(Vec::new()),
            Collection::completed(columns),
            Collection::completed(accesses),
        ),
        AnalyzeQueryItem::new(
            None,
            None,
            None,
            true,
            Collection::completed(Vec::new()),
            Collection::completed(Vec::new()),
            Collection::completed(Vec::new()),
            Collection::completed(Vec::new()),
        ),
    ]))
}
