pub(super) mod heavy;

use crate::check::submission::{QueryAnalysis, submit};

use std::fmt::Write;

use ::serde_json::Value;
use ::sql_fun_client::TcpJsonRpcClientAsync;
use ::sql_fun_codegen::{QueryRequirement, QueryRequirements, SQL_FUN_CODEGEN_VERSION};
use ::sql_fun_core::{
    CteName, CteParameterCollection, PlaceholderedSql, ResolvedCteCatalog, SqlFunArgs,
};
use ::tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};

/// Small fixture-owned schema and datasource declarations, without catalog loading.
pub(super) struct Project(::tempfile::TempDir);

impl Project {
    pub(super) fn new() -> Self {
        let directory = ::tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("metadata.toml"),
            r#"
[package.metadata.database.first]
engine = "PostgreSQL"
dialect = "postgres"
version = "17"
search_path = ["public"]
connector = "tokio-postgres"
[package.metadata.database.second]
engine = "PostgreSQL"
dialect = "postgres"
version = "18"
search_path = ["other"]
connector = "tokio-postgres"
"#,
        )
        .unwrap();
        std::fs::write(
            directory.path().join("schema.sql"),
            "CREATE TABLE entries(id int4);",
        )
        .unwrap();
        Self(directory)
    }

    pub(super) fn enable_asterisk_expansion(&self) {
        let path = self.0.path().join("metadata.toml");
        let mut metadata = std::fs::read_to_string(&path).unwrap();
        metadata.push_str(
            "\n[package.metadata.database.first.rewrite_options]\nexpand_asterisk = true\n",
        );
        std::fs::write(path, metadata).unwrap();
    }

    pub(super) fn schema_hash(&self) -> String {
        ::sql_fun_core::Hasher::hash_file(self.0.path().join("schema.sql")).unwrap()
    }

    pub(super) fn write_lockfile(
        &self,
        datasource: &str,
        hash: &str,
        entries: &[::sql_fun_core::lockfile::LockfileEntry],
    ) {
        let mut document = ::sql_fun_core::lockfile::LockfileDocument::new(hash);
        for entry in entries {
            document.push(entry.clone());
        }
        let path = self.args().lockfile_path(datasource).unwrap();
        ::sql_fun_core::lockfile::LockfileDocument::replace(&path, &document.encode().unwrap())
            .unwrap();
    }

    pub(super) fn args(&self) -> SqlFunArgs {
        SqlFunArgs::builder(&std::collections::HashMap::new())
            .unwrap()
            .metadata_file(self.0.path().join("metadata.toml").to_str().unwrap())
            .sql_fun_home(self.0.path())
            .schema_file(self.0.path().join("schema.sql"))
            .build()
            .unwrap()
    }
}

/// A dependency-ordered catalog with a parameter shared across base and CTEs.
pub(super) fn catalog() -> ResolvedCteCatalog {
    let leaf = PlaceholderedSql::from_query("SELECT ${shared:int4} AS id");
    let outer = PlaceholderedSql::from_query("SELECT id FROM z_dependency");
    let mut parameters = CteParameterCollection::default();
    for (name, hints) in leaf.parameters() {
        parameters.push_query_parameter(&CteName::new("library::leaf"), name, hints);
    }
    let mut value = ::serde_json::to_value(ResolvedCteCatalog::default()).unwrap();
    value["parameters"] = ::serde_json::to_value(parameters).unwrap();
    value["cte_queries"] = ::serde_json::from_str(&format!(
        r#"{{"z_dependency":{},"a_visible":{}}}"#,
        ::serde_json::to_string(&leaf).unwrap(),
        ::serde_json::to_string(&outer).unwrap()
    ))
    .unwrap();
    ::serde_json::from_value(value).unwrap()
}

/// Model the expanded constants consumed by the public extraction API.
pub(super) fn requirements(
    datasources: &[&str],
    query: &str,
    catalog: &ResolvedCteCatalog,
) -> Vec<QueryRequirement> {
    let mut source = String::new();
    let encoded = ::serde_json::to_string(catalog).unwrap();
    for (index, datasource) in datasources.iter().enumerate() {
        ::std::write!(
            source,
            r#"
fn query_{index}() {{
    const __SQL_FUN_CODEGEN_VERSION: &str = {SQL_FUN_CODEGEN_VERSION:?};
    const __SQL_FUN_SOURCE_FILE: &str = "src/queries.rs";
    const __SQL_FUN_SOURCE_LINE: u32 = 42;
    const __SQL_FUN_DATASOURCE: &str = {datasource:?};
    const __SQL_FUN_QUERY: &str = {query:?};
    const __SQL_FUN_USING_CTE: &str = {encoded:?};
}}
"#,
        )
        .unwrap();
    }
    QueryRequirements::extract(&::syn::parse_file(&source).unwrap())
        .unwrap()
        .into_iter()
        .collect()
}

