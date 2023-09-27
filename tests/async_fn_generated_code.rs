#[cfg(test)]
mod tests {
    #![allow(dead_code)]

    use std::{sync::Arc, time::Duration};

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
    impl MyAsyncWorker {
        pub fn new(initial_value: f32) -> Self {
            Self {
                current_value: Arc::new(Mutex::new(initial_value)),
            }
        }
        pub fn add(&mut self, value: f32) -> f32 {
            let mut guard = self.current_value.lock();
            *guard += value;
            *guard
        }
        pub fn divide(&mut self, divisor: f32) -> Result<f32, MyAsyncWorkerError> {
            if divisor == 0.0 {
                Err(MyAsyncWorkerError::DivisionByZero)
            } else {
                let mut guard = self.current_value.lock();
                *guard /= divisor;
                Ok(*guard)
            }
        }
        pub fn noop(&mut self) {}
        pub async fn async_mul(&mut self, mutliplier: f32) -> f32 {
            let mut guard = self.current_value.lock();
            *guard *= mutliplier;
            *guard
        }
    }
    impl MyAsyncWorker {
        pub async fn execute_task(
            &mut self,
            task: self::my_async_worker::Task,
        ) -> self::my_async_worker::TaskResult {
            match task {
                self::my_async_worker::Task::Add(self::my_async_worker::AddParams { value }) => {
                    let ret = self.add(value);
                    self::my_async_worker::TaskResult::Add(self::my_async_worker::AddResult(ret))
                }
                self::my_async_worker::Task::Divide(self::my_async_worker::DivideParams {
                    divisor,
                }) => {
                    let ret = self.divide(divisor);
                    self::my_async_worker::TaskResult::Divide(self::my_async_worker::DivideResult(
                        ret,
                    ))
                }
                self::my_async_worker::Task::Noop(self::my_async_worker::NoopParams {}) => {
                    let ret = self.noop();
                    self::my_async_worker::TaskResult::Noop(self::my_async_worker::NoopResult(ret))
                }
                self::my_async_worker::Task::AsyncMul(self::my_async_worker::AsyncMulParams {
                    mutliplier,
                }) => {
                    let ret = self.async_mul(mutliplier).await;
                    self::my_async_worker::TaskResult::AsyncMul(
                        self::my_async_worker::AsyncMulResult(ret),
                    )
                }
            }
        }
        pub async fn execute_channeled_task(&mut self, task: self::my_async_worker::ChanneledTask) {
            match task {
                self::my_async_worker::ChanneledTask::Add {
                    result_sender,
                    params: self::my_async_worker::AddParams { value },
                } => {
                    let ret = self.add(value);
                    let _ = result_sender.send(self::my_async_worker::AddResult(ret));
                }
                self::my_async_worker::ChanneledTask::Divide {
                    result_sender,
                    params: self::my_async_worker::DivideParams { divisor },
                } => {
                    let ret = self.divide(divisor);
                    let _ = result_sender.send(self::my_async_worker::DivideResult(ret));
                }
                self::my_async_worker::ChanneledTask::Noop {
                    result_sender,
                    params: self::my_async_worker::NoopParams {},
                } => {
                    let ret = self.noop();
                    let _ = result_sender.send(self::my_async_worker::NoopResult(ret));
                }
                self::my_async_worker::ChanneledTask::AsyncMul {
                    result_sender,
                    params: self::my_async_worker::AsyncMulParams { mutliplier },
                } => {
                    let ret = self.async_mul(mutliplier).await;
                    let _ = result_sender.send(self::my_async_worker::AsyncMulResult(ret));
                }
            }
        }
        pub async fn try_execute_channeled_task_from_queue(
            &mut self,
            receiver: &mut ::method_taskifier::task_channel::TaskReceiver<
                self::my_async_worker::ChanneledTask,
            >,
        ) -> Result<bool, ::method_taskifier::AllClientsDroppedError> {
            let task = receiver.try_recv();
            match task {
                Ok(task) => {
                    self.execute_channeled_task(task).await;
                    return Ok(true);
                }
                Err(::method_taskifier::task_channel::TryRecvError::Empty) => return Ok(false),
                Err(::method_taskifier::task_channel::TryRecvError::Disconnected) => {
                    return Err(::method_taskifier::AllClientsDroppedError)
                }
            }
        }
        pub async fn execute_channeled_task_from_queue(
            &mut self,
            receiver: &mut ::method_taskifier::task_channel::TaskReceiver<
                self::my_async_worker::ChanneledTask,
            >,
        ) -> Result<(), ::method_taskifier::AllClientsDroppedError> {
            let task = receiver.recv_async().await;
            match task {
                Ok(task) => {
                    self.execute_channeled_task(task).await;
                    Ok(())
                }
                Err(::method_taskifier::task_channel::RecvError::Disconnected) => {
                    Err(::method_taskifier::AllClientsDroppedError)
                }
            }
        }
        pub async fn execute_remaining_channeled_tasks_from_queue(
            &mut self,
            receiver: &mut ::method_taskifier::task_channel::TaskReceiver<
                self::my_async_worker::ChanneledTask,
            >,
        ) -> Result<(), ::method_taskifier::AllClientsDroppedError> {
            while self.try_execute_channeled_task_from_queue(receiver).await? {}
            Ok(())
        }
        pub async fn execute_channeled_tasks_from_queue_until_clients_dropped(
            &mut self,
            receiver: &mut ::method_taskifier::task_channel::TaskReceiver<
                self::my_async_worker::ChanneledTask,
            >,
        ) -> Result<(), ::method_taskifier::AllClientsDroppedError> {
            loop {
                self.execute_channeled_task_from_queue(receiver).await?
            }
        }
    }
    pub mod my_async_worker {
        use super::*;
        #[derive(:: serde :: Serialize, :: serde :: Deserialize)]
        pub struct AddParams {
            pub value: f32,
        }
        #[derive(:: serde :: Serialize, :: serde :: Deserialize)]
        pub struct AddResult(pub f32);
        #[derive(:: serde :: Serialize, :: serde :: Deserialize)]
        pub struct DivideParams {
            pub divisor: f32,
        }
        #[derive(:: serde :: Serialize, :: serde :: Deserialize)]
        pub struct DivideResult(pub Result<f32, MyAsyncWorkerError>);
        #[derive(:: serde :: Serialize, :: serde :: Deserialize)]
        pub struct NoopParams {}
        #[derive(:: serde :: Serialize, :: serde :: Deserialize)]
        pub struct NoopResult(pub ());
        #[derive(:: serde :: Serialize, :: serde :: Deserialize)]
        pub struct AsyncMulParams {
            pub mutliplier: f32,
        }
        #[derive(:: serde :: Serialize, :: serde :: Deserialize)]
        pub struct AsyncMulResult(pub f32);
        #[derive(:: serde :: Serialize, :: serde :: Deserialize)]
        pub enum Task {
            Add(AddParams),
            Divide(DivideParams),
            Noop(NoopParams),
            AsyncMul(AsyncMulParams),
        }
        impl Task {
            pub fn add(value: f32) -> self::Task {
                self::Task::Add(self::AddParams { value })
            }
            pub fn divide(divisor: f32) -> self::Task {
                self::Task::Divide(self::DivideParams { divisor })
            }
            pub fn noop() -> self::Task {
                self::Task::Noop(self::NoopParams {})
            }
            pub fn async_mul(mutliplier: f32) -> self::Task {
                self::Task::AsyncMul(self::AsyncMulParams { mutliplier })
            }
        }
        #[derive(:: serde :: Serialize, :: serde :: Deserialize)]
        pub enum TaskResult {
            Add(AddResult),
            Divide(DivideResult),
            Noop(NoopResult),
            AsyncMul(AsyncMulResult),
        }
        impl TaskResult {
            pub fn as_add_result(&self) -> Option<&self::AddResult> {
                if let self::TaskResult::Add(result) = self {
                    Some(result)
                } else {
                    None
                }
            }
            pub fn as_divide_result(&self) -> Option<&self::DivideResult> {
                if let self::TaskResult::Divide(result) = self {
                    Some(result)
                } else {
                    None
                }
            }
            pub fn as_noop_result(&self) -> Option<&self::NoopResult> {
                if let self::TaskResult::Noop(result) = self {
                    Some(result)
                } else {
                    None
                }
            }
            pub fn as_async_mul_result(&self) -> Option<&self::AsyncMulResult> {
                if let self::TaskResult::AsyncMul(result) = self {
                    Some(result)
                } else {
                    None
                }
            }
        }
        pub enum ChanneledTask {
            Add {
                params: AddParams,
                result_sender: ::tokio::sync::oneshot::Sender<AddResult>,
            },
            Divide {
                params: DivideParams,
                result_sender: ::tokio::sync::oneshot::Sender<DivideResult>,
            },
            Noop {
                params: NoopParams,
                result_sender: ::tokio::sync::oneshot::Sender<NoopResult>,
            },
            AsyncMul {
                params: AsyncMulParams,
                result_sender: ::tokio::sync::oneshot::Sender<AsyncMulResult>,
            },
        }
        pub fn channel() -> (
            ::method_taskifier::task_channel::TaskSender<ChanneledTask>,
            ::method_taskifier::task_channel::TaskReceiver<ChanneledTask>,
        ) {
            ::method_taskifier::task_channel::task_channel()
        }
        #[derive(Clone)]
        pub struct Client {
            task_sender: ::method_taskifier::task_channel::TaskSender<ChanneledTask>,
        }
        impl Client {
            pub fn new(
                sender: ::method_taskifier::task_channel::TaskSender<ChanneledTask>,
            ) -> Self {
                Self {
                    task_sender: sender,
                }
            }
            pub async fn execute_task(
                &self,
                task: self::my_async_worker::Task,
            ) -> Result<self::my_async_worker::TaskResult, ::method_taskifier::AllWorkersDroppedError>
            {
                match task {
                    self::my_async_worker::Task::Add(params) => {
                        let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
                        let ret =
                            self.task_sender
                                .send(self::my_async_worker::ChanneledTask::Add {
                                    params,
                                    result_sender,
                                });
                        if let Err(e) = ret {
                            ::log::error!("{}::Client::add, msg = {:?}", module_path!(), e);
                            return Err(::method_taskifier::AllWorkersDroppedError);
                        }
                        match result_receiver.await {
                            Ok(ret) => Ok(self::my_async_worker::TaskResult::Add(ret)),
                            Err(e) => {
                                ::log::error!(
                                    "{}::Client::add response, msg = {:?}",
                                    module_path!(),
                                    e
                                );
                                Err(::method_taskifier::AllWorkersDroppedError)
                            }
                        }
                    }
                    self::my_async_worker::Task::Divide(params) => {
                        let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
                        let ret =
                            self.task_sender
                                .send(self::my_async_worker::ChanneledTask::Divide {
                                    params,
                                    result_sender,
                                });
                        if let Err(e) = ret {
                            ::log::error!("{}::Client::divide, msg = {:?}", module_path!(), e);
                            return Err(::method_taskifier::AllWorkersDroppedError);
                        }
                        match result_receiver.await {
                            Ok(ret) => Ok(self::my_async_worker::TaskResult::Divide(ret)),
                            Err(e) => {
                                ::log::error!(
                                    "{}::Client::divide response, msg = {:?}",
                                    module_path!(),
                                    e
                                );
                                Err(::method_taskifier::AllWorkersDroppedError)
                            }
                        }
                    }
                    self::my_async_worker::Task::Noop(params) => {
                        let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
                        let ret =
                            self.task_sender
                                .send(self::my_async_worker::ChanneledTask::Noop {
                                    params,
                                    result_sender,
                                });
                        if let Err(e) = ret {
                            ::log::error!("{}::Client::noop, msg = {:?}", module_path!(), e);
                            return Err(::method_taskifier::AllWorkersDroppedError);
                        }
                        match result_receiver.await {
                            Ok(ret) => Ok(self::my_async_worker::TaskResult::Noop(ret)),
                            Err(e) => {
                                ::log::error!(
                                    "{}::Client::noop response, msg = {:?}",
                                    module_path!(),
                                    e
                                );
                                Err(::method_taskifier::AllWorkersDroppedError)
                            }
                        }
                    }
                    self::my_async_worker::Task::AsyncMul(params) => {
                        let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
                        let ret =
                            self.task_sender
                                .send(self::my_async_worker::ChanneledTask::AsyncMul {
                                    params,
                                    result_sender,
                                });
                        if let Err(e) = ret {
                            ::log::error!("{}::Client::async_mul, msg = {:?}", module_path!(), e);
                            return Err(::method_taskifier::AllWorkersDroppedError);
                        }
                        match result_receiver.await {
                            Ok(ret) => Ok(self::my_async_worker::TaskResult::AsyncMul(ret)),
                            Err(e) => {
                                ::log::error!(
                                    "{}::Client::async_mul response, msg = {:?}",
                                    module_path!(),
                                    e
                                );
                                Err(::method_taskifier::AllWorkersDroppedError)
                            }
                        }
                    }
                }
            }
            pub async fn add(
                &self,
                value: f32,
            ) -> Result<f32, ::method_taskifier::AllWorkersDroppedError> {
                let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
                let ret = self.task_sender.send(ChanneledTask::Add {
                    params: AddParams { value },
                    result_sender,
                });
                if let Err(e) = ret {
                    ::log::error!("{}::Client::add, msg = {:?}", module_path!(), e);
                    return Err(::method_taskifier::AllWorkersDroppedError);
                }
                match result_receiver.await {
                    Ok(ret) => Ok(ret.0),
                    Err(e) => {
                        ::log::error!("{}::Client::add response, msg = {:?}", module_path!(), e);
                        Err(::method_taskifier::AllWorkersDroppedError)
                    }
                }
            }
            pub async fn divide(
                &self,
                divisor: f32,
            ) -> Result<Result<f32, MyAsyncWorkerError>, ::method_taskifier::AllWorkersDroppedError>
            {
                let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
                let ret = self.task_sender.send(ChanneledTask::Divide {
                    params: DivideParams { divisor },
                    result_sender,
                });
                if let Err(e) = ret {
                    ::log::error!("{}::Client::divide, msg = {:?}", module_path!(), e);
                    return Err(::method_taskifier::AllWorkersDroppedError);
                }
                match result_receiver.await {
                    Ok(ret) => Ok(ret.0),
                    Err(e) => {
                        ::log::error!("{}::Client::divide response, msg = {:?}", module_path!(), e);
                        Err(::method_taskifier::AllWorkersDroppedError)
                    }
                }
            }
            pub async fn noop(&self) -> Result<(), ::method_taskifier::AllWorkersDroppedError> {
                let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
                let ret = self.task_sender.send(ChanneledTask::Noop {
                    params: NoopParams {},
                    result_sender,
                });
                if let Err(e) = ret {
                    ::log::error!("{}::Client::noop, msg = {:?}", module_path!(), e);
                    return Err(::method_taskifier::AllWorkersDroppedError);
                }
                match result_receiver.await {
                    Ok(ret) => Ok(ret.0),
                    Err(e) => {
                        ::log::error!("{}::Client::noop response, msg = {:?}", module_path!(), e);
                        Err(::method_taskifier::AllWorkersDroppedError)
                    }
                }
            }
            pub async fn async_mul(
                &self,
                mutliplier: f32,
            ) -> Result<f32, ::method_taskifier::AllWorkersDroppedError> {
                let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
                let ret = self.task_sender.send(ChanneledTask::AsyncMul {
                    params: AsyncMulParams { mutliplier },
                    result_sender,
                });
                if let Err(e) = ret {
                    ::log::error!("{}::Client::async_mul, msg = {:?}", module_path!(), e);
                    return Err(::method_taskifier::AllWorkersDroppedError);
                }
                match result_receiver.await {
                    Ok(ret) => Ok(ret.0),
                    Err(e) => {
                        ::log::error!(
                            "{}::Client::async_mul response, msg = {:?}",
                            module_path!(),
                            e
                        );
                        Err(::method_taskifier::AllWorkersDroppedError)
                    }
                }
            }
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
