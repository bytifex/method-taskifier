#[cfg(test)]
mod tests {
    #![allow(dead_code)]

    use std::sync::Arc;

    use method_taskifier_macros::method_taskifier_impl;
    use parking_lot::Mutex;

    use method_taskifier::prelude::ArcMutex;

    #[derive(Debug, ::serde::Serialize, ::serde::Deserialize)]
    pub enum MyWorkerError {
        DivisionByZero,
    }

    #[derive(Clone)]
    struct MyWorker {
        current_value: ArcMutex<f32>,
    }

    #[method_taskifier_impl(
        task_definitions_module_path = self,
        client_name = MyClient,
        use_serde,
        // debug,
    )]
    impl MyWorker {
        pub fn new(initial_value: f32) -> Self {
            Self {
                current_value: Arc::new(Mutex::new(initial_value)),
            }
        }

        #[method_taskifier_worker_fn]
        pub fn add(&mut self, value: f32) -> f32 {
            let mut guard = self.current_value.lock();
            *guard += value;
            *guard
        }

        #[method_taskifier_worker_fn]
        pub fn divide(&mut self, divisor: f32) -> Result<f32, MyWorkerError> {
            if divisor == 0.0 {
                Err(MyWorkerError::DivisionByZero)
            } else {
                let mut guard = self.current_value.lock();
                *guard /= divisor;
                Ok(*guard)
            }
        }

        #[method_taskifier_worker_fn]
        pub fn noop(&mut self) {}

        #[method_taskifier_worker_fn]
        pub fn mul(&mut self, mutliplier: f32) -> f32 {
            let mut guard = self.current_value.lock();
            *guard *= mutliplier;
            *guard
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn synchronously_execute_tasks() {
        let (sender, receiver) = channel();
        let client = MyClient::new(sender);
        let mut worker = MyWorker::new(7.0);

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
