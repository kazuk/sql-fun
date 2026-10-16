#[cfg(test)]
mod test_helpers;

use std::{path::PathBuf, process::ExitCode, sync::Arc};

use anyhow::Context;
use serde_json::Value;
use sql_fun_client::{
    ConnectionOptions, JsonRpcResponse, TcpJsonRpcClientAsync,
    payloads::{
        AnalyzeQueryArgs, DescribeTableArgs, EchoArgs, InitializeSchemaArgs, ListObjectArgs,
        ReadCollectionArgs, ReleaseCollectionArgs,
    },
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    sync::{Mutex, watch},
};

/// Arguments for the authenticated JSON Lines API bridge.
#[derive(Debug, Clone, clap::Args)]
pub struct ApiArgs {
    /// Server user namespace (required through this option or USER).
    #[arg(short, long, env = "USER", required = true)]
    user: String,
    /// Launch-only shell command, run in the caller's working directory.
    #[arg(long, env = "SQL_FUN_SERVER_COMMAND")]
    server_command: Option<String>,
}

impl ApiArgs {
    pub(crate) async fn execute(&self) -> Result<ExitCode, ::anyhow::Error> {
        // The application owns logging; the shared server/client do not configure it.
        let _ = ::tracing_subscriber::fmt()
            .with_env_filter(::tracing_subscriber::EnvFilter::from_default_env())
            .with_writer(std::io::stderr)
            .try_init();
        let executable = resolve_executable(std::env::var_os("SQL_FUN_SERVER"))?;
        let options = ConnectionOptions::builder()
            .server_executable(executable)
            .user(&self.user)
            .maybe_server_command(self.server_command.clone())
            .build();
        let client = ::sql_fun_client::connect(&options)
            .await
            .context("api: cannot connect to sql-fun-server; check USER, SQL_FUN_SERVER and the launch command")?;
        bridge(
            client,
            BufReader::new(::tokio::io::stdin()),
            ::tokio::io::stdout(),
        )
        .await?;
        Ok(ExitCode::SUCCESS)
    }
}

fn resolve_executable(explicit: Option<std::ffi::OsString>) -> Result<PathBuf, ::anyhow::Error> {
    if let Some(path) = explicit {
        anyhow::ensure!(
            !path.is_empty(),
            "SQL_FUN_SERVER must name an executable path"
        );
        // An explicit reference may be created by the launch command. Do not
        // canonicalize it or require an existing file before that command runs.
        return std::path::absolute(PathBuf::from(path))
            .context("cannot resolve SQL_FUN_SERVER to an absolute path");
    }
    ::which::which("sql-fun-server")
        .context("sql-fun-server is not on PATH; install it or set SQL_FUN_SERVER to its executable path")
        .and_then(|path| std::path::absolute(path).context("cannot resolve sql-fun-server path"))
}

#[cfg(test)]
mod test_executable {
    use super::resolve_executable;

    #[test]
    fn relative_reference_can_precede_cargo_build() {
        let path = resolve_executable(Some("not-built-yet/sql-fun-server".into())).unwrap();
        assert!(path.is_absolute());
        assert!(path.ends_with("not-built-yet/sql-fun-server"));
    }

    #[test]
    fn empty_explicit_reference_is_not_a_path_fallback() {
        assert!(
            resolve_executable(Some("".into()))
                .unwrap_err()
                .to_string()
                .contains("SQL_FUN_SERVER")
        );
    }

