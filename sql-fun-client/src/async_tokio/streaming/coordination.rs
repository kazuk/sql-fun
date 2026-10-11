use crate::async_tokio::read_response;
use crate::{JsonRpcRequest, JsonRpcResponse};

use std::{collections::HashMap, convert::Infallible, future::Future, pin::Pin, sync::Arc};

use ::tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

const OUTSTANDING_LIMIT: usize = 32;

type CallbackFuture = Pin<Box<dyn Future<Output = Result<(), Infallible>> + Send>>;
pub(super) type Callback = Box<dyn FnOnce(JsonRpcResponse) -> CallbackFuture + Send>;

/// The request owns this admission slot through callback task collection.
pub(super) struct RequestSlot {
    _permit: ::tokio::sync::OwnedSemaphorePermit,
}

impl From<::tokio::sync::OwnedSemaphorePermit> for RequestSlot {
    fn from(permit: ::tokio::sync::OwnedSemaphorePermit) -> Self {
        Self { _permit: permit }
    }
}

#[derive(Clone)]
pub(super) struct RequestAdmission(Arc<::tokio::sync::Semaphore>);

impl RequestAdmission {
    pub(super) fn new() -> Self {
        Self(Arc::new(::tokio::sync::Semaphore::new(OUTSTANDING_LIMIT)))
    }

    pub(super) async fn reserve_request(&self) -> Result<RequestSlot, String> {
        self.0
            .clone()
            .acquire_owned()
            .await
            .map(RequestSlot::from)
            .map_err(|_| String::from("connection closed"))
    }

    pub(super) fn stop_accepting(&self) {
        self.0.close();
    }
}

pub(super) struct WireRequest {
    id: u64,
    method: String,
    frame: Vec<u8>,
}

impl WireRequest {
    pub(super) fn serialize(
        id: u64,
        method: &str,
        params: &::serde_json::Value,
    ) -> Result<Self, ::serde_json::Error> {
        let request = JsonRpcRequest::build_request(id, method, params);
        let mut frame = ::serde_json::to_vec(&request)?;
        frame.push(b'\n');
        Ok(Self {
            id,
            method: method.to_owned(),
            frame,
        })
    }

    async fn write_on<W: AsyncWrite + Unpin>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(&self.frame).await?;
        ::tracing::info!(
            request_id = self.id,
            method = self.method,
            "request written"
        );
        Ok(())
    }
}

pub(super) struct Submission {
    request: WireRequest,
    delivery: PendingCallback,
}

impl Submission {
    pub(super) fn new(request: WireRequest, callback: Callback, slot: RequestSlot) -> Self {
        Self {
            request,
            delivery: PendingCallback { callback, slot },
        }
    }
}

pub(super) struct SubmissionQueue(Option<::tokio::sync::mpsc::Sender<Submission>>);
pub(super) struct AcceptedRequests(::tokio::sync::mpsc::Receiver<Submission>);

impl SubmissionQueue {
    pub(super) fn channel() -> (Self, AcceptedRequests) {
        let (sender, receiver) = ::tokio::sync::mpsc::channel(OUTSTANDING_LIMIT);
        (Self(Some(sender)), AcceptedRequests(receiver))
    }

    pub(super) async fn enqueue(&self, submission: Submission) -> Result<(), String> {
        let Some(sender) = self.0.as_ref() else {
            return Err(String::from("connection closed"));
        };
        sender
            .send(submission)
            .await
            .map_err(|_| String::from("connection closed"))
    }

    pub(super) fn finish_accepting(&mut self) {
        self.0.take();
    }
}

impl AcceptedRequests {
    pub(super) async fn next_accepted(&mut self) -> Option<Submission> {
        self.0.recv().await
    }
    pub(super) fn has_queued_requests(&self) -> bool {
        !self.0.is_empty()
    }
    pub(super) fn reject_remaining(&mut self) {
        self.0.close();
    }
}

