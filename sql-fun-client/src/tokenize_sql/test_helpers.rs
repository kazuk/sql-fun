use ::serde_json::Value;
use ::tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use ::tokio::net::{TcpListener, TcpStream};

/// A bounded wire peer for token-specific continuation and release failures.
/// Uses the real client worker without starting a service or loading a catalog.
pub(super) struct TokenPeer {
    client: crate::TcpJsonRpcClientAsync,
    task: ::tokio::task::JoinHandle<usize>,
}
impl TokenPeer {
    pub(super) async fn start(continuation: Value, release_ok: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = ::tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut reader = BufReader::new(stream);
            let mut releases = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).await.unwrap() == 0 {
                    break;
                }
                let request: Value = ::serde_json::from_str(&line).unwrap();
                let mut response = ::serde_json::json!({"jsonrpc":"2.0","id":request["id"]});
                match request["method"].as_str().unwrap() {
                    "tokenize_sql" => {
                        assert_eq!(
                            request["params"],
                            ::serde_json::json!({"source":"SELECT 名", "dialect":"postgres"})
                        );
                        response["result"] = ::serde_json::json!({
                            "handle":"tokens", "total_length":3, "offset":0, "require_release":true,
                            "items":[{"kind":"keyword", "start":0, "end":6}]
                        });
                    }
                    "read_collection" => {
                        assert_eq!(request["params"]["collection_handle"], "tokens");
                        assert_eq!(request["params"]["start_offset"], 1);
                        response["result"] = continuation.clone();
                    }
                    "release_collection" => {
                        assert_eq!(request["params"]["collection_handle"], "tokens");
                        releases += 1;
                        if release_ok {
                            response["result"] = Value::Null;
                        } else {
                            response["error"] =
                                ::serde_json::json!({"code":0,"message":"release rejected"});
                        }
                    }
                    _ => panic!("unexpected method"),
                }
                let mut bytes = ::serde_json::to_vec(&response).unwrap();
                bytes.push(b'\n');
                reader.get_mut().write_all(&bytes).await.unwrap();
            }
            releases
        });
        let client =
            crate::TcpJsonRpcClientAsync::try_from(TcpStream::connect(address).await.unwrap())
                .unwrap();
        Self { client, task }
    }

    pub(super) fn client(&mut self) -> &mut crate::TcpJsonRpcClientAsync {
        &mut self.client
    }

    pub(super) async fn finish(self) -> usize {
        self.client.finish().await.unwrap();
        self.task.await.unwrap()
    }
}

pub(super) fn continuation(items: &Value) -> Value {
    ::serde_json::json!({"handle":"tokens", "total_length":3, "offset":1, "require_release":true,"items":items})
}
