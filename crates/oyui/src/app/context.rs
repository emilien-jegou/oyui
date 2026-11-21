//! The shared state of one running session.
//!
//! Finding 4.7 / 6.4: the same dynamic state used to be held by three or
//! four structs at once (`Session`, `AppActionsHandler`, `App`, `Config`),
//! each carrying its own `Arc` clones, cloned again per handler variant by
//! `generate()`. There was no single owner, and the locking discipline was
//! spread across all of them.
//!
//! [`SessionContext`] is now the single owner. Consumers hold it through one
//! `Arc` and reach the fields through `Deref`, so the field-access syntax in
//! every handler is unchanged while the ownership collapses to one place.

use crate::app::UiState;
use crate::diff_cache::DiffCache;
use crate::terminal_colors::TerminalColorMode;
use crate::theme::ThemeState;
use crate::tree::FileTree;
use crate::worker::EventRegistry;
use parking_lot::{Mutex, RwLock};
use std::path::PathBuf;
use std::sync::Arc;
use typed_builder::TypedBuilder;

/// Everything a running session shares.
#[derive(TypedBuilder, Clone)]
pub struct SessionContext {
    // --- dynamic shared state: the single owner of each handle ---
    pub ui: Arc<Mutex<UiState>>,
    pub theme: Arc<RwLock<ThemeState>>,
    pub tree: Arc<RwLock<FileTree>>,
    pub cache: DiffCache,
    pub worker: Arc<EventRegistry>,
    /// Shared config-error cell; action failures surface through it.
    pub error: Arc<RwLock<Option<String>>>,
    /// Set once the user confirmed; lets callers distinguish confirm from quit.
    pub confirmed: Arc<std::sync::atomic::AtomicBool>,
    /// Out-of-band escape sequences (e.g. OSC 52 clipboard) waiting for the
    /// event loop. Handlers never write the tty themselves: the Ratatui
    /// backend owns it, and anything that is not a frame is enqueued here
    /// and emitted through the backend between frames, so it can never
    /// interleave with a frame's diff bytes and make the terminal lose a
    /// delta Ratatui believes it sent.
    ///
    /// This is a leaf lock: never hold it while taking another lock.
    #[builder(default_code = "Arc::new(Mutex::new(Vec::new()))")]
    pub terminal_writes: Arc<Mutex<Vec<String>>>,

    // --- session shape: fixed for the lifetime of the session ---
    pub operation: crate::app::Operation,
    pub left_path: PathBuf,
    pub right_path: PathBuf,
    pub base_path: Option<PathBuf>,
    /// Destination for the confirmed result; `None` means read-only.
    pub write_target: Option<PathBuf>,
    /// Whether confirming is allowed with unresolved conflicts.
    pub allow_unresolved: bool,
    pub algorithm: crate::cli::DiffAlgorithm,
    pub color_mode: TerminalColorMode,
    /// Start every discovered change staged (used by `oyui squash`).
    pub default_staged: bool,
}
