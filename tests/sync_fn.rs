#[cfg(test)]
mod tests {
    #![allow(dead_code)]

    use std::sync::Arc;

    use method_taskifier_macros::method_taskifier_impl;
    use parking_lot::Mutex;

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
        pub fn mul(&mut self, mutliplier: f32) -> f32 {
            let mut guard = self.current_value.lock();
            *guard *= mutliplier;
            *guard
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn synchronously_execute_tasks() {
        let (sender, receiver) = my_async_worker::channel();
        let client = my_async_worker::Client::new(sender);
        let mut worker = MyAsyncWorker::new(7.0);

        let result_future = client.divide(2.0);
        worker.execute_channeled_task(receiver.try_recv().unwrap());
        let result = result_future.await.unwrap().unwrap();
        assert_eq!(*worker.current_value.lock(), 3.5);
        assert_eq!(result, 3.5);

        let result_future = client.add(15.0);
        worker.execute_channeled_task(receiver.try_recv().unwrap());
        let result = result_future.await.unwrap();
        assert_eq!(*worker.current_value.lock(), 18.5);
        assert_eq!(result, 18.5);

        let result_future = client.noop();
        worker.execute_channeled_task(receiver.try_recv().unwrap());
        result_future.await.unwrap();

        let result_future = client.mul(2.0);
        worker.execute_channeled_task(receiver.try_recv().unwrap());
        let result = result_future.await.unwrap();
        assert_eq!(*worker.current_value.lock(), 37.0);
        assert_eq!(result, 37.0);
    }
}
