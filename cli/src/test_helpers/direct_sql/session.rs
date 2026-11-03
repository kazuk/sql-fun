use crate::DiagnosticRenderer;
use ::serde_json::Value;
use ::sql_fun_client::{TcpJsonRpcClientAsync, payloads::SchemaContext};
use ::tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};

pub(crate) struct SessionInputs {
    file: ::tempfile::NamedTempFile,
}
impl SessionInputs {
    pub(crate) fn new(queries: &[&str]) -> ::testresult::TestResult<Self> {
        let file = ::tempfile::NamedTempFile::with_suffix(".json")?;
        std::fs::write(
            file.path(),
            ::serde_json::to_vec(
                &::serde_json::json!({"queries":queries, "expected":vec![::serde_json::json!([]); queries.len()]}),
            )?,
        )?;
        Ok(Self { file })
    }
    pub(crate) fn expectations(&self) -> String {
        format!("{}#$.expected", self.file.path().display())
    }
    pub(crate) fn selection(&self) -> String {
        format!("{}#$.queries", self.file.path().display())
    }
}

fn page(items: &[Value]) -> Value {
    ::serde_json::json!({"handle":"test","total_length":items.len(),"offset":0,"require_release":false,"items":items})
}

pub(crate) fn context() -> SchemaContext {
    ::serde_json::from_value(::serde_json::json!({"context_handle":"test-context","schema_dump_hash":null,"problems":page(&[])})).unwrap()
}

/// A small protocol peer injects deterministic RPC/transport failures without a service.
pub(crate) async fn peer() -> (TcpJsonRpcClientAsync, ::tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = ::tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut stream = BufReader::new(stream);
        loop {
            let mut line = String::new();
            if stream.read_line(&mut line).await.unwrap() == 0 {
                break;
            }
            let request: Value = ::serde_json::from_str(&line).unwrap();
            let query = request["params"]["query"]["Text"].as_str().unwrap();
            if query == "transport_error" {
                break;
            }
            let response = if query == "api_error" {
                ::serde_json::json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32603,"message":"injected internal failure"}})
            } else {
                let item = ::serde_json::json!({"query_span":null,"query_kind":null,"execute_sql":null,"complete":true,"probrems":page(&[]),"parameters":page(&[]),"result_columns":page(&[]),"column_access":page(&[])});
                ::serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":{"queries":page(&[item])}})
            };
            stream
                .get_mut()
                .write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
        }
    });
    (
        TcpJsonRpcClientAsync::try_from(TcpStream::connect(address).await.unwrap()).unwrap(),
        task,
    )
}

pub(crate) fn renderer() -> ::testresult::TestResult<DiagnosticRenderer> {
    let args = ::sql_fun_core::SqlFunArgs::builder(&std::collections::HashMap::new())?
        .term_color(::sql_fun_core::TerminalColor::Never)
        .build()?;
    Ok(DiagnosticRenderer::from_args(&args)?)
}
