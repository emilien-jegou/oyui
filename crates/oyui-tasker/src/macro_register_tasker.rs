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
                $variant(<$task_path as ::oyui_tasker::worker::WorkerTask>::Request),
            )*
            Shutdown,
        }

        pub enum WorkerEvent {
            $(
                $variant(<$task_path as ::oyui_tasker::worker::WorkerTask>::Response),
            )*
        }

        $(
            impl From<<$task_path as ::oyui_tasker::worker::WorkerTask>::Request> for WorkerRequest {
                fn from(req: <$task_path as ::oyui_tasker::worker::WorkerTask>::Request) -> Self {
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
                self.tx.send(req.into())
            }

            pub fn shutdown(&self) -> Result<(), ::tokio::sync::mpsc::error::SendError<WorkerRequest>> {
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

            pub async fn shutdown(&mut self) -> Result<(), ::tokio::sync::mpsc::error::SendError<WorkerRequest>> {
                self.tx.send(WorkerRequest::Shutdown)?;
                if let Some(handle) = self.handle.take() {
                    let _ = handle.await;
                }
                Ok(())
            }

            pub fn split(&self) -> (TaskerSender, TaskerReceiver) {
                // Note: tokio::sync::mpsc::Receiver is NOT Cloneable. 
                // We provide split() by consuming the Tasker or using a different pattern if needed.
                // For now, this requires &mut self or unique ownership.
                unimplemented!("MPSC Receiver cannot be cloned. Use into_split() or share the Tasker via Arc.")
            }

            pub fn into_split(self) -> (TaskerSender, TaskerReceiver, Option<::tokio::task::JoinHandle<()>>) {
                (
                    TaskerSender { tx: self.tx },
                    TaskerReceiver { rx: self.rx },
                    self.handle,
                )
            }

            pub fn spawn<C>(ctx: C) -> Self
            where
                C: Send + Sync + Clone + 'static,
                $( <$task_path as ::oyui_tasker::worker::WorkerTask>::Context: ::oyui_tasker::worker::ExtractsFrom<C>, )*
            {
                let (req_tx, mut req_rx) = ::tokio::sync::mpsc::unbounded_channel::<WorkerRequest>();
                let (ev_tx, ev_rx) = ::tokio::sync::mpsc::unbounded_channel::<WorkerEvent>();

                let handle = ::tokio::spawn(async move {
                    while let Some(request) = req_rx.recv().await {
                        match request {
                            WorkerRequest::Shutdown => break,
                            $(
                                WorkerRequest::$variant(req) => {
                                    let ctx = ctx.clone();
                                    let ev_tx = ev_tx.clone();
                                    ::tokio::spawn(async move {
                                        let task_ctx = <<$task_path as ::oyui_tasker::worker::WorkerTask>::Context as ::oyui_tasker::worker::ExtractsFrom<C>>::extract(&ctx);
                                        let res = <$task_path as ::oyui_tasker::worker::WorkerTask>::handle(req, task_ctx).await;
                                        let _ = ev_tx.send(WorkerEvent::$variant(res));
                                    });
                                }
                            )*
                        }
                    }
                });

                Self {
                    tx: req_tx,
                    rx: ev_rx,
                    handle: Some(handle),
                }
            }
        }
    };
}
