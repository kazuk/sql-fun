use ::tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use ::tokio::net::{TcpListener, TcpStream};

/// A narrow transport peer reverses replies after EOF; actual lifecycle tests
/// use authenticated real-server fixtures in the CLI integration target.
pub async fn reversing_peer() -> ::sql_fun_client::TcpJsonRpcClientAsync {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    ::tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut stream = BufReader::new(stream);
        let mut requests = Vec::new();
        loop {
            let mut line = String::new();
            if stream.read_line(&mut line).await.unwrap() == 0 {
                break;
            }
            requests.push(::serde_json::from_str::<::serde_json::Value>(&line).unwrap());
        }
        for request in requests.into_iter().rev() {
            let mut response = ::serde_json::to_vec(&::serde_json::json!({"jsonrpc":"2.0", "id":request["id"], "result":request["params"]})).unwrap();
            response.push(b'\n');
            stream.get_mut().write_all(&response).await.unwrap();
        }
    });
    ::sql_fun_client::TcpJsonRpcClientAsync::try_from(TcpStream::connect(address).await.unwrap())
        .unwrap()
}

/// Inject a permanent output failure at writing or at flushing.
pub struct FailingWriter {
    flush_only: bool,
}

impl FailingWriter {
    pub fn new(flush_only: bool) -> Self {
        Self { flush_only }
    }
}

impl ::tokio::io::AsyncWrite for FailingWriter {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        if self.flush_only {
            std::task::Poll::Ready(Ok(buf.len()))
        } else {
            std::task::Poll::Ready(Err(std::io::Error::other("injected write failure")))
        }
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Err(std::io::Error::other("injected flush failure")))
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

/// Replies immediately and reports wire arrivals, allowing a bounded-output test
/// to distinguish accepted work from requests still waiting for client capacity.
pub async fn eager_peer() -> (
    ::sql_fun_client::TcpJsonRpcClientAsync,
    ::tokio::sync::mpsc::Receiver<usize>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (count, receiver) = ::tokio::sync::mpsc::channel(64);
    ::tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut stream = BufReader::new(stream);
        let mut total = 0;
        loop {
            let mut line = String::new();
            if stream.read_line(&mut line).await.unwrap() == 0 {
                break;
            }
            let request: ::serde_json::Value = ::serde_json::from_str(&line).unwrap();
            total += 1;
            count.send(total).await.unwrap();
            let mut response = ::serde_json::to_vec(&::serde_json::json!({"jsonrpc":"2.0", "id":request["id"], "result":request["params"]})).unwrap();
            response.push(b'\n');
            stream.get_mut().write_all(&response).await.unwrap();
        }
    });
    (
        ::sql_fun_client::TcpJsonRpcClientAsync::try_from(
            TcpStream::connect(address).await.unwrap(),
        )
        .unwrap(),
        receiver,
    )
}

pub fn echo_lines(count: usize) -> String {
    use std::fmt::Write;
    let mut input = String::new();
    for n in 0..count {
        writeln!(input, "{{\"method\":\"echo\",\"params\":[{n}]}}").unwrap();
    }
    input
}
