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
