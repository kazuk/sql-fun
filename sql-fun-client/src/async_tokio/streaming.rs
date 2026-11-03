mod coordination;

use crate::TypedResponseError;

use self::coordination::{
    AcceptedRequests, CallbackDeliveries, ConnectionReporter, ConnectionStatus, ConnectionTask,
    IncomingResponses, OutgoingRequests, RequestAdmission, Submission, SubmissionQueue,
    TransportTasks, WireRequest,
};
use crate::{JsonRpcResponse, TcpJsonRpcClientAsyncError};

use std::{
    convert::Infallible,
    future::Future,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

use ::tokio::{io::BufReader, net::TcpStream};

pub(super) struct Streaming {
    sender: RequestSender,
    finish: Option<::tokio::sync::oneshot::Sender<()>>,
    status: ConnectionStatus,
    worker: ConnectionTask,
    address: SocketAddr,
}

impl std::fmt::Debug for Streaming {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Streaming")
            .field("address", &self.address)
            .finish_non_exhaustive()
    }
}

impl Streaming {
    pub(super) fn peer_addr(&self) -> std::io::Result<SocketAddr> {
        if self.status.is_open() {
            Ok(self.address)
        } else {
            Err(std::io::Error::other("connection closed"))
        }
    }

    pub(super) fn start(reader: BufReader<TcpStream>, first_id: u64) -> std::io::Result<Self> {
        let address = reader.get_ref().peer_addr()?;
        let (submissions, accepted) = SubmissionQueue::channel();
        let admission = RequestAdmission::new(8);
        let collection_admission = RequestAdmission::new(24);
        let (finish, finishing) = ::tokio::sync::oneshot::channel();
        let (status, reporter) = ConnectionStatus::channel();
        let worker_admission = admission.clone();
        let worker_collections = collection_admission.clone();
        let typed = TypedWork::default();
        let worker_typed = typed.clone();
        let sender = RequestSender {
            typed,
            submissions,
            admission,
            collection_admission,
            status: status.clone(),
            ids: Arc::new(AtomicU64::new(first_id)),
        };
        let worker = ConnectionTask::start(async move {
            let result = ConnectionTransport::start(reader)
                .drain(
                    accepted,
                    &reporter,
                    &worker_admission,
                    &worker_collections,
                    finishing,
                    worker_typed,
                )
                .await;
            worker_admission.stop_accepting();
            worker_collections.stop_accepting();
            reporter.report_completion(&result);
            result
        });
        Ok(Self {
            sender,
            finish: Some(finish),
            status,
            worker,
            address,
        })
    }

    pub(super) fn sender(&self) -> RequestSender {
        self.sender.clone()
    }

    pub(super) async fn submit_request<F, Fut>(
        &self,
        method: &str,
        params: &::serde_json::Value,
        callback: F,
    ) -> Result<(), TcpJsonRpcClientAsyncError>
    where
        F: FnOnce(JsonRpcResponse) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), Infallible>> + Send + 'static,
    {
        self.sender
            .submit_user(
                method,
                params,
                Box::new(move |response| {
                    Box::pin(async move {
                        if let Ok(response) = response {
                            callback(response).await
                        } else {
                            Ok(())
                        }
                    })
                }),
            )
            .await
    }

    pub(super) async fn wait_for_failure(&self) -> Result<(), TcpJsonRpcClientAsyncError> {
        Err(TcpJsonRpcClientAsyncError::connection(
            &self.status.wait_for_failure().await,
        ))
    }

    pub(super) async fn finish(mut self) -> Result<(), TcpJsonRpcClientAsyncError> {
        self.sender.admission.stop_accepting();
        if let Some(finish) = self.finish.take() {
            let _ = finish.send(());
        }
        self.worker
            .finish()
            .await
            .map_err(|error| TcpJsonRpcClientAsyncError::connection(&error))
    }
}

/// Coordinate accepted requests, response delivery, and graceful I/O completion.
struct ConnectionTransport {
    requests: OutgoingRequests,
    responses: IncomingResponses,
    tasks: TransportTasks,
}

impl ConnectionTransport {
    fn start(reader: BufReader<TcpStream>) -> Self {
        let (read, write) = ::tokio::io::split(reader);
        let (requests, writes) = OutgoingRequests::channel();
        let (responses, events) = IncomingResponses::channel();
        let tasks = TransportTasks::start(writes.send_on(write), events.deliver_from(read));
        Self {
            requests,
            responses,
            tasks,
        }
    }

