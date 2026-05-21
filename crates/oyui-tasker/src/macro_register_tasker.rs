/// Registers and generates the necessary boilerplate for a background worker.
///
/// Takes a mapping of `EnumVariant => TaskPath`.
///
/// # Example
/// ```ignore
/// register_tasker! {
///     tasks = [
///         Calculate => calculate::CalcTask,
///         Fetch => fetch::FetchTask,
///     ]
/// }
/// ```
#[macro_export]
macro_rules! register_tasker {
    (
        tasks = [ $( $variant:ident => $task_path:path ),* $(,)? ]
    ) => {
        #[derive(Debug, Clone)]
        pub enum WorkerRequest {
            $(
                $variant(<$task_path as $crate::worker::WorkerTask>::Request),
            )*
            Shutdown,
        }

        pub enum WorkerEvent {
            $(
                $variant(<$task_path as $crate::worker::WorkerTask>::Response),
            )*
        }

        $(
            impl From<<$task_path as $crate::worker::WorkerTask>::Request> for WorkerRequest {
                fn from(req: <$task_path as $crate::worker::WorkerTask>::Request) -> Self {
                    WorkerRequest::$variant(req)
                }
            }
        )*

        #[derive(Clone)]
        pub struct TaskerSender {
            tx: ::tokio::sync::mpsc::UnboundedSender<WorkerRequest>,
        }

        impl TaskerSender {
            pub fn send<R>(&self, req: R) -> Result<(), ::tokio::sync::mpsc::error::SendError<WorkerRequest>>
            where R: Into<WorkerRequest> {
                let request = req.into();
                $crate::reexport::tracing::trace!(?request, "TaskerSender sending request");
                self.tx.send(request)
            }

            pub fn shutdown(&self) -> Result<(), ::tokio::sync::mpsc::error::SendError<WorkerRequest>> {
                $crate::reexport::tracing::info!("TaskerSender sending Shutdown signal");
                self.tx.send(WorkerRequest::Shutdown)
            }
        }

        pub struct TaskerReceiver {
            rx: ::tokio::sync::mpsc::UnboundedReceiver<WorkerEvent>,
        }

        impl TaskerReceiver {
            pub async fn recv(&mut self) -> Option<WorkerEvent> {
                self.rx.recv().await
            }
        }

        pub struct Tasker {
            tx: ::tokio::sync::mpsc::UnboundedSender<WorkerRequest>,
            rx: ::tokio::sync::mpsc::UnboundedReceiver<WorkerEvent>,
            handle: Option<::tokio::task::JoinHandle<()>>,
        }

        impl Tasker {
            pub fn send<R>(&self, req: R) -> Result<(), ::tokio::sync::mpsc::error::SendError<WorkerRequest>>
            where R: Into<WorkerRequest> {
                self.tx.send(req.into())
            }

            pub async fn recv(&mut self) -> Option<WorkerEvent> {
                self.rx.recv().await
            }

            pub fn try_recv(&mut self) -> Result<WorkerEvent, ::tokio::sync::mpsc::error::TryRecvError> {
                self.rx.try_recv()
            }

            pub fn into_split(self) -> (TaskerSender, TaskerReceiver, Option<::tokio::task::JoinHandle<()>>) {
                (
                  TaskerSender { tx: self.tx },
                  TaskerReceiver { rx: self.rx },
                  self.handle
                )
            }

            pub async fn shutdown(&mut self) -> Result<(), ::tokio::sync::mpsc::error::SendError<WorkerRequest>> {
                self.tx.send(WorkerRequest::Shutdown)?;
                if let Some(handle) = self.handle.take() {
                    let _ = handle.await;
                }
                Ok(())
            }

            pub fn spawn<C>(ctx: C) -> Self
            where
                C: Send + Sync + 'static,
                $( <$task_path as $crate::worker::WorkerTask>::Context: $crate::worker::ExtractsFrom<C>, )*
            {
                let c = ::std::sync::Arc::new(ctx);
                let (req_tx, mut req_rx) = ::tokio::sync::mpsc::unbounded_channel::<WorkerRequest>();
                let (ev_tx, ev_rx) = ::tokio::sync::mpsc::unbounded_channel::<WorkerEvent>();

                let handle = ::tokio::spawn(async move {
                    use $crate::reexport::tracing::Instrument;

                    async move {
                        while let Some(request) = req_rx.recv().await {
                            match request {
                                WorkerRequest::Shutdown => break,
                                $(
                                    WorkerRequest::$variant(req) => {
                                        let ctx = ::std::sync::Arc::clone(&c);
                                        let ev_tx = ev_tx.clone();

                                        ::tokio::spawn(
                                            async move {
                                                let span = $crate::reexport::tracing::info_span!(
                                                    "task_handle",
                                                    event_type = stringify!($variant),
                                                    request = ?req
                                                );

                                                async {
                                                    let task_ctx = <<$task_path as $crate::worker::WorkerTask>::Context as $crate::worker::ExtractsFrom<C>>::extract(&*ctx);
                                                    let res = <$task_path as $crate::worker::WorkerTask>::handle(req, task_ctx).await;

                                                    $crate::reexport::tracing::info!(response = ?res, "Task completed successfully");
                                                    let _ = ev_tx.send(WorkerEvent::$variant(res));
                                                }
                                                .instrument(span)
                                                .await
                                            }
                                        );
                                    }
                                )*
                            }
                        }
                    }
                    .instrument($crate::reexport::tracing::info_span!("tasker_worker_loop"))
                    .await;
                });

                Self { tx: req_tx, rx: ev_rx, handle: Some(handle) }
            }
        }
    };
}
