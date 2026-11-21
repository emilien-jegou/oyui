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
    let snippet: String = written
        .iter()
        .take(160)
        .map(|b| if b.is_ascii_graphic() || *b == b' ' { *b as char } else { '.' })
        .collect();
    let _ = writeln!(
        guard,
        "flush\t{}\t{snippet}\t{}B",
        epoch_millis(),
        written.len()
    );
}

/// io::Write wrapper logging every terminal write through [`flush`].
///
/// Attached to `CrosstermBackend`'s writer, so every flush ratatui performs
/// is timestamped with the exact bytes — the one probe the traces never had:
/// if a frame's bytes are empty, nothing was flushed at all.
pub struct CountingStdout {
    inner: io::Stdout,
}

impl CountingStdout {
    pub fn new() -> Self {
        Self { inner: io::stdout() }
    }
}

impl Write for CountingStdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        flush(buf);
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl Default for CountingStdout {
    fn default() -> Self {
        Self::new()
    }
}

/// Wall-clock milliseconds; the frame log's shared time base.
pub fn epoch_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}
