//! `sql-fun` schema caching server

#![allow(dead_code)]

mod buffers;
mod cache;
mod core_wrapping;
mod cte_realization;
mod error;
mod file_hash_cache;
mod json_rpc;
mod panic_policy;
mod pending_future;
mod pid_file;
mod schema_cache;
mod session_wrapper;

#[cfg(test)]
mod test_helpers;

use crate::buffers::JsonLinesStreamReader;

pub use self::{
    buffers::{PipeBuffer, ReadableBuffer},
    cache::Cache,
    json_rpc::JsonRpcError,
    panic_policy::install_server_panic_hook,
    pending_future::PendingFut,
    schema_cache::{
        CachedSchemaAnalysis, InitializedSchema, LayerDiagnostics, SchemaCacheService,
        SchemaContextKey, SchemaLoadError, SchemaSelection,
    },
};

use self::{
    core_wrapping::{CoreDialect, CorePostgresVersion},
    error::SqlFunServerError,
    json_rpc::{JsonRpcRequest, JsonRpcResponse},
};

use ::sql_fun_sqlast::{
    PostgresDialect, SqlFunArgs,
    sem::{BaseContext, FrozenBaseContext},
};
use clap::Parser;
use pid_file::PidFile;
use serde_json::json;
use sql_fun_server_api::{
    AnalyzeQueryArgs, AnalyzeQueryResponse, Collection, DescribeTableArgs, DescribeTableResponse,
    InitializeSchemaArgs, ListObjectArgs, ObjectSummary, ReadCollectionArgs, ReleaseCollectionArgs,
    SchemaContext, SqlFunServerApi, SqlFunServerApiError,
};
use std::{convert::Into, net::SocketAddr, sync::LazyLock, time::Duration};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
};

/// service for user
#[derive(clap::Parser, Clone)]
pub struct Args {
    #[arg(short, long, env = "USER")]
    user: String,

    #[arg(short, long, env = "REQUEST_LENGTH_LIMIT", default_value_t=2*1024*1024)]
    request_length_limit: usize,

    #[arg(short, long, env = "IDLE_TIMEOUT", default_value_t = 30)]
    idle_timeout: u64,

    #[arg(short, long, env = "AUTH_TIMEOUT", default_value_t = 5)]
    auth_timeout: u64,

    #[arg(skip)]
    token: String,
}

/// Run the server event loop until interrupted.
///
/// # Errors
///
/// Returns [`ServerError`] when binding or accepting sockets fails.
pub async fn service() -> Result<(), ServerError> {
    let mut args = Args::parse();

    let listener = TcpListener::bind("127.0.0.1:0").await?;

    let local_addr = listener.local_addr()?;
    let port = local_addr.port();
    let Some(pid_file) = PidFile::try_register_primary_server(&args, port)? else {
        return Ok(());
    };
    args.token = pid_file.token().to_owned();

    loop {
        eprintln!("Server started at {local_addr}");

        let (accepted_socker, remote_addr) = listener.accept().await?;
        let args = args.clone();

        tokio::task::spawn(async move { serve_client(&args, accepted_socker, remote_addr).await });
    }
}

/// API implementation
pub struct Api {
    schema_cache: std::sync::OnceLock<SchemaCacheService>,
}

static API_IMPL: LazyLock<Api> = LazyLock::new(|| Api {
    schema_cache: std::sync::OnceLock::new(),
});

impl Api {
    /// get static instance
    #[must_use]
    pub fn api_impl() -> &'static Self {
        &API_IMPL
    }

    /// Obtain the server-lifetime cache for an explicitly selected installation.
    ///
    /// Session infrastructure retains results from this service independently.
    /// The first explicit installation fixes the service scope until restart.
    ///
    /// # Panics
    /// Panics if the service registration invariant is violated.
    ///
    /// # Errors
    /// Rejects unavailable or conflicting builtin installations.
    pub fn schema_cache_service(
        &self,
        home: &std::path::Path,
    ) -> Result<&SchemaCacheService, SchemaLoadError> {
        let canonical = home
            .canonicalize()
            .map_err(|error| SchemaLoadError::source(&error))?;
        if self.schema_cache.get().is_none() {
            let service = SchemaCacheService::new(&canonical)?;
            let _ = self.schema_cache.set(service);
        }
        let service = self.schema_cache.get().expect("registered service");
        service.require_installation(&canonical)?;
        Ok(service)
    }

    async fn initialize_schema_context_impl(
        &self,
        args: InitializeSchemaArgs,
    ) -> Result<SchemaContext, SqlFunServerError> {
        if args.datasource().is_empty() {
            return Err(SqlFunServerError::invalid_datasource(args.datasource()));
        }
        let mut builder = SqlFunArgs::builder(args.environments())?;
        let dialect = CoreDialect::from(args.engine_dialect());
        let engine_version = CorePostgresVersion::try_from(args.engine_version())?;
        let extensions = args.extensions();
        let search_path = args.search_path();
        let schema_dump = args.schema_dump();
        let extensions: Vec<_> = extensions
            .iter()
            .map(|e| format!("{}@{}", e.extension_name(), e.extension_version()))
            .collect();
        let extensions = extensions.join(",");

        let context_args = builder
            .sql_dialect(dialect)
            .postgres_version(engine_version)
            .postgres_search_path(search_path.clone())
            .postgres_extensions(extensions)?
            .schema_file(schema_dump)
            .build()?;

        let metadata_collection = context_args.load_metadata()?;
        let metadata = metadata_collection
            .get(args.datasource())
            .ok_or_else(|| SqlFunServerError::invalid_datasource(args.datasource()))?;
        let version = context_args.postgres_version(metadata)?;
        let mut builtin_schema = version.definition_base_path(context_args.sql_fun_home()?);
        builtin_schema.push("schema.sql");

        let base_context = BaseContext::new(&context_args, metadata)?;
        PostgresDialect::parse_schema_file(
            &builtin_schema,
            &FrozenBaseContext::from(base_context),
            &mut None,
        )?;

        todo!();
    }
}