/// A recording peer using the production typed client, without server/catalog setup.
pub(super) async fn peer(
    fail_method: Option<&'static str>,
) -> (TcpJsonRpcClientAsync, ::tokio::task::JoinHandle<Vec<Value>>) {
    peer_with_gate(fail_method, None, None).await
}

/// Hold the second RPC until the first envelope has reached its consumer.
pub(super) async fn gated_second_failure(
    release: ::tokio::sync::oneshot::Receiver<()>,
) -> (TcpJsonRpcClientAsync, ::tokio::task::JoinHandle<Vec<Value>>) {
    peer_with_gate(Some("second_analysis"), Some(release), None).await
}

async fn peer_with_gate(
    fail_method: Option<&'static str>,
    mut release: Option<::tokio::sync::oneshot::Receiver<()>>,
    response: Option<Value>,
) -> (TcpJsonRpcClientAsync, ::tokio::task::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = ::tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut stream = BufReader::new(stream);
        let mut requests = Vec::new();
        let mut analyses = 0;
        loop {
            let mut line = String::new();
            if stream.read_line(&mut line).await.unwrap() == 0 {
                if fail_method == Some("finish") {
                    // Fail protocol completion only after accepted analyses were delivered.
                    stream
                        .get_mut()
                        .write_all(b"invalid completion JSON\n")
                        .await
                        .unwrap();
                }
                break;
            }
            let request: Value = ::serde_json::from_str(&line).unwrap();
            let method = request["method"].as_str().unwrap();
            if method == "analyze_query" {
                analyses += 1;
                if analyses == 2
                    && let Some(release) = release.take()
                {
                    release.await.unwrap();
                }
            }
            let second_failure = fail_method == Some("second_analysis") && analyses == 2;
            let response = if fail_method == Some(method) || second_failure {
                ::serde_json::json!({"jsonrpc":"2.0", "id":request["id"], "error":{"code":-32602,"message":"injected failure"}})
            } else {
                let result = match method {
                    "initialize_schema_context" if fail_method == Some("missing_handle") => {
                        ::serde_json::json!({"context_handle":null,"schema_dump_hash":"hash","problems":page()})
                    }
                    "initialize_schema_context" => {
                        ::serde_json::json!({"context_handle":format!("context-{}",request["params"]["datasource"].as_str().unwrap()),"schema_dump_hash":"hash","problems":page()})
                    }
                    "analyze_query" => response
                        .clone()
                        .unwrap_or_else(|| ::serde_json::json!({"queries":page()})),
                    "tokenize_sql" => {
                        let source = request["params"]["source"].as_str().unwrap();
                        if source.is_empty() {
                            page()
                        } else {
                            ::serde_json::json!({"handle":"tokens","total_length":1,"offset":0,
                                "require_release":false,
                                "items":[{"kind":"keyword","start":0,"end":source.len()}]})
                        }
                    }
                    _ => panic!("unexpected method: {method}"),
                };
                ::serde_json::json!({"jsonrpc":"2.0", "id":request["id"], "result":result})
            };
            requests.push(request);
            stream
                .get_mut()
                .write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
        }
        requests
    });
    (
        TcpJsonRpcClientAsync::try_from(TcpStream::connect(address).await.unwrap()).unwrap(),
        task,
    )
}

fn page() -> Value {
    ::serde_json::json!({"handle":"empty","total_length":0,"offset":0,"require_release":false,"items":[]})
}

/// A completed diagnostic collection for an analysis response fixture.
pub(super) fn diagnostic_page(problems: &[::sql_fun_client::payloads::AnalysisProbrem]) -> Value {
    ::serde_json::json!({"handle":"diagnostics","total_length":problems.len(),"offset":0,
        "require_release":false,"items":problems})
}

/// Collect channel items on the consumer side while preserving producer failures.
pub(super) async fn collect_submission(
    client: &mut TcpJsonRpcClientAsync,
    args: &SqlFunArgs,
    requirements: Vec<QueryRequirement>,
) -> Result<Vec<QueryAnalysis>, ::anyhow::Error> {
    let (sender, mut receiver) = ::tokio::sync::mpsc::channel(1);
    let producer = submit(client, args, requirements, sender);
    let consumer = async {
        let mut collected = Vec::new();
        while let Some(result) = receiver.recv().await {
            collected.push(result);
        }
        collected
    };
    let (completed, collected) = ::tokio::join!(producer, consumer);
    completed?;
    Ok(collected)
}

