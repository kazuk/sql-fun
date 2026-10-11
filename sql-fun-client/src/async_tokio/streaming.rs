mod coordination;

use self::coordination::{
    AcceptedRequests, CallbackDeliveries, ConnectionReporter, ConnectionStatus, ConnectionTask,
    IncomingResponses, OutgoingRequests, RequestAdmission, RequestSlot, Submission,
    SubmissionQueue, TransportTasks, WireRequest,
};
use crate::{JsonRpcResponse, TcpJsonRpcClientAsyncError};

use std::{convert::Infallible, future::Future, net::SocketAddr};

use ::tokio::io::BufReader;
use ::tokio::net::TcpStream;

pub(super) struct Streaming {
    submissions: SubmissionQueue,
    admission: RequestAdmission,
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
    fn error(&self) -> TcpJsonRpcClientAsyncError {
        TcpJsonRpcClientAsyncError::connection(&self.status.failure_message())
    }

    pub(super) fn peer_addr(&self) -> std::io::Result<SocketAddr> {
        if self.status.is_open() {
            Ok(self.address)
        } else {
            Err(std::io::Error::other("connection closed"))
        }
    }

    pub(super) fn start(reader: BufReader<TcpStream>) -> std::io::Result<Self> {
        let address = reader.get_ref().peer_addr()?;
        let (submissions, accepted) = SubmissionQueue::channel();
        let admission = RequestAdmission::new();
        let (status, reporter) = ConnectionStatus::channel();
        let worker_admission = admission.clone();
        let worker = ConnectionTask::start(async move {
            let result = ConnectionTransport::start(reader)
                .drain(accepted, &reporter, &worker_admission)
                .await;
            worker_admission.stop_accepting();
            reporter.report_completion(&result);
            result
        });
        Ok(Self {
            submissions,
            admission,
            status,
            worker,
            address,
        })
    }

    pub(super) async fn reserve_request(&self) -> Result<RequestSlot, TcpJsonRpcClientAsyncError> {
        self.admission
            .reserve_request()
            .await
            .map_err(|_| self.error())
    }

    pub(super) async fn submit_request<F, Fut>(
        &self,
        id: u64,
        method: &str,
        params: &::serde_json::Value,
        callback: F,
        slot: RequestSlot,
    ) -> Result<(), TcpJsonRpcClientAsyncError>
    where
        F: FnOnce(JsonRpcResponse) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), Infallible>> + Send + 'static,
    {
        let request = WireRequest::serialize(id, method, params)?;
        let submission = Submission::new(
            request,
            Box::new(move |response| Box::pin(callback(response))),
            slot,
        );
        self.submissions
            .enqueue(submission)
            .await
            .map_err(|_| self.error())
    }

    pub(super) async fn wait_for_failure(&self) -> Result<(), TcpJsonRpcClientAsyncError> {
        Err(TcpJsonRpcClientAsyncError::connection(
            &self.status.wait_for_failure().await,
        ))
    }

    pub(super) async fn finish(mut self) -> Result<(), TcpJsonRpcClientAsyncError> {
        self.submissions.finish_accepting();
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
    ) -> Result<(), String> {
        let mut callbacks = CallbackDeliveries::new();
        let result = loop {
            ::tokio::select! {
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