#[async_trait::async_trait]
impl sql_fun_server_api::SqlFunServerApi for Api {
    async fn echo(
        &self,
        args: ::sql_fun_server_api::EchoArgs,
    ) -> Result<serde_json::Value, SqlFunServerApiError> {
        Ok(args.into_value())
    }

    /// Initialize schema-aware query analysis context
    async fn initialize_schema_context(
        &self,
        args: InitializeSchemaArgs,
    ) -> Result<SchemaContext, SqlFunServerApiError> {
        self.initialize_schema_context_impl(args)
            .await
            .map_err(Into::into)
    }

    /// Read a collection
    ///
    /// When server returns multiple items, server keeps scrollable collection,
    /// [`crate::SqlFunServerApi::read_collection`] reads items in collection.
    ///
    /// # Return value
    ///
    /// returns [`Collection<T>`], T depends collection item type
    ///
    async fn read_collection(
        &self,
        _args: ReadCollectionArgs,
    ) -> Result<serde_json::Value, SqlFunServerApiError> {
        todo!()
    }

    /// Release a collection
    ///
    /// When server returns collection, server keeps scrollable collection,
    /// [`SqlFunServerApi::release_collection`] release a server side collection.
    ///
    async fn release_collection(
        &self,
        _args: ReleaseCollectionArgs,
    ) -> Result<(), SqlFunServerApiError> {
        todo!()
    }

    /// analyze query
    async fn analyze_query(
        &self,
        _args: AnalyzeQueryArgs,
    ) -> Result<AnalyzeQueryResponse, SqlFunServerApiError> {
        todo!()
    }

    /// list schema objects
    async fn list_objects(
        &self,
        _args: ListObjectArgs,
    ) -> Result<Collection<ObjectSummary>, SqlFunServerApiError> {
        todo!()
    }

    /// describe a table
    async fn describe_table(
        &self,
        _args: DescribeTableArgs,
    ) -> Result<DescribeTableResponse, SqlFunServerApiError> {
        todo!()
    }
}

/// process a request
///
/// # Errors
///
/// [`JsonRpcError`]
pub async fn process_request(
    request_message: &JsonRpcRequest,
) -> Result<serde_json::Value, JsonRpcError> {
    let method = request_message.method();

    match method {
        "echo" => {
            let params = request_message
                .params()
                .ok_or_else(|| JsonRpcError::parameter_required(method))?;
            let args = serde_json::from_value::<::sql_fun_server_api::EchoArgs>(params.clone())?;
            Ok(Api::api_impl().echo(args).await?)
        }
        "initialize_schema_context" => {
            let Some(params) = request_message.params() else {
                Err(JsonRpcError::parameter_required(method))?
            };
            let initialize_arg: InitializeSchemaArgs = serde_json::from_value(params.clone())?;
            let result = Api::api_impl()
                .initialize_schema_context(initialize_arg)
                .await?;
            let result_value = serde_json::to_value(result)?;
            Ok(result_value)
        }

        _ => Err(JsonRpcError::unknown_method(method)),
    }
}

#[cfg(test)]
mod test_echo_dispatch {
    use crate::{json_rpc::JsonRpcRequest, process_request};