    async fn drain(
        mut self,
        mut accepted: AcceptedRequests,
        reporter: &ConnectionReporter,
        admission: &RequestAdmission,
        collections: &RequestAdmission,
        mut finishing: ::tokio::sync::oneshot::Receiver<()>,
        typed: TypedWork,
    ) -> Result<(), String> {
        let mut callbacks = CallbackDeliveries::new();
        let mut finish_requested = false;
        let result = loop {
            if finish_requested
                && !typed.is_active()
                && !accepted.has_queued_requests()
                && self.requests.is_accepting_writes()
            {
                accepted.reject_remaining();
                self.requests.finish_sending();
            }
            ::tokio::select! {
                _ = &mut finishing, if !finish_requested => { finish_requested = true; }
                submission = accepted.next_accepted(), if self.requests.is_accepting_writes() => {
                    if let Some(submission) = submission {
                        let request = callbacks.register(submission);
                        if let Err(error) = self.requests.queue_write(request) { break Err(error); }
                    } else { self.requests.finish_sending(); }
                }
                response = self.responses.next_response() => {
                    match response.dispatch_or_eof(&mut callbacks) {
                        Ok(true) => break self.validate_eof(&callbacks, &accepted),
                        Ok(false) => {},
                        Err(error) => break Err(error),
                    }
                }
                () = callbacks.collect_completion(), if callbacks.has_running_callbacks() => {}
                completed = self.tasks.observe_completion(), if self.tasks.has_running_io() => {
                    if let Err(error) = completed { break Err(error); }
                }
            }
        };
        if let Err(error) = &result {
            reporter.report_failure(error);
            admission.stop_accepting();
            collections.stop_accepting();
            callbacks.fail_pending(&mut accepted, error);
            accepted.reject_remaining();
        }
        let result = self.tasks.finish_io(result).await;
        callbacks.finish_deliveries().await;
        result
    }

    fn validate_eof(
        &self,
        callbacks: &CallbackDeliveries,
        accepted: &AcceptedRequests,
    ) -> Result<(), String> {
        if callbacks.has_missing_responses() || accepted.has_queued_requests() {
            return Err(String::from("EOF with missing responses"));
        }
        if self.requests.is_accepting_writes() {
            return Err(String::from("unexpected connection EOF"));
        }
        Ok(())
    }
}

/// Shared submission protocol for user requests and same-session continuation.
#[derive(Clone)]
pub(crate) struct RequestSender {
    submissions: SubmissionQueue,
    admission: RequestAdmission,
    collection_admission: RequestAdmission,
    status: ConnectionStatus,
    ids: Arc<AtomicU64>,
    typed: TypedWork,
}

impl RequestSender {
    pub(crate) fn begin_typed(&self) -> TypedGuard {
        self.typed.begin()
    }
    pub(crate) async fn submit_user(
        &self,
        method: &str,
        params: &::serde_json::Value,
        callback: coordination::Callback,
    ) -> Result<(), TcpJsonRpcClientAsyncError> {
        self.submit(method, params, &self.admission, callback).await
    }

    async fn submit(
        &self,
        method: &str,
        params: &::serde_json::Value,
        admission: &RequestAdmission,
        callback: coordination::Callback,
    ) -> Result<(), TcpJsonRpcClientAsyncError> {
        let slot = admission
            .reserve_request()
            .await
            .map_err(|message| TcpJsonRpcClientAsyncError::connection(&message))?;
        let id = self.allocate_id()?;
        let request = WireRequest::serialize(id, method, params)?;
        self.submissions
            .enqueue(Submission::new(request, callback, slot))
            .await
            .map_err(|message| TcpJsonRpcClientAsyncError::connection(&message))
    }

    fn allocate_id(&self) -> Result<u64, TcpJsonRpcClientAsyncError> {
        let mut id = self.ids.load(Ordering::Relaxed);
        loop {
            let Some(next) = id.checked_add(1) else {
                return Err(TcpJsonRpcClientAsyncError::connection(
                    "request IDs exhausted",
                ));
            };
            match self
                .ids
                .compare_exchange_weak(id, next, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return Ok(id),
                Err(current) => id = current,
            }
        }
    }

    pub(crate) async fn collection_rpc(
        &self,
        method: &str,
        params: &::serde_json::Value,
    ) -> Result<::serde_json::Value, TypedResponseError> {
        let (sent, received) = ::tokio::sync::oneshot::channel();
        self.submit(
            method,
            params,
            &self.collection_admission,
            Box::new(move |response| {
                Box::pin(async move {
                    let _ = sent.send(response);
                    Ok(())
                })
            }),
        )
        .await
        .map_err(TypedResponseError::from)?;
        let response = received
            .await
            .map_err(|_| TypedResponseError::message("internal response delivery closed"))?
            .map_err(|message| TypedResponseError::message(&message))?;
        response.into_result().map_err(TypedResponseError::from)
    }

    pub(crate) fn is_open(&self) -> bool {
        self.status.is_open()
    }
}

/// Tracks accepted typed work which may still need to submit continuation RPCs.
#[derive(Clone, Default)]
struct TypedWork(Arc<AtomicUsize>);
impl TypedWork {
    fn begin(&self) -> TypedGuard {
        self.0.fetch_add(1, Ordering::Relaxed);
        TypedGuard { work: self.clone() }
    }
    fn is_active(&self) -> bool {
        self.0.load(Ordering::Relaxed) != 0
    }
}
pub(crate) struct TypedGuard {
    work: TypedWork,
}
impl Drop for TypedGuard {
    fn drop(&mut self) {
        self.work.0.fetch_sub(1, Ordering::Relaxed);
    }
}
