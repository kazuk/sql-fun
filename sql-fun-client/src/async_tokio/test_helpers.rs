use ::tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use ::tokio::net::{TcpListener, TcpStream};

/// A narrow wire fixture for framing/response validation, not lifecycle coverage.
pub async fn response_peer(response: &[u8]) -> std::io::Result<super::TcpJsonRpcClientAsync> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let response = response.to_owned();
    ::tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(stream);
        let mut request = String::new();
        reader.read_line(&mut request).await.unwrap();
        let request: ::serde_json::Value = ::serde_json::from_str(&request).unwrap();
        assert_eq!(request["params"]["sql_fun_token"], "secret");
        reader.get_mut().write_all(&response).await.unwrap();
        // Keep the socket open until the client closes it, allowing cancellation
        // and unterminated-frame checks without replacing the actual server tests.
        let mut remainder = Vec::new();
        let _ = reader.read_to_end(&mut remainder).await;
    });
    super::TcpJsonRpcClientAsync::try_from(TcpStream::connect(address).await?)
}

/// Respond only after write EOF, exercising submission independence and half-close.
pub async fn draining_peer(
    responses: usize,
    reverse: bool,
) -> std::io::Result<super::TcpJsonRpcClientAsync> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    ::tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(stream);
        let mut requests = Vec::new();
        loop {
            let mut frame = String::new();
            if reader.read_line(&mut frame).await.unwrap() == 0 {
                break;
            }
            requests.push(::serde_json::from_str::<::serde_json::Value>(&frame).unwrap());
        }
        if reverse {
            requests.reverse();
        }
        for request in requests.into_iter().take(responses) {
            let response = ::serde_json::json!({"jsonrpc":"2.0", "id":request["id"], "result":request["params"]});
            let mut frame = ::serde_json::to_vec(&response).unwrap();
            frame.push(b'\n');
            reader.get_mut().write_all(&frame).await.unwrap();
        }
    });
    super::TcpJsonRpcClientAsync::try_from(TcpStream::connect(address).await?)
}

/// Capture structured tracing output without changing production logging setup.
#[derive(Clone, Default)]
pub struct LogCapture(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl LogCapture {
    pub fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl std::io::Write for LogCapture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> ::tracing_subscriber::fmt::MakeWriter<'a> for LogCapture {
    type Writer = Self;
    fn make_writer(&'a self) -> Self {
        self.clone()
    }
}

/// Echo each request immediately, independently of other callback completion.
pub async fn eager_peer() -> std::io::Result<super::TcpJsonRpcClientAsync> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    ::tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(stream);
        loop {
            let mut frame = String::new();
            if reader.read_line(&mut frame).await.unwrap() == 0 {
                break;
            }
            let request: ::serde_json::Value = ::serde_json::from_str(&frame).unwrap();
            let response = ::serde_json::json!({"jsonrpc":"2.0", "id":request["id"], "result":request["params"]});
            let mut frame = ::serde_json::to_vec(&response).unwrap();
            frame.push(b'\n');
            reader.get_mut().write_all(&frame).await.unwrap();
        }
    });
    super::TcpJsonRpcClientAsync::try_from(TcpStream::connect(address).await?)
}