pub(super) struct OutgoingRequests(Option<::tokio::sync::mpsc::UnboundedSender<WireRequest>>);
pub(super) struct RequestWrites(::tokio::sync::mpsc::UnboundedReceiver<WireRequest>);

impl OutgoingRequests {
    pub(super) fn channel() -> (Self, RequestWrites) {
        // Every queued write owns an admission slot in CallbackDeliveries.
        let (sender, receiver) = ::tokio::sync::mpsc::unbounded_channel();
        (Self(Some(sender)), RequestWrites(receiver))
    }

    pub(super) const fn is_accepting_writes(&self) -> bool {
        self.0.is_some()
    }

    pub(super) fn queue_write(&self, request: WireRequest) -> Result<(), String> {
        let Some(sender) = self.0.as_ref() else {
            return Err(String::from("request writer closed"));
        };
        sender
            .send(request)
            .map_err(|_| String::from("request writer closed"))
    }

    pub(super) fn finish_sending(&mut self) {
        self.0.take();
    }
}

impl RequestWrites {
    pub(super) async fn send_on<W: AsyncWrite + Unpin>(
        mut self,
        mut writer: W,
    ) -> Result<(), String> {
        let result: std::io::Result<()> = async {
            while let Some(request) = self.0.recv().await {
                request.write_on(&mut writer).await?;
            }
            writer.flush().await?;
            writer.shutdown().await
        }
        .await;
        result.map_err(|error| error.to_string())
    }
}

struct PendingCallback {
    callback: Callback,
    slot: RequestSlot,
}

impl PendingCallback {
    async fn deliver(self, response: JsonRpcResponse) -> RequestSlot {
        let Ok(()) = (self.callback)(response).await;
        // The task result retains admission until completion is collected.
        self.slot
    }
}

/// Callback failures have no client-visible contract; only completion is tracked.
pub(super) struct CallbackDeliveries {
    pending: HashMap<u64, PendingCallback>,
    running: ::tokio::task::JoinSet<RequestSlot>,
}

impl CallbackDeliveries {
    pub(super) fn new() -> Self {
        Self {
            pending: HashMap::new(),
            running: ::tokio::task::JoinSet::new(),
        }
    }

    pub(super) fn register(&mut self, submission: Submission) -> WireRequest {
        self.pending
            .insert(submission.request.id, submission.delivery);
        submission.request
    }

    fn deliver_response(&mut self, response: JsonRpcResponse) -> Result<(), String> {
        response.trace_received();
        let Some(delivery) = self.pending.remove(&response.id()) else {
            return Err(format!("unexpected response ID {}", response.id()));
        };
        self.running.spawn(delivery.deliver(response));
        Ok(())
    }

    pub(super) fn has_missing_responses(&self) -> bool {
        !self.pending.is_empty()
    }
    pub(super) fn has_running_callbacks(&self) -> bool {
        !self.running.is_empty()
    }
    pub(super) async fn collect_completion(&mut self) {
        let _ = self.running.join_next().await;
    }
    pub(super) async fn finish_deliveries(&mut self) {
        while self.has_running_callbacks() {
            self.collect_completion().await;
        }
    }
}

pub(super) struct ResponseEvent(Result<Option<JsonRpcResponse>, String>);

impl ResponseEvent {
    const fn received(response: Option<JsonRpcResponse>) -> Self {
        Self(Ok(response))
    }
    fn failed(message: &str) -> Self {
        Self(Err(message.to_owned()))
    }

    pub(super) fn dispatch_or_eof(
        self,
        callbacks: &mut CallbackDeliveries,
    ) -> Result<bool, String> {
        let Some(response) = self.0? else {
            return Ok(true);
        };
        callbacks.deliver_response(response)?;
        Ok(false)
    }
}

pub(super) struct IncomingResponses(::tokio::sync::mpsc::Receiver<ResponseEvent>);
pub(super) struct ResponsePublisher(::tokio::sync::mpsc::Sender<ResponseEvent>);

