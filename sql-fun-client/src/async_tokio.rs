use std::{collections::HashMap, convert::Infallible};

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{
        TcpStream,
        tcp::{ReadHalf, WriteHalf},
    },
    sync::{
        mpsc::{Receiver, Sender},
        oneshot,
    },
};

use crate::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, JsonRpcResponseBody};

pub enum InflightRequests {
    New(u64, tokio::sync::oneshot::Sender<JsonRpcResponse>),
    ResponseReceived(JsonRpcResponse),
    Error(Box<TcpJsonRpcClientAsyncError>),
}

/// JSON-RPC Client errors
#[derive(Debug, thiserror::Error)]
pub enum TcpJsonRpcClientAsyncError {
    /// channel sending error
    #[error(transparent)]
    ChannelError(#[from] tokio::sync::mpsc::error::SendError<InflightRequests>),

    /// tx channel sending error
    #[error(transparent)]
    TxChannelError(#[from] tokio::sync::mpsc::error::SendError<JsonRpcRequest>),

    /// rx error
    #[error(transparent)]
    RxOneshot(#[from] tokio::sync::oneshot::error::RecvError),

    /// json serialize error
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// IO error
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// JSON-RPC error
    #[error(transparent)]
    JsonRpcError(#[from] JsonRpcError),
}

/// JSON-RPC Client
#[derive(Debug)]
pub struct TcpJsonRpcClientAsync {
    requests: Sender<InflightRequests>,
    send_requests: Sender<JsonRpcRequest>,
    id: u64,
}

impl TcpJsonRpcClientAsync {
    /// convert stream to client
    ///
    /// # Errors
    ///
    /// - `std::io::Error`
    ///
    pub fn try_from(stream: TcpStream) -> Result<Self, std::io::Error> {
        let (sender, receiver) = tokio::sync::mpsc::channel(5);
        let (request_sender, tx_request) = tokio::sync::mpsc::channel(5);

        let response_sender = sender.clone();

        tokio::task::spawn(async move {
            // manage TCP stream IO
            Self::stream_worker(stream, tx_request, response_sender).await;
        });

        tokio::task::spawn(async move {
            // manage inflight request responses
            Self::response_worker(receiver).await
        });

        Ok(Self {
            requests: sender,
            send_requests: request_sender,
            id: 0,
        })
    }

    const fn get_id(&mut self) -> u64 {
        let result = self.id;
        self.id += 1;
        result
    }

    /// Invoke a method
    ///
    /// # Errors
    ///
    /// - `TcpJsonRpcClientAsyncError`
    pub async fn invoke_method(
        &mut self,
        method: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value, TcpJsonRpcClientAsyncError> {
        let (sender, receiver) = oneshot::channel();

        let id = self.get_id();
        // send to response worker
        self.requests
            .send(InflightRequests::New(id, sender))
            .await?;
        let request = JsonRpcRequest::build_request(id, String::from(method), params);
        // send to stream worker
        self.send_requests.send(request).await?;
        let response = receiver.await?;

        match response.body {
            JsonRpcResponseBody::Response(value) => Ok(value),
            JsonRpcResponseBody::Error(err) => Err(TcpJsonRpcClientAsyncError::from(err)),
        }
    }

    async fn receive_one_response(
        response_sender: &Sender<InflightRequests>,
        buff_reader: &mut BufReader<ReadHalf<'_>>,
    ) -> Result<(), TcpJsonRpcClientAsyncError> {
        let mut buf = String::new();
        buff_reader.read_line(&mut buf).await?;

        let response: JsonRpcResponse = serde_json::from_str(&buf)?;
        response_sender
            .send(InflightRequests::ResponseReceived(response))
            .await?;
        Ok(())
    }

    async fn send_one_request(
        writer: &mut WriteHalf<'_>,
        request: JsonRpcRequest,
    ) -> Result<(), TcpJsonRpcClientAsyncError> {
        let mut json = serde_json::to_string(&request)?;
        json.push('\n');
        writer.write_all(json.as_bytes()).await?;
        Ok(())
    }

    async fn stream_worker(
        mut stream: TcpStream,
        mut tx_request: Receiver<JsonRpcRequest>,
        response_sender: Sender<InflightRequests>,
    ) {
        let (read_half, mut write_half) = stream.split();
        let mut buff_reader = BufReader::new(read_half);

        while !response_sender.is_closed() {
            tokio::select! {
                biased;
                Some(request) = tx_request.recv() => {
                    if let Err(e) = Self::send_one_request(&mut write_half, request).await {
                        let _ = response_sender
                            .send(InflightRequests::Error(Box::new(e)))
                            .await;
                    }
                },
                result = Self::receive_one_response(&response_sender, &mut buff_reader) => {
                    if let Err(e) = result {
                        let _ = response_sender
                            .send(InflightRequests::Error(Box::new(e)))
                            .await;
                    }
                },
            }
        }
    }

    async fn response_worker(mut receiver: Receiver<InflightRequests>) -> Result<(), Infallible> {
        let mut pending = HashMap::new();

        while let Some(event) = receiver.recv().await {
            match event {
                InflightRequests::New(id, receiver) => {
                    pending.insert(id, receiver);
                }
                InflightRequests::ResponseReceived(json_rpc_response) => {
                    let id = json_rpc_response.id;

                    let Some(pending_receiver) = pending.remove(&id) else {
                        continue;
                    };
                    let _ = pending_receiver.send(json_rpc_response);
                }
                // stream errors, exit & close this stream
                InflightRequests::Error(_error) => {
                    receiver.close();
                    return Ok(());
                }
            }
        }
        Ok(())
    }
}
