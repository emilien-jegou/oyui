use std::{
    io::{self, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use tracing_error::ErrorLayer;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};
use typed_builder::TypedBuilder;

/// A custom writer that can be suspended atomically.
#[derive(Clone)]
pub struct SuspendableWriter {
    suspended: Arc<AtomicBool>,
}

impl Write for SuspendableWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.suspended.load(Ordering::Relaxed) {
            Ok(buf.len())
        } else {
            io::stderr().write(buf)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.suspended.load(Ordering::Relaxed) {
            Ok(())
        } else {
            io::stderr().flush()
        }
    }
}

// Required so tracing can spawn clones of our writer for different threads/spans
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SuspendableWriter {
    type Writer = Self;

    fn make_writer(&self) -> Self::Writer {
        self.clone()
    }
}

#[derive(TypedBuilder)]
pub struct Tracer {
    #[builder(default = false)]
    pub log_enable: bool,

    #[builder(default = None)]
    pub log_save_path: Option<PathBuf>,

    #[builder(default = false)]
    pub log_console: bool,
}

pub struct TracerGuards {
    pub file_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
    pub console_suspended: Arc<AtomicBool>,
}

impl Tracer {
    pub fn setup(self) -> eyre::Result<TracerGuards> {
        let get_env_filter =
            || EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

        let console_suspended = Arc::new(AtomicBool::new(false));

        // 1. Conditionally create the console layer using our SuspendableWriter
        let console_layer = if self.log_console {
            let writer = SuspendableWriter {
                suspended: Arc::clone(&console_suspended),
            };
            Some(
                fmt::layer()
                    .with_writer(writer)
                    .with_target(true)
                    .without_time()
                    .with_filter(get_env_filter()),
            )
        } else {
            None
        };

        let mut file_layer = None;
        let mut file_guard = None;

        if self.log_enable {
            let path = self.log_save_path.unwrap_or_else(|| "/tmp/oyui.log".into());

            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;

            let (non_blocking, guard) = tracing_appender::non_blocking(file);
            file_guard = Some(guard);

            file_layer = Some(
                fmt::layer()
                    .with_writer(non_blocking)
                    .with_ansi(false)
                    .with_target(true)
                    .with_filter(get_env_filter()),
            );
        }

        let registry = tracing_subscriber::registry()
            .with(console_layer)
            .with(file_layer)
            .with(ErrorLayer::default());

        registry.try_init()?;

        Ok(TracerGuards {
            file_guard,
            console_suspended,
        })
    }
}
