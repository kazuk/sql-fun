use crate::{Args, serve_connections, test_helpers::heavy};

use std::{pin::pin, time::Duration};

#[tokio::test(start_paused = true)]
async fn ready_accept_takes_precedence_over_expired_idle_deadline() {
    let listener = heavy::idle_listener().await;
    let addr = listener.local_addr().unwrap();
    let args = Args {
        user: "accept-race-test".to_owned(),
        request_length_limit: 4096,
        idle_timeout: 30,
        server_idle_timeout: 1,
        auth_timeout: 5,
        token: "test-token".to_owned(),
    };
    let mut server = pin!(serve_connections(&args, listener));
    assert!(::futures_util::poll!(&mut server).is_pending());

    // Do not poll the server while preparing both readiness conditions.
    let client = ::tokio::net::TcpStream::connect(addr).await.unwrap();
    ::tokio::task::yield_now().await;
    ::tokio::time::advance(Duration::from_secs(1)).await;
    assert!(::futures_util::poll!(&mut server).is_pending());
    // Acceptance must have cancelled the old deadline rather than just yielding.
    assert!(::futures_util::poll!(&mut server).is_pending());
    drop(client);
}
