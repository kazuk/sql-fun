#![allow(dead_code)]

mod buffers;
mod core_wrapping;
mod error;
mod json_rpc;
mod pid_file;

pub use self::{
    buffers::{PipeBuffer, ReadableBuffer},
    json_rpc::JsonRpcError,
};

use self::{
    core_wrapping::{CoreDialect, CorePostgresVersion},
    error::SqlFunServerError,
    json_rpc::{JsonRpcRequest, JsonRpcResponse},
};

use clap::Parser;
use futures_core::future::BoxFuture;
use futures_util::{FutureExt, future::Shared};
use indexmap::IndexMap;
use pid_file::PidFile;
use sql_fun_server_api::{
    AnalyzeQueryArgs, AnalyzeQueryResponse, Collection, DescribeTableArgs, DescribeTableResponse,
    InitializeSchemaArgs, ListObjectArgs, ObjectSummary, ReadCollectionArgs, ReleaseCollectionArgs,
    SchemaContext, SqlFunServerApi, SqlFunServerApiError,
};
use sql_fun_sqlast::{PostgresDialect, SqlFunArgs, sem::BaseContext};
use std::{
    convert::Into,
    hash::RandomState,
    marker::PhantomData,
    net::SocketAddr,
    pin::Pin,
    sync::LazyLock,
    task::{Context, Poll},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::RwLock,
};

#[derive(Clone, Debug)]
pub struct PendingFut<V, E>(Shared<BoxFuture<'static, Result<V, E>>>);

impl<V, E> Future for PendingFut<V, E>
where
    Result<V, E>: Clone, // Shared の要件
{
    type Output = Result<V, E>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx)
    }
}

impl<V, E> PendingFut<V, E>
where
    V: Clone,
    E: Clone,
{
    pub fn new<F>(fut: F) -> Self
    where
        F: Future<Output = Result<V, E>> + Send + 'static,
    {
        Self(fut.boxed().shared())
    }
}

pub struct Loader<K, V, E, F, FutureResult>
where
    K: std::hash::Hash + std::cmp::Eq,
    V: Clone,
    E: Clone,
    F: Fn(K) -> FutureResult,
    FutureResult: Future<Output = Result<V, E>> + Send + 'static,
{
    func: Box<F>,
    _phantom: PhantomData<K>,
}

impl<K, V, E, F, FutureResult> Loader<K, V, E, F, FutureResult>
where
    K: std::hash::Hash + std::cmp::Eq,
    V: Clone,
    E: Clone,
    F: Fn(K) -> FutureResult,
    FutureResult: Future<Output = Result<V, E>> + Send + 'static,
{
    pub fn new(fun: F) -> Self {
        Self {
            func: Box::new(fun),
            #[allow(clippy::default_constructed_unit_structs)]
            _phantom: PhantomData::default(),
        }
    }

    pub fn load(&self, key: K) -> PendingFut<V, E> {
        PendingFut::new((self.func)(key))
    }
}

#[derive(Clone, Debug)]
enum CacheEntry<Value, Error>
where
    Value: Clone + std::fmt::Debug,
    Error: Clone + std::fmt::Debug,
{
    Starting,
    Pending(PendingFut<Value, Error>),
    Occupied(Value, Instant),
    Error(Error, Instant),
}

impl<Value, Error> CacheEntry<Value, Error>
where
    Value: Clone + std::fmt::Debug,
    Error: Clone + std::fmt::Debug,
{
    pub fn pending(fut: &PendingFut<Value, Error>) -> Self {
        Self::Pending(fut.clone())
    }

    pub fn to_result(&self) -> Result<Value, Error> {
        #[allow(clippy::match_wildcard_for_single_variants)]
        match self {
            Self::Occupied(v, _ttl) => Ok(v.clone()),
            Self::Error(e, _ttl) => Err(e.clone()),
            _ => panic!("can not to convert result {self:?}"),
        }
    }

    pub fn update_with_result(&mut self, value: &Result<Value, Error>) {
        match value {
            Ok(v) => *self = Self::Occupied(v.clone(), Instant::now()),
            Err(e) => *self = Self::Error(e.clone(), Instant::now()),
        }
    }

    pub fn update_last_used(&mut self) {
        #[allow(clippy::match_wildcard_for_single_variants)]
        match self {
            Self::Occupied(_, last_used) | Self::Error(_, last_used) => {
                *last_used = Instant::now();
            }
            _ => {}
        }
    }

    pub fn last_used(&self) -> Option<Instant> {
        match self {
            Self::Pending(_) | Self::Starting => None,
            Self::Occupied(_, lu) | Self::Error(_, lu) => Some(*lu),
        }
    }
}

