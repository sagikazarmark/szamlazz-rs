//! Non-durable execution timers. Durable sleeps remain the SDK's responsibility.

use std::{future::Future, time::Duration};

#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn timeout<T>(
    duration: Duration,
    future: impl Future<Output = T>,
) -> Result<T, ()> {
    tokio::time::timeout(duration, future).await.map_err(|_| ())
}

#[cfg(target_arch = "wasm32")]
pub(super) async fn timeout<T>(
    duration: Duration,
    future: impl Future<Output = T>,
) -> Result<T, ()> {
    use futures_util::future::{Either, select};
    match select(std::pin::pin!(future), std::pin::pin!(sleep(duration))).await {
        Either::Left((value, _)) => Ok(value),
        Either::Right(_) => Err(()),
    }
}

pub(super) async fn sleep(duration: Duration) {
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(duration).await;
    #[cfg(target_arch = "wasm32")]
    send_wrapper::SendWrapper::new(gloo_timers::future::TimeoutFuture::new(
        u32::try_from(duration.as_millis()).unwrap_or(u32::MAX),
    ))
    .await;
}