    #[rstest::rstest]
    #[case::object(r#"{"nested":[null,true,{"n":42}]}"#)]
    #[case::array(r#"[null,{"nested":[1,2]},false]"#)]
    #[case::empty_object("{}")]
    #[case::empty_array("[]")]
    #[tokio::test]
    async fn echoes_without_schema_setup(#[case] params: &str) {
        let request: JsonRpcRequest = ::serde_json::from_str(&format!(
            r#"{{"jsonrpc":"2.0","id":7,"method":"echo","params":{params}}}"#
        ))
        .unwrap();
        assert_eq!(
            process_request(&request)
                .await
                .unwrap_or_else(|_| panic!("valid echo parameters rejected")),
            ::serde_json::from_str::<::serde_json::Value>(params).unwrap()
        );
    }

    #[rstest::rstest]
    #[case::omitted("")]
    #[case::null(",\"params\":null")]
    #[case::scalar(",\"params\":42")]
    #[tokio::test]
    async fn rejects_missing_or_scalar_params(#[case] params_field: &str) {
        let request: JsonRpcRequest = ::serde_json::from_str(&format!(
            r#"{{"jsonrpc":"2.0","id":7,"method":"echo"{params_field}}}"#
        ))
        .unwrap();
        assert!(process_request(&request).await.is_err());
    }
}

async fn serve_client(args: &Args, stream: TcpStream, addr: SocketAddr) -> Result<(), ServerError> {
    let (read_half, mut write_half) = stream.into_split();

    let mut request_reader = JsonLinesStreamReader::new_async(
        read_half,
        8,
        8192,
        b'\n',
        1024,
        Duration::from_secs(args.idle_timeout),
    )
    .await?;
    let auth_timeout = Duration::from_secs(args.auth_timeout);
    let request = tokio::time::timeout(auth_timeout, request_reader.read_request()).await;
    let Ok(request) = request else {
        tracing::warn!("authentication failed: first request is not initialize : source = {addr}");
        return Err(ServerError::authentication_timeout());
    };
    let Ok(Some(request)) = request else {
        tracing::warn!("authentication failed: request not acceptable");
        return Err(ServerError::authentication_failed_generic_err());
    };

    let request_message: JsonRpcRequest = serde_json::from_value(request)?;
    if request_message.method() != "initialize" {
        tracing::warn!("authentication failed: first request is not initialize : source = {addr}");
        return ServerError::raise_authentication_failed_non_initialize();
    }
    if let Some(params) = request_message.params()
        && let Some(token_node) = params.get("sql_fun_token")
        && let Some(token) = token_node.as_str()
    {
        if args.token != token {
            tracing::warn!("authentication failed: invalid token : source = {addr} ");
            return ServerError::raise_authentication_failed_invalid_token();
        }
    } else {
        tracing::warn!("authentication failed: invalid token : source = {addr} ");
        return ServerError::raise_authentication_failed_invalid_token();
    }

    let response = JsonRpcResponse::success(&request_message, json!("OK"));
    let mut response_json = serde_json::to_string(&response)?;
    response_json.push('\n');
    write_half.write_all(response_json.as_bytes()).await?;

    let wrapper = self::session_wrapper::SessionWrapper::new();
    wrapper
        .session_handler(request_reader, write_half, addr, args.request_length_limit)
        .await
}

#[cfg(test)]
#[path = "serve_client_tests.rs"]
mod serve_client_tests;

/// server error
#[derive(thiserror::Error, Debug)]
pub enum ServerError {
    /// IO
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Serde
    #[error(transparent)]
    Serde(#[from] serde_json::Error),

    /// unexpected BUG
    #[error("logic bug {0}")]
    Bug(String),

    /// reader error
    #[error(transparent)]
    Reader(#[from] crate::buffers::ReaderError),

    /// authentication failed
    #[error("Authentication Failed: {0}")]
    AuthenticationFailed(AuthenticationFailed),
}

/// reason for authentication failed
#[derive(Debug)]
pub enum AuthenticationFailed {
    /// timeout
    Timeout,
    /// protocol failed
    ProtocolFailed,
    /// invalid token
    InvalidToken,
    /// request receive error
    RequestReceive,
}

impl std::fmt::Display for AuthenticationFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout => f.write_str("timeout"),
            Self::ProtocolFailed => f.write_str("protocol failed"),
            Self::InvalidToken => f.write_str("invalid token"),
            Self::RequestReceive => f.write_str("server can not receive request (IO or JSON)"),
        }
    }
}

impl ServerError {
    const fn authentication_timeout() -> Self {
        Self::AuthenticationFailed(AuthenticationFailed::Timeout)
    }

    const fn authentication_failed_generic_err() -> Self {
        Self::AuthenticationFailed(AuthenticationFailed::RequestReceive)
    }

    const fn raise_authentication_failed_non_initialize() -> Result<(), Self> {
        Err(Self::AuthenticationFailed(
            AuthenticationFailed::ProtocolFailed,
        ))
    }

    const fn raise_authentication_failed_invalid_token() -> Result<(), Self> {
        Err(Self::AuthenticationFailed(
            AuthenticationFailed::InvalidToken,
        ))
    }
}