    #[test]
    fn development_exports_keep_reference_separate_from_launch() {
        let envrc = include_str!("../../.envrc");
        assert!(
            envrc.contains(r#"export SQL_FUN_SERVER="${GIT_ROOT}/target/debug/sql-fun-server""#)
        );
        assert!(envrc.contains("export SQL_FUN_SERVER_COMMAND='cargo run -p sql-fun-server'"));
    }
}

struct ApiInput {
    method: String,
    params: Value,
}

impl ApiInput {
    fn parse(line: &str) -> Result<Self, ::anyhow::Error> {
        let value: Value = ::serde_json::from_str(line).context("invalid JSON")?;
        let object = value.as_object().context("request must be a JSON object")?;
        anyhow::ensure!(
            !object.contains_key("id"),
            "id must be omitted; the client assigns request IDs"
        );
        if let Some(version) = object.get("jsonrpc") {
            anyhow::ensure!(
                version.as_str() == Some("2.0"),
                "jsonrpc must be the string \"2.0\" when supplied"
            );
        }
        let method = object.get("method").context("method must be a string")?;
        let method = method.as_str().context("method must be a string")?;
        let params = object.get("params").context("params is required")?;
        Self::validate_params(method, params)
            .with_context(|| format!("invalid params for {method}"))?;
        Ok(Self {
            method: method.to_owned(),
            params: params.clone(),
        })
    }

    fn validate_params(method: &str, params: &Value) -> Result<(), ::anyhow::Error> {
        fn typed<T: ::serde::de::DeserializeOwned>(params: &Value) -> Result<(), ::anyhow::Error> {
            ::serde_json::from_value::<T>(params.clone())?;
            Ok(())
        }
        match method {
            "echo" => typed::<EchoArgs>(params),
            "initialize_schema_context" => typed::<InitializeSchemaArgs>(params),
            "read_collection" => typed::<ReadCollectionArgs>(params),
            "release_collection" => typed::<ReleaseCollectionArgs>(params),
            "analyze_query" => typed::<AnalyzeQueryArgs>(params),
            "list_object" => typed::<ListObjectArgs>(params),
            "describe_table" => typed::<DescribeTableArgs>(params),
            _ => anyhow::bail!("unknown API method {method}"),
        }
    }
}

#[cfg(test)]
mod test_input {
    use super::ApiInput;

    #[rstest::rstest]
    #[case::omitted(r#"{"method":"echo","params":{}}"#, true)]
    #[case::valid_version(r#"{"jsonrpc":"2.0","method":"echo","params":[]}"#, true)]
    #[case::old_version(r#"{"jsonrpc":"1.0","method":"echo","params":{}}"#, false)]
    #[case::null_version(r#"{"jsonrpc":null,"method":"echo","params":{}}"#, false)]
    #[case::numeric_version(r#"{"jsonrpc":2.0,"method":"echo","params":{}}"#, false)]
    #[case::object_version(r#"{"jsonrpc":{},"method":"echo","params":{}}"#, false)]
    #[case::null_id(r#"{"id":null,"method":"echo","params":{}}"#, false)]
    #[case::string_id(r#"{"id":"x","method":"echo","params":{}}"#, false)]
    #[case::missing_method(r#"{"params":{}}"#, false)]
    #[case::numeric_method(r#"{"method":1,"params":{}}"#, false)]
    #[case::missing_params(r#"{"method":"echo"}"#, false)]
    #[case::null_params(r#"{"method":"echo","params":null}"#, false)]
    #[case::scalar_params(r#"{"method":"echo","params":42}"#, false)]
    #[case::unknown(r#"{"method":"unknown","params":{}}"#, false)]
    #[case::authentication_is_not_api(r#"{"method":"initialize","params":{}}"#, false)]
    #[case::non_object("[]", false)]
    #[case::malformed("{", false)]
    fn validates_envelope(#[case] line: &str, #[case] valid: bool) {
        assert_eq!(ApiInput::parse(line).is_ok(), valid);
    }

