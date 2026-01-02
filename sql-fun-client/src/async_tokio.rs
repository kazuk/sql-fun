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
    ResponseRecieved(JsonRpcResponse),
    Error(Box<TcpJsonRpcClinetAsyncError>),
}

#[derive(Debug, thiserror::Error)]
pub enum TcpJsonRpcClinetAsyncError {
    #[error(transparent)]
    ChannelError(#[from] tokio::sync::mpsc::error::SendError<InflightRequests>),

    #[error(transparent)]
    TxChannelError(#[from] tokio::sync::mpsc::error::SendError<JsonRpcRequest>),

    #[error(transparent)]
    RxOneshot(#[from] tokio::sync::oneshot::error::RecvError),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    JsonRpcError(#[from] JsonRpcError),
}

#[derive(Debug)]
pub struct TcpJsonRpcClinetAsync {
    requests: Sender<InflightRequests>,
    send_requests: Sender<JsonRpcRequest>,
    id: u64,
}

impl TcpJsonRpcClinetAsync {
    /// convert stream to client
    ///
    /// # Errors
    ///
    /// - `std::io::Error`
    ///
    pub fn try_from(stream: TcpStream) -> Result<Self, std::io::Error> {
        let (sender, reciever) = tokio::sync::mpsc::channel(5);
        let (request_sender, tx_request) = tokio::sync::mpsc::channel(5);

        let response_sender = sender.clone();

        tokio::task::spawn(async move {
            // manage TCP stream IO
            Self::stream_worker(stream, tx_request, response_sender).await;
        });

        tokio::task::spawn(async move {
            // manage inflight request responces
            Self::response_worker(reciever).await
        });

        Ok(Self {
            requests: sender,
            send_requests: request_sender,
            id: 0,
        })
    }

    fn get_id(&mut self) -> u64 {
        let result = self.id;
        self.id += 1;
        result
    }

    /// Invoke a method
    ///
    /// # Errors
    ///
    /// - `TcpJsonRpcClinetAsyncError`
    pub async fn invoke_method(
        &mut self,
        method: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value, TcpJsonRpcClinetAsyncError> {
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
            JsonRpcResponseBody::Error(err) => Err(TcpJsonRpcClinetAsyncError::from(err)),
        }
    }

    async fn recieve_one_response(
        response_sender: &Sender<InflightRequests>,
        buff_reader: &mut BufReader<ReadHalf<'_>>,
    ) -> Result<(), TcpJsonRpcClinetAsyncError> {
        let mut buf = String::new();
        buff_reader.read_line(&mut buf).await?;

        let response: JsonRpcResponse = serde_json::from_str(&buf)?;
        response_sender
            .send(InflightRequests::ResponseRecieved(response))
            .await?;
        Ok(())
    }

    async fn send_one_request(
        writer: &mut WriteHalf<'_>,
        request: JsonRpcRequest,
    ) -> Result<(), TcpJsonRpcClinetAsyncError> {
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
                result = Self::recieve_one_response(&response_sender, &mut buff_reader) => {
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
                InflightRequests::ResponseRecieved(json_rpc_response) => {
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