pub struct Cache<Key, Value, Error, LoaderFun, FutureResult, S = RandomState>
where
    Key: std::hash::Hash + std::cmp::Eq + Clone + Copy,
    Value: Clone + std::fmt::Debug,
    Error: Clone + std::fmt::Debug,
    LoaderFun: Fn(Key) -> FutureResult,
    FutureResult: Future<Output = Result<Value, Error>> + Send + 'static,
{
    load_fn: Loader<Key, Value, Error, LoaderFun, FutureResult>,
    entries: RwLock<IndexMap<Key, CacheEntry<Value, Error>, S>>,
    expilation_timeout: Duration,
}

impl<Key, Value, Error, LoaderFun, FutureResult> Cache<Key, Value, Error, LoaderFun, FutureResult>
where
    Key: std::hash::Hash + std::cmp::Eq + Clone + Copy,
    Value: Clone + std::fmt::Debug,
    Error: Clone + std::fmt::Debug,
    LoaderFun: Fn(Key) -> FutureResult,
    FutureResult: Future<Output = Result<Value, Error>> + Send + 'static,
{
    pub fn new(loader: LoaderFun, expilation_timeout: Duration) -> Self {
        Self {
            load_fn: Loader::new(loader),
            entries: RwLock::new(IndexMap::new()),
            expilation_timeout,
        }
    }

    async fn get_entry_or_insert_starting(&self, key: &Key) -> Option<CacheEntry<Value, Error>> {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(key) {
            entry.update_last_used();
            Some(entry.clone())
        } else {
            entries.insert(*key, CacheEntry::Starting);
            None
        }
    }

    async fn get_entry_expect_pending(&self, key: &Key) -> Option<CacheEntry<Value, Error>> {
        let entry = loop {
            let entry = {
                let entries = self.entries.read().await;
                entries.get(key).cloned()
            };
            if matches!(entry, Some(CacheEntry::Starting)) {
                tokio::task::yield_now().await;
                continue;
            }
            break entry;
        };
        if matches!(entry, Some(CacheEntry::Pending(_))) {
            entry
        } else {
            None
        }
    }

    async fn insert_entry(&self, key: &Key, entry: &CacheEntry<Value, Error>) {
        let mut entries = self.entries.write().await;
        entries.insert(*key, entry.clone());
    }

    async fn update_entry_with_value(
        &self,
        key: &Key,
        value: Result<Value, Error>,
    ) -> Result<Value, Error> {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(key) {
            if matches!(entry, CacheEntry::Pending(_)) {
                entry.update_with_result(&value);
            }
            entry.to_result()
        } else {
            value
        }
    }

    /// get cached value
    ///
    /// # Errors
    ///
    /// when loader function returns error
    ///
    /// # Remarks:
    ///
    /// This method is not cancellation-safe. Callers must not cancel/drop the
    /// future before it completes; doing so may leave the cache in a
    /// Starting/Pending state. Panics inside the loader can also leave a Starting
    /// entry behind.
    ///
    pub async fn get(&self, key: Key) -> Result<Value, Error> {
        if let Some(entry) = self.get_entry_or_insert_starting(&key).await {
            match entry {
                CacheEntry::Pending(cell) => {
                    let result = cell.await;
                    self.update_entry_with_value(&key, result).await
                }
                CacheEntry::Occupied(value, _ttl) => Ok(value),
                CacheEntry::Error(e, _ttl) => Err(e),
                CacheEntry::Starting => {
                    if let Some(CacheEntry::Pending(cell)) =
                        self.get_entry_expect_pending(&key).await
                    {
                        let result = cell.await;
                        self.update_entry_with_value(&key, result).await
                    } else {
                        // can not to catch pending state
                        // may be loader is immediately completed and current state is Occupied / Error.
                        //
                        // then recursive call to this function results correct value
                        self.get(key).await
                    }
                }
            }
        } else {
            let pending_fut = self.load_fn.load(key);
            self.insert_entry(&key, &CacheEntry::pending(&pending_fut))
                .await;
            let result = pending_fut.await;
            self.update_entry_with_value(&key, result).await
        }
    }

    pub async fn evict_entries(&self) {
        let mut entries = self.entries.write().await;

        #[allow(clippy::unchecked_time_subtraction)]
        let expire_before = Instant::now() - self.expilation_timeout;

        entries.retain(|_key, entry| match entry.last_used() {
            None => true,
            Some(last_used) => last_used >= expire_before,
        });
    }
}

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

    let listner = TcpListener::bind("127.0.0.1:0").await?;

    let local_addr = listner.local_addr()?;
    let port = local_addr.port();
    let Some(_pid_file) = PidFile::try_register_primary_server(&args, port)? else {
        return Ok(());
    };

    loop {
        eprintln!("Server started at {local_addr}");

        let (accepted_socker, remote_addr) = listner.accept().await?;

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

    #[expect(clippy::unused_async)]
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
        tracing::info!("{addr}: recieved {line}");
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
