use ::sql_fun_client::{
    Collection,
    payloads::{
        AnalysisProbrem, AnalyzeQueryItem, AnalyzeQueryResponse, DiagnosticSeverity, Nullability,
        ResultSetColumn, TypeReference,
    },
};

pub(super) fn response(codes: &[&str], columns: Vec<ResultSetColumn>) -> AnalyzeQueryResponse {
    let diagnostics = codes
        .iter()
        .map(|code| {
            AnalysisProbrem::new(
                code,
                DiagnosticSeverity::warning(),
                std::path::Path::new("query.sql"),
                None,
            )
        })
        .collect();
    AnalyzeQueryResponse::new(Collection::completed(vec![AnalyzeQueryItem::new(
        None,
        None,
        None,
        true,
        Collection::completed(diagnostics),
        Collection::completed(Vec::new()),
        Collection::completed(columns),
        Collection::completed(Vec::new()),
    )]))
}
pub(super) fn column(
    name: &str,
    typed: bool,
    array: bool,
    nullability: Option<bool>,
) -> ResultSetColumn {
    let column_type = if typed {
        Some(TypeReference::new("pg_catalog", "int4", array))
    } else {
        None
    };
    ResultSetColumn::new(
        0,
        name,
        column_type,
        Nullability::from_not_null(nullability),
    )
}
