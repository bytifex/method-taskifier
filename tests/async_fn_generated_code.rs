#[cfg(test)]
mod tests {
    #![allow(dead_code)]

    use std::{sync::Arc, time::Duration};

    use parking_lot::Mutex;
    use tokio::time::{sleep_until, Instant};

    use method_taskifier::{prelude::ArcMutex, AllWorkersDroppedError};

    #[derive(Debug, ::serde::Serialize, ::serde::Deserialize)]
    pub enum MyWorkerError {
        DivisionByZero,
    }

    #[derive(Clone)]
    struct MyWorker {
        current_value: ArcMutex<f32>,
    }
    impl MyWorker {
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
        pub fn divide(&mut self, divisor: f32) -> Result<f32, MyWorkerError> {
            if divisor == 0.0 {
                Err(MyWorkerError::DivisionByZero)
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
        pub fn non_taskified_fn(&mut self) {}
    }
    impl MyWorker {
        pub async fn execute_task(&mut self, task: self::Task) -> self::TaskResult {
            match task {
                self::Task::Add(self::AddParams { value }) => {
                    ::log::trace!("Executing channeled task = {}::self::Add", module_path!());
                    let ret = self.add(value);
                    self::TaskResult::Add(self::AddResult(ret))
                }
                self::Task::Divide(self::DivideParams { divisor }) => {
                    ::log::trace!(
                        "Executing channeled task = {}::self::Divide",
                        module_path!()
                    );
                    let ret = self.divide(divisor);
                    self::TaskResult::Divide(self::DivideResult(ret))
                }
                self::Task::Noop(self::NoopParams {}) => {
                    ::log::trace!("Executing channeled task = {}::self::Noop", module_path!());
                    let ret = self.noop();
                    self::TaskResult::Noop(self::NoopResult(ret))
                }
                self::Task::AsyncMul(self::AsyncMulParams { mutliplier }) => {
                    ::log::trace!(
                        "Executing channeled task = {}::self::AsyncMul",
                        module_path!()
                    );
                    let ret = self.async_mul(mutliplier).await;
                    self::TaskResult::AsyncMul(self::AsyncMulResult(ret))
                }
            }
        }
        pub async fn execute_channeled_task(&mut self, task: self::ChanneledTask) {
            match task {
                self::ChanneledTask::Add {
                    result_sender,
                    params: self::AddParams { value },
                } => {
                    ::log::trace!("Executing channeled task = {}::self::Add", module_path!());
                    let ret = self.add(value);
                    ::log::trace!(
                        "Sending channeled task result = {}::self::Add",
                        module_path!()
                    );
                    let _ = result_sender.send(self::AddResult(ret));
                }
                self::ChanneledTask::Divide {
                    result_sender,
                    params: self::DivideParams { divisor },
                } => {
                    ::log::trace!(
                        "Executing channeled task = {}::self::Divide",
                        module_path!()
                    );
                    let ret = self.divide(divisor);
                    ::log::trace!(
                        "Sending channeled task result = {}::self::Divide",
                        module_path!()
                    );
                    let _ = result_sender.send(self::DivideResult(ret));
                }
                self::ChanneledTask::Noop {
                    result_sender,
                    params: self::NoopParams {},
                } => {
                    ::log::trace!("Executing channeled task = {}::self::Noop", module_path!());
                    let ret = self.noop();
                    ::log::trace!(
                        "Sending channeled task result = {}::self::Noop",
                        module_path!()
                    );
                    let _ = result_sender.send(self::NoopResult(ret));
                }
                self::ChanneledTask::AsyncMul {
                    result_sender,
                    params: self::AsyncMulParams { mutliplier },
                } => {
                    ::log::trace!(
                        "Executing channeled task = {}::self::AsyncMul",
                        module_path!()
                    );
                    let ret = self.async_mul(mutliplier).await;
                    ::log::trace!(
                        "Sending channeled task result = {}::self::AsyncMul",
                        module_path!()
                    );
                    let _ = result_sender.send(self::AsyncMulResult(ret));
                }
            }
        }
        pub async fn try_execute_channeled_task_from_queue(
            &mut self,
            receiver: &::method_taskifier::task_channel::TaskReceiver<self::ChanneledTask>,
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
            receiver: &mut ::method_taskifier::task_channel::TaskReceiver<self::ChanneledTask>,
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
            receiver: &::method_taskifier::task_channel::TaskReceiver<self::ChanneledTask>,
        ) -> Result<(), ::method_taskifier::AllClientsDroppedError> {
            while self.try_execute_channeled_task_from_queue(receiver).await? {}
            Ok(())
        }
        pub async fn execute_channeled_tasks_from_queue_until_clients_dropped(
            &mut self,
            receiver: &mut ::method_taskifier::task_channel::TaskReceiver<self::ChanneledTask>,
        ) -> Result<(), ::method_taskifier::AllClientsDroppedError> {
            loop {
                self.execute_channeled_task_from_queue(receiver).await?
            }
        }
    }

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
    pub struct DivideResult(pub Result<f32, MyWorkerError>);
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
    pub struct MyClient {
        task_sender: ::method_taskifier::task_channel::TaskSender<ChanneledTask>,
    }
    impl MyClient {
        pub fn new(sender: ::method_taskifier::task_channel::TaskSender<ChanneledTask>) -> Self {
            Self {
                task_sender: sender,
            }
        }
        pub async fn execute_task(
            &self,
            task: self::Task,
        ) -> Result<self::TaskResult, ::method_taskifier::AllWorkersDroppedError> {
            match task {
                self::Task::Add(params) => {
                    ::log::trace!("{}::Client::add, sending task to workers", module_path!());
                    let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
                    let ret = self.task_sender.send(self::ChanneledTask::Add {
                        params,
                        result_sender,
                    });
                    if let Err(e) = ret {
                        ::log::error!("{}::Client::add, msg = {:?}", module_path!(), e);
                        return Err(::method_taskifier::AllWorkersDroppedError);
                    }
                    ::log::trace!("{}::Client::add, waiting for task result", module_path!());
                    match result_receiver.await {
                        Ok(ret) => Ok(self::TaskResult::Add(ret)),
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
                self::Task::Divide(params) => {
                    ::log::trace!(
                        "{}::Client::divide, sending task to workers",
                        module_path!()
                    );
                    let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
                    let ret = self.task_sender.send(self::ChanneledTask::Divide {
                        params,
                        result_sender,
                    });
                    if let Err(e) = ret {
                        ::log::error!("{}::Client::divide, msg = {:?}", module_path!(), e);
                        return Err(::method_taskifier::AllWorkersDroppedError);
                    }
                    ::log::trace!(
                        "{}::Client::divide, waiting for task result",
                        module_path!()
                    );
                    match result_receiver.await {
                        Ok(ret) => Ok(self::TaskResult::Divide(ret)),
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
                self::Task::Noop(params) => {
                    ::log::trace!("{}::Client::noop, sending task to workers", module_path!());
                    let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
                    let ret = self.task_sender.send(self::ChanneledTask::Noop {
                        params,
                        result_sender,
                    });
                    if let Err(e) = ret {
                        ::log::error!("{}::Client::noop, msg = {:?}", module_path!(), e);
                        return Err(::method_taskifier::AllWorkersDroppedError);
                    }
                    ::log::trace!("{}::Client::noop, waiting for task result", module_path!());
                    match result_receiver.await {
                        Ok(ret) => Ok(self::TaskResult::Noop(ret)),
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
                self::Task::AsyncMul(params) => {
                    ::log::trace!(
                        "{}::Client::async_mul, sending task to workers",
                        module_path!()
                    );
                    let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
                    let ret = self.task_sender.send(self::ChanneledTask::AsyncMul {
                        params,
                        result_sender,
                    });
                    if let Err(e) = ret {
                        ::log::error!("{}::Client::async_mul, msg = {:?}", module_path!(), e);
                        return Err(::method_taskifier::AllWorkersDroppedError);
                    }
                    ::log::trace!(
                        "{}::Client::async_mul, waiting for task result",
                        module_path!()
                    );
                    match result_receiver.await {
                        Ok(ret) => Ok(self::TaskResult::AsyncMul(ret)),
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
        pub fn add(
            &self,
            value: f32,
        ) -> impl ::std::future::Future<Output = Result<f32, ::method_taskifier::AllWorkersDroppedError>>
        {
            ::log::trace!("{}::Client::add, sending task to workers", module_path!());
            let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
            let ret = self.task_sender.send(ChanneledTask::Add {
                params: AddParams { value },
                result_sender,
            });
            async move {
                if let Err(e) = ret {
                    ::log::error!("{}::Client::add, msg = {:?}", module_path!(), e);
                    return Err(::method_taskifier::AllWorkersDroppedError);
                }
                ::log::trace!("{}::Client::add, waiting for task result", module_path!());
                match result_receiver.await {
                    Ok(ret) => Ok(ret.0),
                    Err(e) => {
                        ::log::error!("{}::Client::add response, msg = {:?}", module_path!(), e);
                        Err(::method_taskifier::AllWorkersDroppedError)
                    }
                }
            }
        }
        pub fn divide(
            &self,
            divisor: f32,
        ) -> impl ::std::future::Future<
            Output = Result<Result<f32, MyWorkerError>, ::method_taskifier::AllWorkersDroppedError>,
        > {
            ::log::trace!(
                "{}::Client::divide, sending task to workers",
                module_path!()
            );
            let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
            let ret = self.task_sender.send(ChanneledTask::Divide {
                params: DivideParams { divisor },
                result_sender,
            });
            async move {
                if let Err(e) = ret {
                    ::log::error!("{}::Client::divide, msg = {:?}", module_path!(), e);
                    return Err(::method_taskifier::AllWorkersDroppedError);
                }
                ::log::trace!(
                    "{}::Client::divide, waiting for task result",
                    module_path!()
                );
                match result_receiver.await {
                    Ok(ret) => Ok(ret.0),
                    Err(e) => {
                        ::log::error!("{}::Client::divide response, msg = {:?}", module_path!(), e);
                        Err(::method_taskifier::AllWorkersDroppedError)
                    }
                }
            }
        }
        pub fn noop(
            &self,
        ) -> impl ::std::future::Future<Output = Result<(), ::method_taskifier::AllWorkersDroppedError>>
        {
            ::log::trace!("{}::Client::noop, sending task to workers", module_path!());
            let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
            let ret = self.task_sender.send(ChanneledTask::Noop {
                params: NoopParams {},
                result_sender,
            });
            async move {
                if let Err(e) = ret {
                    ::log::error!("{}::Client::noop, msg = {:?}", module_path!(), e);
                    return Err(::method_taskifier::AllWorkersDroppedError);
                }
                ::log::trace!("{}::Client::noop, waiting for task result", module_path!());
                match result_receiver.await {
                    Ok(ret) => Ok(ret.0),
                    Err(e) => {
                        ::log::error!("{}::Client::noop response, msg = {:?}", module_path!(), e);
                        Err(::method_taskifier::AllWorkersDroppedError)
                    }
                }
            }
        }
        pub fn async_mul(
            &self,
            mutliplier: f32,
        ) -> impl ::std::future::Future<Output = Result<f32, ::method_taskifier::AllWorkersDroppedError>>
        {
            ::log::trace!(
                "{}::Client::async_mul, sending task to workers",
                module_path!()
            );
            let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
            let ret = self.task_sender.send(ChanneledTask::AsyncMul {
                params: AsyncMulParams { mutliplier },
                result_sender,
            });
            async move {
                if let Err(e) = ret {
                    ::log::error!("{}::Client::async_mul, msg = {:?}", module_path!(), e);
                    return Err(::method_taskifier::AllWorkersDroppedError);
                }
                ::log::trace!(
                    "{}::Client::async_mul, waiting for task result",
                    module_path!()
                );
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
        async fn manual_add_caller(&mut self, value: &f32) -> Result<f32, AllWorkersDroppedError> {
            {
                self.add(*value).await
            }
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn caling_worker_method_directly() {
        let mut worker = MyWorker::new(7.0);
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

        worker.non_taskified_fn();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn single_async_worker() {
        let (sender, mut receiver) = channel();
        let mut client = MyClient::new(sender);
        let mut worker = MyWorker::new(7.0);

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

                let result = client.manual_add_caller(&5.0).await.unwrap();
                assert_eq!(*worker.current_value.lock(), 42.0);
                assert_eq!(result, 42.0);
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
        let (sender, mut receiver) = channel();
        let client = MyClient::new(sender);
        let mut worker = MyWorker::new(7.0);

        let client_task = {
            let worker = worker.clone();
            tokio::spawn(async move {
                let result = client.execute_task(Task::divide(2.0)).await.unwrap();
                assert_eq!(*worker.current_value.lock(), 3.5);
                assert_eq!(*result.as_divide_result().unwrap().0.as_ref().unwrap(), 3.5);

                let result = client.execute_task(Task::add(15.0)).await.unwrap();
                assert_eq!(*worker.current_value.lock(), 18.5);
                assert_eq!(result.as_add_result().unwrap().0, 18.5);

                let result = client.execute_task(Task::noop()).await.unwrap();
                assert_eq!(result.as_noop_result().unwrap().0, ());

                let result = client.execute_task(Task::async_mul(2.0)).await.unwrap();
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
