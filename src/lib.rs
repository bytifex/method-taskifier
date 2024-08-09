use std::{fmt::Debug, future::Future, sync::Arc};

pub mod prelude;
pub mod task_channel;

use parking_lot::Mutex;
use thiserror::Error;
use tokio::{
    sync::watch::{self, Receiver, Sender},
    task::JoinHandle,
};

pub use method_taskifier_macros::method_taskifier_impl;

#[derive(Debug, Error)]
#[error("all workers are dropped")]
pub struct AllWorkersDroppedError;

#[derive(Debug, Error)]
#[error("all clients are dropped")]
pub struct AllClientsDroppedError;

#[derive(Debug, Error)]
#[error("invalid number of executors: {0}")]
pub struct InvalidNumberOfExecutors(pub u8);

pub struct AsyncWorkerRunner {
    state_sender: Sender<bool>,
    join_handles: Vec<JoinHandle<()>>,
}

async fn wait_for<T>(mut state_receiver: Receiver<T>, predicate: impl Fn(&T) -> bool) {
    while let Ok(()) = state_receiver.changed().await {
        if predicate(&*state_receiver.borrow()) {
            break;
        }
    }
}

impl AsyncWorkerRunner {
    pub fn run_worker<WorkerBuilderType, WorkerType, WorkerFutureType>(
        number_of_executors: u8,
        worker_builder: WorkerBuilderType,
    ) -> Result<Self, InvalidNumberOfExecutors>
    where
        WorkerFutureType: Future<Output = ()> + Send,
        WorkerType: (FnOnce() -> WorkerFutureType) + Send,
        WorkerBuilderType: (Fn() -> WorkerType) + Send + 'static,
    {
        let worker_builder = Arc::new(Mutex::new(worker_builder));

        if number_of_executors == 0 {
            return Err(InvalidNumberOfExecutors(number_of_executors));
        }

        let (sender, receiver) = watch::channel(true);

        let join_handles = (0..number_of_executors)
            .map(|_| {
                let worker_builder = worker_builder.clone();
                let receiver = receiver.clone();
                tokio::spawn(async move {
                    // the following loop is there to restart the worker in case of a panic or in case the worker finished
                    while {
                        let worker_builder = worker_builder.clone();
                        let receiver = receiver.clone();
                        let task_result = tokio::spawn(async move {
                            log::info!("Starting worker");

                            let worker = worker_builder.lock()();

                            let restart_worker = tokio::select! {
                                _ = worker() => {
                                    true
                                }
                                _ = wait_for(receiver, |value_ref| !*value_ref) => {
                                    false
                                }
                            };
                            restart_worker
                        });

                        match task_result.await {
                            Ok(restart_worker) => restart_worker,
                            Err(e) => {
                                log::error!("Worker exited with error: {e}");
                                true
                            }
                        }
                    } {}
                })
            })
            .collect();

        Ok(AsyncWorkerRunner {
            state_sender: sender,
            join_handles,
        })
    }

    pub fn stop(&self) {
        let _ = self.state_sender.send(false);
    }

    pub async fn join(self) {
        for join_handle in self.join_handles {
            let _ = join_handle.await;
        }
    }
}

#[cfg(test)]
mod test {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use tokio::sync::mpsc;

    use crate::AsyncWorkerRunner;

    #[tokio::test]
    async fn worker_restart_after_panic() {
        let (sender, mut receiver) = mpsc::unbounded_channel::<()>();

        let counter = Arc::new(AtomicUsize::new(0));
        let worker_builder = {
            let counter = counter.clone();
            move || {
                let counter = counter.clone();
                let sender = sender.clone();
                || async move {
                    let prev_value = counter.fetch_add(1, Ordering::SeqCst);
                    if prev_value == 0 {
                        panic!()
                    } else {
                        let _ = sender.send(());
                    }
                }
            }
        };

        let worker_runner = AsyncWorkerRunner::run_worker(1, worker_builder).unwrap();

        receiver.recv().await;

        worker_runner.stop();
        worker_runner.join().await;

        assert!(counter.load(Ordering::SeqCst) > 1);
    }
}
