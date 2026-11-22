//! Frame-by-frame tracing: app-side iteration telemetry and the exact bytes
//! each terminal flush writes. Process-wide, single-writer (the UI thread);
//! the appender ignores failures so tracing can never affect the session.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

type FrameFile = Option<Mutex<(File, PathBuf)>>;

static FRAME_LOG: OnceLock<(FrameFile, PathBuf)> = OnceLock::new();

/// Arms the frame log; a file sibling to `log_path`, suffixed `.frames`,
/// receives per-iteration and per-flush lines. Called once from the
/// terminal bootstrap.
pub fn arm(log_path: PathBuf) {
    let frames = log_path.file_name().map(|n| {
        let mut name = n.to_string_lossy().into_owned();
        name.push_str(".frames");
        log_path.with_file_name(name)
    });
    if let Some(frames) = frames {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&frames)
            .ok()
            .map(|f| Mutex::new((f, frames.clone())));
        let _ = FRAME_LOG.set((file, frames));
    }
}

/// Appends one structured frame line: the instant plus `key=value` pairs,
/// already linearized by the caller. One lock per line; failures never
/// propagate.
pub fn frame(fields: &[(&str, String)]) {
    let Some(file) = FRAME_LOG.get().and_then(|(f, _)| f.as_ref()) else {
        return;
    };
    let Ok(mut guard) = file.lock() else {
        return;
    };
    let (guard, _) = &mut *guard;
    let _ = writeln!(guard, "frame\t{}", epoch_millis());
    for (key, value) in fields {
        let _ = write!(guard, "\t{key}={value}");
    }
    let _ = writeln!(guard);
}

/// Appends one terminal-flush line: the instant, bytes written, and a
/// bounded snippet. The write's head is the diff ratatui flushed.
pub fn flush(written: &[u8]) {
    let Some(file) = FRAME_LOG.get().and_then(|(f, _)| f.as_ref()) else {
        return;
    };
    let Ok(mut guard) = file.lock() else {
        return;
    };
    let (guard, _) = &mut *guard;
    let escaped = written.escape_ascii().to_string();
    let _ = writeln!(
        guard,
        "flush\t{}\t{escaped}\t{}B",
        epoch_millis(),
        written.len()
    );
}

/// io::Write wrapper logging every terminal write through [`flush`].
///
/// Attached to `CrosstermBackend`'s writer, so every flush ratatui performs
/// is timestamped with the exact bytes. The wrapped handle is either stdout
/// (when stdout is a terminal) or the controlling terminal.
pub struct CountingWriter {
    inner: Box<dyn Write>,
    pending: Vec<u8>,
}

impl CountingWriter {
    pub fn new(inner: Box<dyn Write>) -> Self {
        Self {
            inner,
            pending: Vec::new(),
        }
    }
}

impl Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.pending.extend_from_slice(&buf[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()?;
        flush(&self.pending);
        self.pending.clear();
        Ok(())
    }
}

/// Wall-clock milliseconds; the frame log's shared time base.
pub fn epoch_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}
