//! TEMPORARY probe — validates the SECTION_STATE_BUDGET fix on the real
//! rows.rs pair (the file whose syntax-aware diff used to walk ~1M states
//! for ~10s). DELETE after the confirmation.

use std::ops::Range;
use std::path::Path;
use std::time::{Duration, Instant};

use oyui_syndiff::{diff_line_ops, parse_into_trees};

fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

fn line_ranges(content: &str) -> Vec<Range<usize>> {
    let bytes = content.as_bytes();
    let mut ranges = Vec::with_capacity(bytes.len() / 24 + 1);
    let mut start = 0;
    for (i, b) in bytes.iter().enumerate() {
        if *b != b'\n' {
            continue;
        }
        let mut end = i;
        if end > start && bytes[end - 1] == b'\r' {
            end -= 1;
        }
        ranges.push(start..end);
        start = i + 1;
    }
    if start < bytes.len() {
        ranges.push(start..bytes.len());
    }
    ranges
}

#[test]
fn probe_mode2_with_budget() {
    let path = Path::new("rows.rs");
    let left = leak(std::fs::read_to_string("/tmp/opencode/rows-old.rs").expect("old"));
    let right = leak(std::fs::read_to_string("/tmp/opencode/rows-new.rs").expect("new"));

    let start = Instant::now();
    let (lt, rt) = parse_into_trees(left, right, path, None).expect("parses");
    println!("parse: {:?}", start.elapsed());

    let t0 = Instant::now();
    let res = diff_line_ops(&lt, left, &rt, right, None);
    let elapsed = t0.elapsed();
    println!(
        "structural line ops: {} in {:?}",
        if res.is_some() { "Some" } else { "None (fallback)" },
        elapsed
    );
    assert!(
        elapsed < Duration::from_secs(3),
        "the section budget must bound the walk; took {elapsed:?}"
    );

    if let Some(diff) = res {
        println!("ops: {} (structural answer survived)", diff.ops.len());
    } else {
        println!("caller falls back to the line path");
    }
}
