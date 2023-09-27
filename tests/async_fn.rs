#[cfg(test)]
mod tests {
    #![allow(dead_code)]

    use std::{sync::Arc, time::Duration};

    use method_taskifier_macros::method_taskifier_impl;
    use parking_lot::Mutex;
    use tokio::time::{sleep_until, Instant};

    use method_taskifier::prelude::ArcMutex;

    #[derive(Debug, ::serde::Serialize, ::serde::Deserialize)]
    pub enum MyAsyncWorkerError {
        DivisionByZero,
    }

    #[derive(Clone)]
    struct MyAsyncWorker {
        current_value: ArcMutex<f32>,
    }

    #[method_taskifier_impl(
        module_name = my_async_worker,
        use_serde,
        // debug,
    )]
    impl MyAsyncWorker {
        pub fn new(initial_value: f32) -> Self {
            Self {
                current_value: Arc::new(Mutex::new(initial_value)),
            }
        }

        #[method_taskifier_fn]
        pub fn add(&mut self, value: f32) -> f32 {
            let mut guard = self.current_value.lock();
            *guard += value;
            *guard
        }

        #[method_taskifier_fn]
        pub fn divide(&mut self, divisor: f32) -> Result<f32, MyAsyncWorkerError> {
            if divisor == 0.0 {
                Err(MyAsyncWorkerError::DivisionByZero)
            } else {
                let mut guard = self.current_value.lock();
                *guard /= divisor;
                Ok(*guard)
            }
        }

        #[method_taskifier_fn]
        pub fn noop(&mut self) {}

        #[method_taskifier_fn]
        pub async fn async_mul(&mut self, mutliplier: f32) -> f32 {
            let mut guard = self.current_value.lock();
            *guard *= mutliplier;
            *guard
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn caling_worker_method_directly() {
        let mut worker = MyAsyncWorker::new(7.0);
        assert_eq!(*worker.current_value.lock(), 7.0);

        let result = worker.divide(2.0).unwrap();
        assert_eq!(*worker.current_value.lock(), 3.5);
        assert_eq!(result, 3.5);

        let result = worker.add(15.0);
        assert_eq!(*worker.current_value.lock(), 18.5);
        assert_eq!(result, 18.5);

        worker.noop();

        let result = worker.async_mul(2.0).await;
        assert_eq!(*worker.current_value.lock(), 37.0);
        assert_eq!(result, 37.0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn single_async_worker() {
        let (sender, mut receiver) = my_async_worker::channel();
        let client = my_async_worker::Client::new(sender);
        let mut worker = MyAsyncWorker::new(7.0);

        let client_task = {
            let worker = worker.clone();
            tokio::spawn(async move {
                let result = client.divide(2.0).await.unwrap().unwrap();
                assert_eq!(*worker.current_value.lock(), 3.5);
                assert_eq!(result, 3.5);

                let result = client.add(15.0).await.unwrap();
                assert_eq!(*worker.current_value.lock(), 18.5);
                assert_eq!(result, 18.5);

                client.noop().await.unwrap();

                let result = client.async_mul(2.0).await.unwrap();
                assert_eq!(*worker.current_value.lock(), 37.0);
                assert_eq!(result, 37.0);
            })
        };

        tokio::spawn(async move {
            while let Ok(task) = receiver.recv_async().await {
                worker.execute_channeled_task(task).await;
            }
        });

        let timeout = Duration::from_secs(5);
        let timestamp = Instant::now() + timeout;
        tokio::select! {
            _ = sleep_until(timestamp) => {
                assert!(false, "timed out");
            }
            ret = client_task => {
                ret.unwrap();
            }
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn execute_task() {
        let (sender, mut receiver) = my_async_worker::channel();
        let client = my_async_worker::Client::new(sender);
        let mut worker = MyAsyncWorker::new(7.0);

        let client_task = {
            let worker = worker.clone();
            tokio::spawn(async move {
                let result = client
                    .execute_task(my_async_worker::Task::divide(2.0))
                    .await
                    .unwrap();
                assert_eq!(*worker.current_value.lock(), 3.5);
                assert_eq!(*result.as_divide_result().unwrap().0.as_ref().unwrap(), 3.5);

                let result = client
                    .execute_task(my_async_worker::Task::add(15.0))
                    .await
                    .unwrap();
                assert_eq!(*worker.current_value.lock(), 18.5);
                assert_eq!(result.as_add_result().unwrap().0, 18.5);

                let result = client
                    .execute_task(my_async_worker::Task::noop())
                    .await
                    .unwrap();
                assert_eq!(result.as_noop_result().unwrap().0, ());

                let result = client
                    .execute_task(my_async_worker::Task::async_mul(2.0))
                    .await
                    .unwrap();
                assert_eq!(*worker.current_value.lock(), 37.0);
                assert_eq!(result.as_async_mul_result().unwrap().0, 37.0);
            })
        };

        tokio::spawn(async move {
            let _ = worker
                .execute_channeled_tasks_from_queue_until_clients_dropped(&mut receiver)
                .await;
        });

        let timeout = Duration::from_secs(5);
        let timestamp = Instant::now() + timeout;
        tokio::select! {
            _ = sleep_until(timestamp) => {
                assert!(false, "timed out");
            }
            ret = client_task => {
                ret.unwrap();
            }
        }
    }
}
