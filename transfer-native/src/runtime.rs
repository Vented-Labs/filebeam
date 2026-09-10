use std::{future::Future, sync::Arc, time::Duration};

use anyhow::{Result, bail};
use filebeam_transfer::{retry_delay_ms, retryable_status};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    time::sleep,
};
use tokio_util::sync::CancellationToken;

/// Aggregate limits, rather than one limit per item, prevent large selections
/// from multiplying memory, disk IO, or open sockets.
#[derive(Clone)]
pub struct Runtime {
    network: Arc<Semaphore>,
    cpu: Arc<Semaphore>,
    filesystem: Arc<Semaphore>,
    cancelled: CancellationToken,
}

impl Runtime {
    pub fn new(
        network: usize,
        cpu: usize,
        filesystem: usize,
        cancelled: CancellationToken,
    ) -> Result<Self> {
        if network == 0 || cpu == 0 || filesystem == 0 {
            bail!("native transfer limits must be positive");
        }
        Ok(Self {
            network: Arc::new(Semaphore::new(network)),
            cpu: Arc::new(Semaphore::new(cpu)),
            filesystem: Arc::new(Semaphore::new(filesystem)),
            cancelled,
        })
    }

    pub fn cancelled(&self) -> &CancellationToken {
        &self.cancelled
    }

    pub async fn network(&self) -> Result<OwnedSemaphorePermit> {
        permit(&self.network, &self.cancelled).await
    }
    pub async fn cpu(&self) -> Result<OwnedSemaphorePermit> {
        permit(&self.cpu, &self.cancelled).await
    }
    pub async fn filesystem(&self) -> Result<OwnedSemaphorePermit> {
        permit(&self.filesystem, &self.cancelled).await
    }
}

async fn permit(
    semaphore: &Arc<Semaphore>,
    cancelled: &CancellationToken,
) -> Result<OwnedSemaphorePermit> {
    tokio::select! {
        _ = cancelled.cancelled() => bail!("transfer cancelled"),
        permit = semaphore.clone().acquire_owned() => permit.map_err(Into::into),
    }
}

/// Classification deliberately distinguishes a transport failure from an HTTP
/// reply. Only idempotent callers should invoke this helper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryClass {
    Network,
    Http {
        status: u16,
        retry_after_ms: Option<u64>,
    },
}

/// Retry one idempotent operation using the shared transfer policy. The active
/// future is raced with cancellation, not merely the backoff sleep.
pub async fn retry<T, E, F, Fut, Classify>(
    cancelled: &CancellationToken,
    attempts: u32,
    staging: bool,
    mut operation: F,
    retryable: Classify,
) -> Result<T, E>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T, E>>,
    Classify: Fn(&E) -> Option<RetryClass>,
    E: From<anyhow::Error>,
{
    assert!(attempts > 0, "attempts must be positive");
    for attempt in 0..attempts {
        if cancelled.is_cancelled() {
            return Err(anyhow::anyhow!("transfer cancelled").into());
        }
        let result = tokio::select! {
            _ = cancelled.cancelled() => return Err(anyhow::anyhow!("transfer cancelled").into()),
            result = operation(attempt) => result,
        };
        match result {
            Ok(value) => return Ok(value),
            Err(error) if attempt + 1 < attempts => {
                let Some(class) = retryable(&error) else {
                    return Err(error);
                };
                let (allowed, retry_after) = match class {
                    RetryClass::Network => (true, None),
                    RetryClass::Http {
                        status,
                        retry_after_ms,
                    } => (retryable_status(status, staging), retry_after_ms),
                };
                if !allowed {
                    return Err(error);
                }
                let delay = Duration::from_millis(retry_delay_ms(attempt, retry_after, 0));
                tokio::select! {
                    _ = cancelled.cancelled() => return Err(anyhow::anyhow!("transfer cancelled").into()),
                    _ = sleep(delay) => {},
                }
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[tokio::test]
    async fn cancellation_interrupts_a_queued_permit_and_retry_wait() {
        let cancel = CancellationToken::new();
        let runtime = Runtime::new(1, 1, 1, cancel.clone()).unwrap();
        let permit = runtime.network().await.unwrap();
        let waiting = tokio::spawn({
            let runtime = runtime.clone();
            async move { runtime.network().await }
        });
        cancel.cancel();
        assert!(waiting.await.unwrap().is_err());
        drop(permit);

        let attempts = Arc::new(AtomicUsize::new(0));
        let cancel = CancellationToken::new();
        let task = tokio::spawn({
            let attempts = attempts.clone();
            let cancel = cancel.clone();
            async move {
                retry(
                    &cancel,
                    3,
                    false,
                    |_| {
                        attempts.fetch_add(1, Ordering::Relaxed);
                        async { Err::<(), _>(anyhow::anyhow!("retry")) }
                    },
                    |_| Some(RetryClass::Network),
                )
                .await
            }
        });
        tokio::task::yield_now().await;
        cancel.cancel();
        assert!(task.await.unwrap().is_err());
        assert_eq!(attempts.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn retry_delay_is_the_shared_capped_policy() {
        assert_eq!(retry_delay_ms(0, None, 0), 250);
        assert_eq!(retry_delay_ms(99, Some(999_999), 0), 60_000);
        assert!(!retryable_status(423, false));
        assert!(retryable_status(423, true));
    }
}
