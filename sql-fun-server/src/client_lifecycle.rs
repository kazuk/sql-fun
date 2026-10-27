use crate::ServerError;

use std::{future::Future, time::Duration};

use ::tokio::{
    task::{JoinError, JoinSet},
    time::Instant,
};

/// Owns connection handlers until completion, including error returns.
///
/// Only an empty owner can expire. Cancelling the wait to accept another client
/// preserves completion bookkeeping; dropping the owner cancels its handlers.
pub(super) struct ClientLifecycle {
    connections: JoinSet<Result<(), ServerError>>,
    idle_timeout: Option<Duration>,
    idle_since: Instant,
}

impl From<u32> for ClientLifecycle {
    fn from(seconds: u32) -> Self {
        let idle_timeout = if seconds == 0 {
            None
        } else {
            Some(Duration::from_secs(u64::from(seconds)))
        };
        Self {
            connections: JoinSet::new(),
            idle_timeout,
            idle_since: Instant::now(),
        }
    }
}

impl ClientLifecycle {
    pub(super) fn track_connection(
        &mut self,
        connection: impl Future<Output = Result<(), ServerError>> + Send + 'static,
    ) {
        // Acceptance has select priority. Reap here too so a continuous stream
        // of ready accepts cannot retain completed tasks indefinitely.
        while let Some(result) = self.connections.try_join_next() {
            self.connection_ended(result);
        }
        self.connections.spawn(connection);
    }

    /// Wait for a full client-free interval, reaping completed handlers even when
    /// idle shutdown is disabled. Cancellation never restarts an idle interval.
    pub(super) async fn wait_for_idle_shutdown(&mut self) {
        loop {
            if self.connections.is_empty() {
                if let Some(timeout) = self.idle_timeout {
                    // Configuration is bounded to u32 seconds (at most 136 years),
                    // safely within the supported platforms' Instant range.
                    ::tokio::time::sleep_until(self.idle_since + timeout).await;
                    return;
                }
                std::future::pending::<()>().await;
            }

            if let Some(result) = self.connections.join_next().await {
                self.connection_ended(result);
            }
        }
    }

    fn connection_ended(&mut self, result: Result<Result<(), ServerError>, JoinError>) {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => ::tracing::warn!(%error, "client connection ended"),
            Err(error) => {
                // Production's panic hook aborts before this point. Library
                // consumers must not silently lose a panicking handler.
                if error.is_panic() {
                    std::panic::resume_unwind(error.into_panic());
                }
                ::tracing::warn!(%error, "client connection task cancelled");
            }
        }
        if self.connections.is_empty() {
            self.idle_since = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ClientLifecycle;

    use std::{pin::pin, time::Duration};

    use ::tokio::sync::oneshot;

    async fn assert_running(clients: &mut ClientLifecycle) {
        assert!(::futures_util::poll!(pin!(clients.wait_for_idle_shutdown())).is_pending());
    }

    fn connect(clients: &mut ClientLifecycle) -> oneshot::Sender<()> {
        let (disconnect, disconnected) = oneshot::channel();
        clients.track_connection(async move {
            let _ = disconnected.await;
            Ok(())
        });
        disconnect
    }

    #[tokio::test(start_paused = true)]
    async fn startup_deadline_survives_cancelled_waits() {
        let mut clients = ClientLifecycle::from(180);
        assert_running(&mut clients).await;
        ::tokio::time::advance(Duration::from_secs(179)).await;
        assert_running(&mut clients).await;
        ::tokio::time::advance(Duration::from_secs(1)).await;
        clients.wait_for_idle_shutdown().await;
        assert_eq!(
            ::tokio::time::Instant::now() - clients.idle_since,
            Duration::from_secs(180)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn last_disconnect_starts_a_fresh_interval_across_reconnections() {
        let mut clients = ClientLifecycle::from(10);
        for _ in 0..3 {
            ::tokio::time::advance(Duration::from_secs(9)).await;
            let first = connect(&mut clients);
            let second = connect(&mut clients);
            ::tokio::time::advance(Duration::from_secs(20)).await;
            assert_running(&mut clients).await;
            first.send(()).unwrap();
            ::tokio::task::yield_now().await;
            assert_running(&mut clients).await;
            ::tokio::time::advance(Duration::from_secs(20)).await;
            assert_running(&mut clients).await;
            second.send(()).unwrap();
            ::tokio::task::yield_now().await;
            assert_running(&mut clients).await;
            assert!(clients.connections.is_empty());
        }
        ::tokio::time::advance(Duration::from_secs(9)).await;
        assert_running(&mut clients).await;
        ::tokio::time::advance(Duration::from_secs(1)).await;
        clients.wait_for_idle_shutdown().await;
    }

    #[tokio::test(start_paused = true)]
    async fn disabled_shutdown_still_reaps_connections() {
        let mut clients = ClientLifecycle::from(0);
        assert_running(&mut clients).await;
        ::tokio::time::advance(Duration::from_secs(1_000)).await;
        assert_running(&mut clients).await;
        let connection = connect(&mut clients);
        connection.send(()).unwrap();
        ::tokio::task::yield_now().await;
        assert_running(&mut clients).await;
        assert!(clients.connections.is_empty());
        ::tokio::time::advance(Duration::from_secs(1_000)).await;
        assert_running(&mut clients).await;
    }

    #[tokio::test(start_paused = true)]
    async fn continuous_acceptance_does_not_retain_finished_connections() {
        let mut clients = ClientLifecycle::from(0);
        for _ in 0..10 {
            let connection = connect(&mut clients);
            assert_eq!(clients.connections.len(), 1);
            connection.send(()).unwrap();
            ::tokio::task::yield_now().await;
        }
        assert_running(&mut clients).await;
        assert!(clients.connections.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn largest_supported_timeout_can_arm_and_rearm_without_overflow() {
        let mut clients = ClientLifecycle::from(u32::MAX);
        assert_running(&mut clients).await;
        let connection = connect(&mut clients);
        connection.send(()).unwrap();
        ::tokio::task::yield_now().await;
        assert_running(&mut clients).await;
        assert!(clients.connections.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn failed_connection_releases_its_lifecycle_slot() {
        let mut clients = ClientLifecycle::from(1);
        clients.track_connection(async { Err(std::io::Error::other("test disconnect").into()) });
        ::tokio::task::yield_now().await;
        assert_running(&mut clients).await;
        assert!(clients.connections.is_empty());
        ::tokio::time::advance(Duration::from_secs(1)).await;
        clients.wait_for_idle_shutdown().await;
    }
}
