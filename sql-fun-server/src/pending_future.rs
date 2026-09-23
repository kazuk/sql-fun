use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures_core::future::BoxFuture;
use futures_util::{FutureExt, future::Shared};

/// pending future
#[derive(Clone, Debug)]
pub struct PendingFut<V, E>(Shared<BoxFuture<'static, Result<V, E>>>);

impl<V, E> Future for PendingFut<V, E>
where
    Result<V, E>: Clone, // Shared の要件
{
    type Output = Result<V, E>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx)
    }
}

impl<V, E> PendingFut<V, E>
where
    V: Clone,
    E: Clone,
{
    /// create instance
    pub fn new<F>(fut: F) -> Self
    where
        F: Future<Output = Result<V, E>> + Send + 'static,
    {
        Self(fut.boxed().shared())
    }
}
