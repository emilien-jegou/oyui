use oyui_tasker::Listener;
use std::path::PathBuf;
use std::time::SystemTime;

pub struct WatchConfig;

#[derive(Clone, Debug)]
pub struct WatchConfigReq {
    pub path: PathBuf,
    pub last_mtime: Option<SystemTime>,
}

#[derive(Debug, Clone)]
pub struct WatchConfigRes {
    pub path: PathBuf,
    pub last_mtime: Option<SystemTime>,
}

impl Listener<WatchConfigReq, crate::worker::EventSender> for WatchConfig {
    type Context = ();

    #[tracing::instrument(skip_all, fields(path = %event.path.display()))]
    async fn handle(
        event: WatchConfigReq,
        _ctx: Self::Context,
        tx: crate::worker::EventSender,
    ) -> eyre::Result<()> {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
        let mut last_mtime = event.last_mtime;

        let debounce_duration = std::time::Duration::from_millis(250);
        let mut debounce_deadline = None;

        if last_mtime.is_none() {
            let mtime = std::fs::metadata(&event.path)
                .and_then(|meta| meta.modified())
                .ok();
            last_mtime = mtime;
            tx.send(WatchConfigRes {
                path: event.path.clone(),
                last_mtime: mtime,
            })?;
        }

        loop {
            interval.tick().await;

            if let Ok(meta) = std::fs::metadata(&event.path) {
                if let Ok(mtime) = meta.modified() {
                    if Some(mtime) != last_mtime {
                        debounce_deadline = Some(tokio::time::Instant::now() + debounce_duration);
                        last_mtime = Some(mtime);
                    }
                }
            }

            if let Some(deadline) = debounce_deadline {
                if tokio::time::Instant::now() >= deadline {
                    debounce_deadline = None;
                    tracing::debug!(
                        ?last_mtime,
                        "Config file change settled, notifying main thread"
                    );

                    tx.send(WatchConfigRes {
                        path: event.path.clone(),
                        last_mtime,
                    })?;
                }
            }
        }
    }
}
