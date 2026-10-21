pub(super) mod heavy;

use std::fmt::Write;

use ::serde_json::{Value, json};
use ::sql_fun_client::TcpJsonRpcClientAsync;
use ::sql_fun_codegen::{QueryRequirement, QueryRequirements};
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
    let base = PlaceholderedSql::from_query("SELECT ${shared:int4}, id FROM a_visible");
    let leaf = PlaceholderedSql::from_query("SELECT ${shared:int4} AS id");
    let outer = PlaceholderedSql::from_query("SELECT id FROM z_dependency");
    let mut parameters = CteParameterCollection::default();
    for (name, hints) in leaf.parameters() {
        parameters.push_query_parameter(&CteName::new("library::leaf"), name, hints);
    }
    let mut value = ::serde_json::to_value(ResolvedCteCatalog::from(&base)).unwrap();
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
    catalog: &ResolvedCteCatalog,
) -> Vec<QueryRequirement> {
    let mut source = String::new();
    let encoded = ::serde_json::to_string(catalog).unwrap();
    let query = catalog.base_query().encode();
    for (index, datasource) in datasources.iter().enumerate() {
        ::std::write!(
            source,
            r#"
fn query_{index}() {{
    const __SQL_FUN_CODEGEN_VERSION: &str = {version:?};
    const __SQL_FUN_SOURCE_FILE: &str = "src/queries.rs";
    const __SQL_FUN_SOURCE_LINE: u32 = 42;
    const __SQL_FUN_DATASOURCE: &str = {datasource:?};
    const __SQL_FUN_QUERY: &str = {query:?};
    const __SQL_FUN_USING_CTE: &str = {encoded:?};
}}
"#,
            version = env!("CARGO_PKG_VERSION")
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
    peer_with_gate(fail_method, None).await
}

/// Hold the second RPC until the first envelope has reached its consumer.
pub(super) async fn gated_second_failure(
    release: ::tokio::sync::oneshot::Receiver<()>,
) -> (TcpJsonRpcClientAsync, ::tokio::task::JoinHandle<Vec<Value>>) {
    peer_with_gate(Some("second_analysis"), Some(release)).await
}

async fn peer_with_gate(
    fail_method: Option<&'static str>,
    mut release: Option<::tokio::sync::oneshot::Receiver<()>>,
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
                json!({"jsonrpc":"2.0", "id":request["id"], "error":{"code":-32602,"message":"injected failure"}})
            } else {
                let result = match method {
                    "initialize_schema_context" if fail_method == Some("missing_handle") => {
                        json!({"context_handle":null,"schema_dump_hash":"hash","problems":page()})
                    }
                    "initialize_schema_context" => {
                        json!({"context_handle":format!("context-{}",request["params"]["datasource"].as_str().unwrap()),"schema_dump_hash":"hash","problems":page()})
                    }
                    "analyze_query" => json!({"queries":page()}),
                    _ => panic!("unexpected method: {method}"),
                };
                json!({"jsonrpc":"2.0", "id":request["id"], "result":result})
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
    json!({"handle":"empty","total_length":0,"offset":0,"require_release":false,"items":[]})
}

/// Collect channel items on the consumer side while preserving producer failures.
pub(super) async fn collect_submission(
    client: &mut TcpJsonRpcClientAsync,
    args: &SqlFunArgs,
    requirements: Vec<QueryRequirement>,
) -> Result<Vec<super::QueryAnalysis>, ::anyhow::Error> {
    let (sender, mut receiver) = ::tokio::sync::mpsc::channel(1);
    let producer = super::submit(client, args, requirements, sender);
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