/// Use realistic completed statement DTOs with the existing recording peer.
pub(super) async fn peer_with_response(
    response: Value,
    fail: Option<&'static str>,
) -> (TcpJsonRpcClientAsync, ::tokio::task::JoinHandle<Vec<Value>>) {
    peer_with_gate(fail, None, Some(response)).await
}

pub(super) fn successful_response(statement_count: usize) -> Value {
    let statements = (0..statement_count).map(|index| ::serde_json::json!({
        "query_span":null, "execute_sql":format!("SELECT {index}"), "complete":true, "query_kind":"Select",
        "probrems":page(), "column_access":page(), "parameters":page(), "result_columns":page()
    })).collect::<Vec<_>>();
    let mut queries = page();
    queries["total_length"] = ::serde_json::json!(statement_count);
    queries["items"] = ::serde_json::json!(statements);
    ::serde_json::json!({"queries":queries})
}

/// A successful statement followed by an incomplete one, each with diagnostics.
pub(super) fn incomplete_diagnostic_response(statement: &str) -> Value {
    use ::sql_fun_client::payloads::{AnalysisProbrem, DiagnosticSeverity};
    use ::sql_fun_core::{
        DiagnosticPresentation, SqlDialect,
        analysis_problem::{AnalysisProblem, StringSpan},
    };

    let detailed = AnalysisProblem::domain_base_type_not_found(
        "example",
        "missing",
        StringSpan::new(statement.find("missing").unwrap(), 7),
    );
    let presentation =
        DiagnosticPresentation::new(&detailed, statement, SqlDialect::PostgreSQL, 10);
    let warning = AnalysisProblem::unreachable_coalesce_argument(
        ::sql_fun_core::analysis_problem::SemScalarExpr::new("literal", None, None),
        ::sql_fun_core::analysis_problem::SemScalarExpr::new("literal", None, Some(true)),
    );
    let advice = AnalysisProblem::parameter_constraint("consider a type hint");
    let fallback = AnalysisProblem::extension_not_found("missing_extension");
    let source = std::path::Path::new("<query>");
    let mut response = successful_response(2);
    response["queries"]["items"][0]["probrems"] = diagnostic_page(&[
        AnalysisProbrem::new(detailed.code(), DiagnosticSeverity::error(), source, None)
            .with_presentation(Some(&presentation)),
        AnalysisProbrem::new(warning.code(), DiagnosticSeverity::warning(), source, None)
            .with_semantic_problem(&warning),
        AnalysisProbrem::new(advice.code(), DiagnosticSeverity::advice(), source, None)
            .with_semantic_problem(&advice),
    ]);
    response["queries"]["items"][1]["complete"] = ::serde_json::json!(false);
    response["queries"]["items"][1]["execute_sql"] = Value::Null;
    response["queries"]["items"][1]["probrems"] = diagnostic_page(&[AnalysisProbrem::new(
        fallback.code(),
        DiagnosticSeverity::error(),
        source,
        None,
    )
    .with_semantic_problem(&fallback)]);
    response
}

/// A server response whose executable projection differs from its original star.
pub(super) fn expanded_projection_response(sql: &str) -> Value {
    use ::sql_fun_client::payloads::{
        Nullability, ParameterDescribe, ResultSetColumn, TypeReference,
    };
    let parameters = vec![ParameterDescribe::new(
        "lookup",
        1,
        Some(TypeReference::new("pg_catalog", "int4", false)),
        Vec::new(),
    )];
    let columns = vec![
        ResultSetColumn::new(
            0,
            "id",
            Some(TypeReference::new("pg_catalog", "int4", false)),
            Nullability::from_not_null(None),
        ),
        ResultSetColumn::new(
            1,
            "requested",
            Some(TypeReference::new("pg_catalog", "int4", false)),
            Nullability::from_not_null(None),
        ),
    ];
    let mut response = successful_response(1);
    let item = &mut response["queries"]["items"][0];
    item["execute_sql"] = ::serde_json::json!(sql);
    item["parameters"] = ::serde_json::json!({"handle":"bindings","total_length":parameters.len(),"offset":0,
        "require_release":false,"items":parameters});
    item["result_columns"] = ::serde_json::json!({"handle":"columns","total_length":columns.len(),"offset":0,
        "require_release":false,"items":columns});
    response
}
