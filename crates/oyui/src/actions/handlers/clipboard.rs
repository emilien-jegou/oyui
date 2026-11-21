//! System clipboard access via the OSC 52 terminal escape.

/// Builds the OSC 52 sequence that copies `text` to the clipboard.
///
/// Works over SSH and needs no external program. Terminals may impose a size
/// limit and silently truncate large payloads.
///
/// The sequence is *returned*, never written from here: the event loop owns
/// the only tty writer (the Ratatui backend), and out-of-band bytes pushed to
/// `SessionContext::terminal_writes` are emitted through it between frames.
/// A direct write on `io::stdout()` would interleave with a frame's diff
/// bytes — the confirmed trigger of the stale-highlight bug (root
/// `AGENTS.md`, "Terminal rendering").
pub(crate) fn osc52_sequence(text: &str) -> String {
    let encoded = base64_encode(text.as_bytes());
    format!("\x1b]52;c;{encoded}\x1b\\")
}

/// Minimal standard-alphabet base64 encoder for OSC 52 payloads.
fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((n >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::base64_encode;

    #[test]
    fn base64_matches_reference_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
    }
}
