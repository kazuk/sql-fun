#![allow(dead_code)]

mod buffers;
mod cache;
mod core_wrapping;
mod error;
mod json_rpc;
mod pending_future;
mod pid_file;

pub use self::{
    buffers::{PipeBuffer, ReadableBuffer},
    cache::{Cache, Loader},
    json_rpc::JsonRpcError,
    pending_future::PendingFut,
};

use self::{
    core_wrapping::{CoreDialect, CorePostgresVersion},
    error::SqlFunServerError,
    json_rpc::{JsonRpcRequest, JsonRpcResponse},
};

use clap::Parser;
use pid_file::PidFile;
use sql_fun_server_api::{
    AnalyzeQueryArgs, AnalyzeQueryResponse, Collection, DescribeTableArgs, DescribeTableResponse,
    InitializeSchemaArgs, ListObjectArgs, ObjectSummary, ReadCollectionArgs, ReleaseCollectionArgs,
    SchemaContext, SqlFunServerApi, SqlFunServerApiError,
};
use sql_fun_sqlast::{PostgresDialect, SqlFunArgs, sem::BaseContext};
use std::{convert::Into, net::SocketAddr, sync::LazyLock};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

#[derive(clap::Parser, Clone)]
pub struct Args {
    #[arg(short, long, env = "USER")]
    user: String,
}

/// Run the server event loop until interrupted.
///
/// # Errors
///
/// Returns [`ServerError`] when binding or accepting sockets fails.
pub async fn service() -> Result<(), ServerError> {
    let args = Args::parse();

    let listener = TcpListener::bind("127.0.0.1:0").await?;

    let local_addr = listener.local_addr()?;
    let port = local_addr.port();
    let Some(_pid_file) = PidFile::try_register_primary_server(&args, port)? else {
        return Ok(());
    };

    loop {
        eprintln!("Server started at {local_addr}");

        let (accepted_socker, remote_addr) = listener.accept().await?;

        tokio::task::spawn(async move { serve_client(accepted_socker, remote_addr).await });
    }
}

pub struct Api {}

static API_IMPL: LazyLock<Api> = LazyLock::new(|| Api {});

impl Api {
    #[must_use]
    pub fn api_impl() -> &'static Self {
        &API_IMPL
    }

    async fn initialize_schema_context_impl(
        &self,
        args: InitializeSchemaArgs,
    ) -> Result<SchemaContext, SqlFunServerError> {
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

        let metadata = context_args.load_metadata()?;
        let version = context_args.postgres_version(&metadata)?;
        let mut builtin_schema = version.definition_base_path(context_args.sql_fun_home()?);
        builtin_schema.push("schema.sql");

        let base_context = BaseContext::new(context_args.clone())?;
        PostgresDialect::parse_schema_file(&builtin_schema, base_context)?;

        todo!();
    }
}

#[async_trait::async_trait]
impl sql_fun_server_api::SqlFunServerApi for Api {
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

async fn serve_client(stream: TcpStream, addr: SocketAddr) -> Result<(), ServerError> {
    let (read_half, mut write_half) = stream.into_split();
    let mut buf_read = tokio::io::BufReader::new(read_half);

    loop {
        let mut line = String::new();
        buf_read.read_line(&mut line).await?;
        tracing::info!("{addr}: received {line}");
        let request_message: JsonRpcRequest = serde_json::from_str(&line)?;
        let response = process_request(&request_message).await;
        let response = match response {
            Ok(result) => JsonRpcResponse::success(&request_message, result),
            Err(error) => JsonRpcResponse::error(&request_message, &error),
        };
        let mut response_json = serde_json::to_string(&response)?;
        response_json.push('\n');

        tracing::info!("{addr}: sending {response_json}");
        write_half.write_all(response_json.as_bytes()).await?;
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ServerError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Serde(#[from] serde_json::Error),

    #[error("logic bug {0}")]
    Bug(String),
}
