//! Neutral id newtypes shared by peer modules.
//!
//! [`CallbackId`] lives here rather than in `script` so the keybind registry
//! can name it without the low-level input layer knowing about the script
//! engine: both sides now depend on `commons`, and neither on the other.

/// Opaque handle to a callback the script registered through `keybind`.
///
/// Minted by the script host; the keybind registry only stores and compares
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CallbackId(pub(crate) u64);
