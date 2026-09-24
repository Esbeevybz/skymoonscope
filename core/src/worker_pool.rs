use std::{future::Future, pin::Pin, sync::Arc};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::runtime::{Builder, Runtime};

type Task = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

#[derive(Debug, Error)]
pub enum TaskQueueError {
    #[error("event worker queue is full")]
    Full,
    #[error("event worker queue is closed")]
    Closed,
}

/// A dedicated thread pool configuration for heavy contract event processing.
/// This prevents CPU-intensive parsing from blocking the main HTTP async runtime.
#[derive(Clone)]
pub struct EventWorkerPool {
    runtime: Arc<Runtime>,
    sender: mpsc::Sender<Task>,
}

impl EventWorkerPool {
    /// Initializes a new dedicated Tokio runtime for event processing.
    ///
    /// # Arguments
    /// * `worker_threads` - The number of OS threads to allocate to this pool.
    pub fn new(worker_threads: usize) -> std::io::Result<Self> {
        let runtime = Builder::new_multi_thread()
            .worker_threads(worker_threads)
            .thread_name("event-parser-worker")
            .enable_all()
            .build()?;

        let (sender, receiver) = mpsc::channel(worker_threads);
        let receiver = Arc::new(Mutex::new(receiver));
        for _ in 0..worker_threads {
            let receiver = Arc::clone(&receiver);
            runtime.spawn(async move {
                loop {
                    let task = receiver.lock().await.recv().await;
                    match task {
                        Some(task) => task.await,
                        None => break,
                    }
                }
            });
        }

        Ok(Self {
            runtime: Arc::new(runtime),
            sender,
        })
    }

    /// Spawns an async task on the dedicated event worker pool.
    pub fn spawn<F>(
        &self,
        future: F,
    ) -> Result<tokio::task::JoinHandle<F::Output>, TaskQueueError>
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (result_sender, result_receiver) = oneshot::channel();
        let task = async move {
            let result = future.await;
            let _ = result_sender.send(result);
        };
        self.sender
            .try_send(Box::pin(task))
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => TaskQueueError::Full,
                mpsc::error::TrySendError::Closed(_) => TaskQueueError::Closed,
            })?;

        Ok(self.runtime.spawn(async move {
            result_receiver
                .await
                .expect("event worker task ended before returning its result")
        }))
    }

    /// Spawns a blocking (CPU-heavy) task on the dedicated event worker pool.
    /// Use this for strict, heavy synchronous parsing logic.
    pub fn spawn_blocking<F, R>(
        &self,
        func: F,
    ) -> Result<tokio::task::JoinHandle<R>, TaskQueueError>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.spawn(async move { func() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_pool_initialization() {
        let pool_result = EventWorkerPool::new(2);
        assert!(
            pool_result.is_ok(),
            "Worker pool should initialize successfully"
        );
    }

    #[test]
    fn test_pool_spawns_async_task() {
        let pool = EventWorkerPool::new(2).expect("Failed to create worker pool");

        let result = pool.runtime.block_on(async {
            let handle = pool.spawn(async { 100 + 42 }).unwrap();
            handle.await.unwrap()
        });

        assert_eq!(
            result, 142,
            "Async task should execute and return correctly"
        );
    }

    #[test]
    fn test_pool_spawns_blocking_task() {
        let pool = EventWorkerPool::new(2).expect("Failed to create worker pool");

        let result = pool.runtime.block_on(async {
            let handle = pool.spawn_blocking(|| {
                // Simulate a heavy CPU-bound parsing task
                let mut sum = 0;
                for i in 1..=1000 {
                    sum += i;
                }
                sum
            }).unwrap();
            handle.await.unwrap()
        });

        assert_eq!(
            result, 500500,
            "Blocking CPU task should compute correctly off-thread"
        );
    }

    #[test]
    fn test_pool_rejects_tasks_when_queue_is_full() {
        let pool = EventWorkerPool::new(1).expect("Failed to create worker pool");

        pool.runtime.block_on(async {
            let (started_sender, started_receiver) = oneshot::channel();
            let (release_sender, release_receiver) = oneshot::channel();
            let first = pool
                .spawn(async move {
                    let _ = started_sender.send(());
                    let _ = release_receiver.await;
                })
                .unwrap();
            started_receiver.await.unwrap();
            let _queued = pool.spawn(async {}).unwrap();

            assert!(matches!(pool.spawn(async {}), Err(TaskQueueError::Full)));
            let _ = release_sender.send(());
            first.await.unwrap();
        });
    }
}
