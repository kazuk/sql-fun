use std::collections::{HashMap, HashSet};

use ::serde_json::Value;
use ::sql_fun_client::{TcpJsonRpcClientAsync, TcpJsonRpcClientAsyncError};
use ::tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use ::tokio::net::{TcpListener, TcpStream};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Fault {
    #[default]
    None,
    NoProgress,
    Handle,
    Offset,
    Length,
    ReleaseFlag,
    ItemType,
    ReadError,
    ReleaseError,
    Malformed,
    Disconnect,
    TooMany,
    InitialBounds,
    InitialOverflow,
    CleanupError,
    ReleaseType,
    InitialError,
    StallRead,
}
impl Fault {
    pub const fn none() -> Self {
        Self::None
    }
    pub const fn no_progress() -> Self {
        Self::NoProgress
    }
    pub const fn handle() -> Self {
        Self::Handle
    }
    pub const fn offset() -> Self {
        Self::Offset
    }
    pub const fn length() -> Self {
        Self::Length
    }
    pub const fn release_flag() -> Self {
        Self::ReleaseFlag
    }
    pub const fn item_type() -> Self {
        Self::ItemType
    }
    pub const fn read_error() -> Self {
        Self::ReadError
    }
    pub const fn release_error() -> Self {
        Self::ReleaseError
    }
    pub const fn malformed() -> Self {
        Self::Malformed
    }
    pub const fn disconnect() -> Self {
        Self::Disconnect
    }
    pub const fn too_many() -> Self {
        Self::TooMany
    }
    pub const fn initial_bounds() -> Self {
        Self::InitialBounds
    }
    pub const fn initial_overflow() -> Self {
        Self::InitialOverflow
    }
    pub const fn cleanup_error() -> Self {
        Self::CleanupError
    }
    pub const fn release_type() -> Self {
        Self::ReleaseType
    }
    pub const fn initial_error() -> Self {
        Self::InitialError
    }
    pub const fn stall_read() -> Self {
        Self::StallRead
    }
    fn corrupt_page(self, page: &mut Value, offset: usize, length: usize, release: bool) {
        match self {
            Self::NoProgress => page["items"] = ::serde_json::json!([]),
            Self::Handle => page["handle"] = ::serde_json::json!("wrong"),
            Self::Offset => page["offset"] = ::serde_json::json!(offset + 1),
            Self::Length => {
                page["total_length"] = ::serde_json::json!(length + 1);
            }
            Self::ReleaseFlag => {
                page["require_release"] = ::serde_json::json!(!release);
            }
            Self::ItemType => page["items"] = ::serde_json::json!(["wrong type"]),
            Self::TooMany => {
                page["items"] = ::serde_json::json!([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
            }
            _ => {}
        }
    }
    pub fn breaks_connection(self) -> bool {
        self == Self::Malformed || self == Self::Disconnect
    }
}

#[derive(Clone, Copy)]
enum ResponseShape {
    Plain,
    Nested,
    Opaque,
    Analyzed,
}
impl ResponseShape {
    const fn plain() -> Self {
        Self::Plain
    }
    const fn nested() -> Self {
        Self::Nested
    }
    const fn opaque() -> Self {
        Self::Opaque
    }
    const fn analyzed() -> Self {
        Self::Analyzed
    }
    const fn is_nested(self) -> bool {
        matches!(self, Self::Nested)
    }
    const fn is_opaque(self) -> bool {
        matches!(self, Self::Opaque)
    }
    const fn is_analyzed(self) -> bool {
        matches!(self, Self::Analyzed)
    }
}

pub struct Scenario {
    total: usize,
    offset: usize,
    initial: usize,
    release: bool,
    fault: Fault,
    requests: usize,
    siblings: usize,
    shape: ResponseShape,
}
impl Scenario {
    pub const fn single(total: usize, offset: usize, initial: usize, release: bool) -> Self {
        Self {
            total,
            offset,
            initial,
            release,
            fault: Fault::none(),
            requests: 1,
            siblings: 1,
            shape: ResponseShape::plain(),
        }
    }
    pub const fn fault(mut self, fault: Fault) -> Self {
        self.fault = fault;
        self
    }
    pub const fn saturated() -> Self {
        Self {
            requests: 8,
            ..Self::siblings()
        }
    }
    pub const fn siblings() -> Self {
        Self {
            siblings: 3,
            ..Self::single(5, 0, 1, true)
        }
    }
    pub const fn nested() -> Self {
        Self {
            shape: ResponseShape::nested(),
            ..Self::single(2, 0, 1, true)
        }
    }
    pub const fn opaque() -> Self {
        Self {
            shape: ResponseShape::opaque(),
            ..Self::single(5, 0, 1, true)
        }
    }

    pub const fn analyzed() -> Self {
        Self {
            shape: ResponseShape::analyzed(),
            ..Self::single(1, 0, 1, true)
        }
    }

    pub const fn analyzed_pages(total: usize, initial: usize) -> Self {
        Self {
            total,
            initial,
            ..Self::analyzed()
        }
    }

    fn analyzed_response(&self, collections: &mut HashMap<String, Vec<Value>>) -> Value {
        let item = ::serde_json::json!({
            "query_span": {"from":{"offset":0,"line":1,"col":1}, "to":{"offset":1,"line":1,"col":2}},
            "query_kind":"Select", "execute_sql":"SELECT 1", "complete":true
        });
        let mut items = Vec::new();
        for index in 0..self.total {
            let mut item = item.clone();
            for name in ["probrems", "column_access", "parameters", "result_columns"] {
                let handle = format!("{name}-{index}");
                item[name] = page(&handle, &[], 0, 0, true);
                collections.insert(handle, Vec::new());
            }
            items.push(item);
        }
        let response =
            ::serde_json::json!({"queries":page("queries", &items, 0, self.initial, true)});
        collections.insert("queries".to_owned(), items);
        response
    }

    fn continuation(&self, request: &Value, collections: &HashMap<String, Vec<Value>>) -> Value {
        let handle = request["params"]["collection_handle"].as_str().unwrap();
        let items = &collections[handle];
        let offset = usize::try_from(request["params"]["start_offset"].as_u64().unwrap()).unwrap();
        let count = usize::try_from(request["params"]["count"].as_u64().unwrap()).unwrap();
        let count = count.min(2).min(items.len() - offset);
        let mut page = page(handle, items, offset, count, self.release);
        self.fault
            .corrupt_page(&mut page, offset, items.len(), self.release);
        page
    }

    fn respond(&self, number: usize, collections: &mut HashMap<String, Vec<Value>>) -> Value {
        if self.shape.is_analyzed() {
            return self.analyzed_response(collections);
        }
        if self.shape.is_opaque() {
            return ::serde_json::json!({"handle":"opaque","items":[],"total_length":100,"offset":0,"require_release":true});
        }
        let mut siblings = Vec::new();
        for sibling in 0..self.siblings {
            let handle = format!("c-{number}-{sibling}");
            let items = if self.shape.is_nested() {
                let mut children = Vec::new();
                for child in 0..2 {
                    let child_handle = format!("{handle}-{child}");
                    let values: Vec<_> = (0..3).map(|i| ::serde_json::json!(i)).collect();
                    children.push(page(&child_handle, &values, 0, 1, true));
                    collections.insert(child_handle, values);
                }
                children
            } else {
                (0..self.total).map(|i| ::serde_json::json!(i)).collect()
            };
            siblings.push(page(
                &handle,
                &items,
                self.offset,
                self.initial,
                self.release,
            ));
            collections.insert(handle, items);
        }
        if self.siblings == 1 {
            let mut value = siblings.pop().unwrap();
            if self.fault == Fault::initial_bounds() {
                value["offset"] = ::serde_json::json!(self.total);
            }
            if self.fault == Fault::initial_overflow() {
                value["offset"] = ::serde_json::json!(usize::MAX);
            }
            value
        } else {
            ::serde_json::json!(siblings)
        }
    }
}
fn page(handle: &str, items: &[Value], offset: usize, count: usize, release: bool) -> Value {
    ::serde_json::json!({"handle":handle,"offset":offset,"total_length":items.len(),"require_release":release,"items": &items[offset..offset+count]})
}

#[derive(Default)]
pub struct RequestLog(Vec<Value>);
impl RequestLog {
    pub fn reads(&self) -> usize {
        self.0
            .iter()
            .filter(|r| r["method"] == "read_collection")
            .count()
    }
    pub fn releases(&self) -> usize {
        self.0
            .iter()
            .filter(|r| r["method"] == "release_collection")
            .count()
    }
    pub fn assert_unique_ids(&self) {
        let ids: HashSet<_> = self.0.iter().map(|r| r["id"].as_u64().unwrap()).collect();
        assert_eq!(ids.len(), self.0.len());
    }
    pub fn assert_initial_range_not_refetched(&self, offset: usize, initial: usize) {
        for request in &self.0 {
            if request["method"] == "read_collection" {
                let start =
                    usize::try_from(request["params"]["start_offset"].as_u64().unwrap()).unwrap();
                assert!(start < offset || start >= offset + initial);
            }
        }
    }
}

pub struct Peer {
    client: TcpJsonRpcClientAsync,
    task: ::tokio::task::JoinHandle<RequestLog>,
    read_started: ::tokio::sync::oneshot::Receiver<()>,
}
impl Peer {
    pub async fn start(scenario: Scenario) -> ::testresult::TestResult<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let (read_sent, read_started) = ::tokio::sync::oneshot::channel();
        let task = ::tokio::spawn(async move {
            let mut read_sent = Some(read_sent);
            let (stream, _) = listener.accept().await.unwrap();
            let mut connection = BufReader::new(stream);
            let mut log = RequestLog::default();
            let mut originals = Vec::new();
            let mut collections = HashMap::new();
            let mut released = HashSet::new();
            loop {
                let mut frame = String::new();
                if connection.read_line(&mut frame).await.unwrap() == 0 {
                    break;
                }
                let request: Value = ::serde_json::from_str(&frame).unwrap();
                log.0.push(request.clone());
                let result = match request["method"].as_str().unwrap() {
                    "probe" => {
                        originals.push(request.clone());
                        // Barrier: all eight normal slots must be occupied before any response.
                        if originals.len() < scenario.requests {
                            continue;
                        }
                        for (number, original) in originals.drain(..).enumerate().rev() {
                            if scenario.fault == Fault::initial_error() {
                                send_error(&mut connection, &original).await;
                                continue;
                            }
                            let value = scenario.respond(number, &mut collections);
                            send(
                                &mut connection,
                                &::serde_json::json!({"jsonrpc":"2.0","id":original["id"],"result":value}),
                            )
                            .await;
                        }
                        continue;
                    }
                    "echo" => request["params"].clone(),
                    "read_collection" => {
                        if let Some(sent) = read_sent.take() {
                            let _ = sent.send(());
                        }
                        if scenario.fault == Fault::stall_read() {
                            continue;
                        }
                        if scenario.fault == Fault::disconnect() {
                            break;
                        }
                        if scenario.fault == Fault::malformed() {
                            connection.get_mut().write_all(b"not JSON\n").await.unwrap();
                            continue;
                        }
                        if scenario.fault == Fault::read_error()
                            || scenario.fault == Fault::cleanup_error()
                        {
                            send_error(&mut connection, &request).await;
                            continue;
                        }
                        scenario.continuation(&request, &collections)
                    }
                    "release_collection" => {
                        let handle = request["params"]["collection_handle"].as_str().unwrap();
                        assert!(released.insert(handle.to_owned()), "double release");
                        if scenario.fault == Fault::release_error()
                            || scenario.fault == Fault::cleanup_error()
                        {
                            send_error(&mut connection, &request).await;
                            continue;
                        }
                        if scenario.fault == Fault::release_type() {
                            ::serde_json::json!("invalid release result")
                        } else {
                            Value::Null
                        }
                    }
                    _ => panic!("unexpected request"),
                };
                send(
                    &mut connection,
                    &::serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":result}),
                )
                .await;
            }
            log
        });
        Ok(Self {
            client: TcpJsonRpcClientAsync::try_from(TcpStream::connect(address).await?)?,
            task,
            read_started,
        })
    }
    pub async fn wait_for_read(&mut self) {
        (&mut self.read_started).await.unwrap();
    }
    pub async fn drop_connection(self) -> RequestLog {
        drop(self.client);
        self.task.await.unwrap()
    }
    pub const fn client(&mut self) -> &mut TcpJsonRpcClientAsync {
        &mut self.client
    }
    pub async fn finish(self) -> ::testresult::TestResult<RequestLog> {
        self.client.finish().await?;
        Ok(self.task.await?)
    }
    pub async fn finish_allow_failure(
        self,
    ) -> (Result<(), TcpJsonRpcClientAsyncError>, RequestLog) {
        let finish = self.client.finish().await;
        (finish, self.task.await.unwrap())
    }
}
async fn send(connection: &mut BufReader<TcpStream>, response: &Value) {
    let mut frame = ::serde_json::to_vec(response).unwrap();
    frame.push(b'\n');
    let _ = connection.get_mut().write_all(&frame).await;
}
async fn send_error(connection: &mut BufReader<TcpStream>, request: &Value) {
    send(connection, &::serde_json::json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32000,"message":"scripted failure"}})).await;
}