    #[rstest::rstest]
    #[case::echo("echo", r#"[{"nested":[null,1,true]}]"#)]
    #[case::initialize("initialize_schema_context", r#"{"datasource":"d","schema_dump":"schema.sql","expected_hash":"0000000000000000000000000000000000000000000000000000000000000000","environments":{}}"#)]
    #[case::read(
        "read_collection",
        r#"{"collection_handle":"c","start_offset":0,"count":null}"#
    )]
    #[case::release("release_collection", r#"{"collection_handle":"c"}"#)]
    #[case::analyze(
        "analyze_query",
        r#"{"context_handle":"c","query":{"Text":"select 1"}}"#
    )]
    #[case::list("list_object", r#"{"predicate":{"Kind":"All"},"extra_fields":[]}"#)]
    #[case::describe("describe_table", r#"{"schema_name":"public","local_name":"t"}"#)]
    #[case::positional("describe_table", r#"["public","t"]"#)]
    fn uses_public_argument_types(#[case] method: &str, #[case] params: &str) {
        let line = format!(r#"{{"method":"{method}","params":{params}}}"#);
        let request = ApiInput::parse(&line).unwrap();
        assert_eq!(
            request.params,
            ::serde_json::from_str::<::serde_json::Value>(params).unwrap()
        );
    }

    #[rstest::rstest]
    fn rejects_wrong_typed_params(
        #[values(
            "initialize_schema_context",
            "read_collection",
            "release_collection",
            "analyze_query",
            "list_object",
            "describe_table"
        )]
        method: &str,
    ) {
        assert!(ApiInput::parse(&format!(r#"{{"method":"{method}","params":{{}}}}"#)).is_err());
    }
}

struct ResponseOutput<W> {
    writer: Mutex<W>,
    failure: watch::Sender<Option<String>>,
}

impl<W: AsyncWrite + Unpin> ResponseOutput<W> {
    fn new(writer: W) -> Self {
        Self {
            writer: Mutex::new(writer),
            failure: watch::channel(None).0,
        }
    }

    async fn deliver(&self, response: JsonRpcResponse) -> Result<(), std::convert::Infallible> {
        let mut writer = self.writer.lock().await;
        if self.failure.borrow().is_some() {
            return Ok(());
        }
        let result = async {
            let mut line = ::serde_json::to_vec(&response)?;
            line.push(b'\n');
            writer.write_all(&line).await?;
            writer.flush().await?;
            Ok::<(), ::anyhow::Error>(())
        }
        .await;
        if let Err(error) = result {
            self.failure
                .send_replace(Some(format!("api stdout: {error}")));
        }
        Ok(())
    }

    async fn flush(&self) {
        let mut writer = self.writer.lock().await;
        if self.failure.borrow().is_none()
            && let Err(error) = writer.flush().await
        {
            self.failure
                .send_replace(Some(format!("api stdout: {error}")));
        }
    }
}

#[cfg(test)]
mod test_output {
    use super::{ResponseOutput, test_helpers};

    #[rstest::rstest]
    #[tokio::test]
    async fn records_failure_without_failing_callback(#[values(false, true)] flush_only: bool) {
        let output = ResponseOutput::new(test_helpers::FailingWriter::new(flush_only));
        let response = ::serde_json::from_str(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#).unwrap();
        assert!(output.deliver(response).await.is_ok());
        let failure = output.failure.borrow().clone().unwrap();
        assert!(failure.contains(if flush_only {
            "flush failure"
        } else {
            "write failure"
        }));
        let response = ::serde_json::from_str(r#"{"jsonrpc":"2.0","id":2,"result":{}}"#).unwrap();
        assert!(output.deliver(response).await.is_ok());
        output.flush().await;
        assert_eq!(output.failure.borrow().as_ref().unwrap(), &failure);
    }
}

async fn output_failed(failure: &mut watch::Receiver<Option<String>>) {
    let _ = failure.wait_for(Option::is_some).await;
}

async fn bridge<R, W>(
    mut client: TcpJsonRpcClientAsync,
    mut input: R,
    writer: W,
) -> Result<(), ::anyhow::Error>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let output = Arc::new(ResponseOutput::new(writer));
    let mut output_failure = output.failure.subscribe();
    let mut errors = Vec::new();
    let mut line_number = 0;
    let mut line = String::new();
    loop {
        line.clear();
        let read = tokio::select! {
            result = input.read_line(&mut line) => result,
            result = client.wait_for_failure() => {
                errors.push(format!("api transport: {}", result.unwrap_err()));
                break;
            }
            () = output_failed(&mut output_failure) => break,
        };
        match read {
            Ok(0) => break,
            Ok(_) => line_number += 1,
            Err(error) => {
                errors.push(format!("api input line {}: {error}", line_number + 1));
                break;
            }
        }
        let request = match ApiInput::parse(&line) {
            Ok(request) => request,
            Err(error) => {
                errors.push(format!("api input line {line_number}: {error:#}"));
                break;
            }
        };
        let callback_output = Arc::clone(&output);
        let submission = client.invoke_method(
            &request.method,
            &request.params,
            move |response| async move { callback_output.deliver(response).await },
        );
        tokio::select! {
            result = submission => {
                if let Err(error) = result {
                    errors.push(format!("api transport: {error}"));
                    break;
                }
            }
            () = output_failed(&mut output_failure) => break,
        }
    }
    // Input errors are EOF for submission, not an early return: accepted work
    // must still reach callbacks. Output failures are owned by this CLI state.
    if let Err(error) = client.finish().await {
        errors.push(format!("api drain: {error}"));
    }
    output.flush().await;
    if let Some(error) = output.failure.borrow().as_ref() {
        errors.push(error.clone());
    }
    anyhow::ensure!(errors.is_empty(), "{}", errors.join("\n"));
    Ok(())
}

#[cfg(test)]
mod test_bridge {
    use super::{bridge, test_helpers};
    use ::tokio::io::{AsyncReadExt, BufReader};

    #[tokio::test]
    async fn slow_output_bounds_submission_and_finish_waits_for_callbacks() {
        let (client, mut received) = test_helpers::eager_peer().await;
        let (writer, mut reader) = ::tokio::io::duplex(1);
        // More than the client's 8 unfinished user-request slots, with no output
        // reader until capacity saturation has been observed on the wire.
        let input = test_helpers::echo_lines(40);
        let completion = ::tokio::spawn(async move {
            bridge(client, BufReader::new(input.as_bytes()), writer).await
        });
        for expected in 1..=8 {
            assert_eq!(
                ::tokio::time::timeout(std::time::Duration::from_secs(5), received.recv())
                    .await
                    .unwrap(),
                Some(expected)
            );
        }
        assert!(
            ::tokio::time::timeout(std::time::Duration::from_millis(50), received.recv())
                .await
                .is_err()
        );
        assert!(!completion.is_finished());
        let mut output = Vec::new();
        ::tokio::time::timeout(
            std::time::Duration::from_secs(5),
            reader.read_to_end(&mut output),
        )
        .await
        .unwrap()
        .unwrap();
        completion.await.unwrap().unwrap();
        assert_eq!(String::from_utf8(output).unwrap().lines().count(), 40);
    }

    #[tokio::test]
    async fn drain_keeps_ids_with_reversed_responses_and_waits_for_output() {
        let client = test_helpers::reversing_peer().await;
        let input = b"{\"method\":\"echo\",\"params\":[0]}\n{\"method\":\"echo\",\"params\":[1]}\n";
        let (writer, mut reader) = ::tokio::io::duplex(1);
        let completion = ::tokio::spawn(bridge(client, BufReader::new(&input[..]), writer));
        let mut output = Vec::new();
        ::tokio::time::timeout(
            std::time::Duration::from_secs(5),
            reader.read_to_end(&mut output),
        )
        .await
        .unwrap()
        .unwrap();
        completion.await.unwrap().unwrap();
        let lines: Vec<::serde_json::Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| ::serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        for response in lines {
            assert_eq!(response["id"], response["result"][0]);
        }
    }
}