impl IncomingResponses {
    pub(super) fn channel() -> (Self, ResponsePublisher) {
        let (sender, receiver) = ::tokio::sync::mpsc::channel(OUTSTANDING_LIMIT);
        (Self(receiver), ResponsePublisher(sender))
    }

    pub(super) async fn next_response(&mut self) -> ResponseEvent {
        let Some(response) = self.0.recv().await else {
            return ResponseEvent::failed("response reader closed");
        };
        response
    }
}

impl ResponsePublisher {
    pub(super) async fn deliver_from<R: AsyncRead + Unpin>(self, reader: R) -> Result<(), String> {
        let mut reader = BufReader::new(reader);
        loop {
            match read_response(&mut reader, 2 * 1024 * 1024).await {
                Ok(response) => {
                    let eof = response.is_none();
                    if self
                        .0
                        .send(ResponseEvent::received(response))
                        .await
                        .is_err()
                        || eof
                    {
                        break;
                    }
                }
                Err(error) => {
                    let _ = self.0.send(ResponseEvent::failed(&error.to_string())).await;
                    break;
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct ConnectionStatus(::tokio::sync::watch::Receiver<Option<String>>);
pub(super) struct ConnectionReporter(::tokio::sync::watch::Sender<Option<String>>);

impl ConnectionStatus {
    pub(super) fn channel() -> (Self, ConnectionReporter) {
        let (sender, receiver) = ::tokio::sync::watch::channel(None);
        (Self(receiver), ConnectionReporter(sender))
    }

    pub(super) fn is_open(&self) -> bool {
        self.0.borrow().is_none()
    }
    pub(super) fn failure_message(&self) -> String {
        self.0
            .borrow()
            .clone()
            .unwrap_or_else(|| String::from("connection closed"))
    }

    pub(super) async fn wait_for_failure(&self) -> String {
        let mut status = self.clone();
        while status.is_open() {
            if status.0.changed().await.is_err() {
                break;
            }
        }
        status.failure_message()
    }
}

impl ConnectionReporter {
    pub(super) fn report_failure(&self, message: &str) {
        self.0.send_replace(Some(message.to_owned()));
    }
    pub(super) fn report_completion(&self, result: &Result<(), String>) {
        match result {
            Ok(()) => self.report_failure("connection closed"),
            Err(error) => self.report_failure(error),
        }
    }
}

/// I/O task errors belong to the client and must be surfaced to its caller.
pub(super) struct TransportTasks(::tokio::task::JoinSet<Result<(), String>>);

impl TransportTasks {
    pub(super) fn start<W, R>(writer: W, reader: R) -> Self
    where
        W: Future<Output = Result<(), String>> + Send + 'static,
        R: Future<Output = Result<(), String>> + Send + 'static,
    {
        let mut tasks = ::tokio::task::JoinSet::new();
        tasks.spawn(writer);
        tasks.spawn(reader);
        Self(tasks)
    }

    pub(super) fn has_running_io(&self) -> bool {
        !self.0.is_empty()
    }

    pub(super) async fn observe_completion(&mut self) -> Result<(), String> {
        let Some(completed) = self.0.join_next().await else {
            return Ok(());
        };
        completed.map_err(|error| error.to_string())?
    }

    pub(super) async fn finish_io(&mut self, mut result: Result<(), String>) -> Result<(), String> {
        if result.is_err() {
            self.0.abort_all();
        }
        while self.has_running_io() {
            let completed = self.observe_completion().await;
            if result.is_ok() {
                result = completed;
            }
        }
        result
    }
}

/// Own the connection supervisor; cancelling completion or dropping it aborts it.
pub(super) struct ConnectionTask(::tokio::task::JoinHandle<Result<(), String>>);

impl ConnectionTask {
    pub(super) fn start<F: Future<Output = Result<(), String>> + Send + 'static>(work: F) -> Self {
        Self(::tokio::spawn(work))
    }

    pub(super) async fn finish(&mut self) -> Result<(), String> {
        (&mut self.0).await.map_err(|error| error.to_string())?
    }
}

impl Drop for ConnectionTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}
