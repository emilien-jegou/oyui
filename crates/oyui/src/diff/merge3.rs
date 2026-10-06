//! Line-based three-way merge producing a [`ConflictedFile`].
//!
//! Non-overlapping edits from both sides are combined; overlapping edits that
//! differ become conflicts with `ours`/`base`/`theirs` sections.

use super::conflict::{Conflict, ConflictedFile, Segment};
use crate::cli::DiffAlgorithm;
use imara_diff::{Algorithm, Diff, InternedInput};

/// A replacement of `base[start..end]` with `lines`.
#[derive(Debug, Clone)]
struct Change {
    start: usize,
    end: usize,
    lines: Vec<String>,
}

/// Merges `left` and `right` relative to `base`.
pub fn merge3(base: &str, left: &str, right: &str, algo: DiffAlgorithm) -> ConflictedFile {
    let base_lines: Vec<&str> = base.lines().collect();
    let changes_left = changes(base, left, algo);
    let changes_right = changes(base, right, algo);

    let groups = group_changes(&changes_left, &changes_right);

    let mut segments = Vec::new();
    let mut pos = 0;

    for group in groups {
        let start = group.start;
        let end = group.end;

        if start > pos {
            segments.push(Segment::Common(
                base_lines[pos..start]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            ));
        }

        let left_in_group: Vec<&Change> = group
            .left_indices
            .iter()
            .map(|&i| &changes_left[i])
            .collect();
        let right_in_group: Vec<&Change> = group
            .right_indices
            .iter()
            .map(|&i| &changes_right[i])
            .collect();

        let ours = apply(&base_lines, start, end, &left_in_group);
        let theirs = apply(&base_lines, start, end, &right_in_group);

        match (left_in_group.is_empty(), right_in_group.is_empty()) {
            (false, true) => segments.push(Segment::Common(ours)),
            (true, false) => segments.push(Segment::Common(theirs)),
            (false, false) => {
                if ours == theirs {
                    segments.push(Segment::Common(ours));
                } else {
                    segments.push(Segment::Conflict(Conflict {
                        ours,
                        base: Some(
                            base_lines[start..end]
                                .iter()
                                .map(|s| s.to_string())
                                .collect(),
                        ),
                        theirs,
                        jj_raw: None,
                    }));
                }
            }
            (true, true) => {}
        }

        pos = end;
    }

    if pos < base_lines.len() {
        segments.push(Segment::Common(
            base_lines[pos..].iter().map(|s| s.to_string()).collect(),
        ));
    }

    ConflictedFile { segments }
}

/// Computes the changed base ranges when going from `base` to `side`.
fn changes(base: &str, side: &str, algo: DiffAlgorithm) -> Vec<Change> {
    let input = InternedInput::new(base, side);
    let diff = Diff::compute(algorithm(algo), &input);
    let side_lines: Vec<&str> = side.lines().collect();

    diff.hunks()
        .map(|h| Change {
            start: h.before.start as usize,
            end: h.before.end as usize,
            lines: side_lines[h.after.start as usize..h.after.end as usize]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        })
        .collect()
}

fn algorithm(algo: DiffAlgorithm) -> Algorithm {
    match algo {
        DiffAlgorithm::Histogram | DiffAlgorithm::SyntaxAware => Algorithm::Histogram,
        DiffAlgorithm::Myers => Algorithm::Myers,
        DiffAlgorithm::MyersMinimal => Algorithm::MyersMinimal,
    }
}

/// Applies `changes` to `base[region]`, filling the gaps with base lines.
fn apply(
    base: &[&str],
    region_start: usize,
    region_end: usize,
    changes: &[&Change],
) -> Vec<String> {
    let mut out = Vec::new();
    let mut pos = region_start;
    let mut ordered: Vec<&Change> = changes.to_vec();
    ordered.sort_by_key(|c| c.start);

    for change in ordered {
        if change.start > pos {
            out.extend(base[pos..change.start].iter().map(|s| s.to_string()));
        }
        out.extend(change.lines.iter().cloned());
        pos = change.end.max(pos);
    }
    if pos < region_end {
        out.extend(base[pos..region_end].iter().map(|s| s.to_string()));
    }
    out
}

/// A merged region plus the side-local indices of the changes forming it.
struct Group {
    start: usize,
    end: usize,
    left_indices: Vec<usize>,
    right_indices: Vec<usize>,
}

/// Groups changes from both sides into transitive-overlap regions.
fn group_changes(left: &[Change], right: &[Change]) -> Vec<Group> {
    struct Entry<'a> {
        change: &'a Change,
        is_left: bool,
        index: usize,
    }

    let mut entries: Vec<Entry> = Vec::new();
    for (i, c) in left.iter().enumerate() {
        entries.push(Entry {
            change: c,
            is_left: true,
            index: i,
        });
    }
    for (i, c) in right.iter().enumerate() {
        entries.push(Entry {
            change: c,
            is_left: false,
            index: i,
        });
    }
    entries.sort_by_key(|e| (e.change.start, e.change.end));

    let mut groups: Vec<Group> = Vec::new();
    for entry in entries {
        let (s, e) = (entry.change.start, entry.change.end);
        if let Some(last) = groups.last_mut() {
            if overlaps_region(last.start, last.end, s, e) {
                last.start = last.start.min(s);
                last.end = last.end.max(e);
                if entry.is_left {
                    last.left_indices.push(entry.index);
                } else {
                    last.right_indices.push(entry.index);
                }
                continue;
            }
        }
        let mut group = Group {
            start: s,
            end: e,
            left_indices: Vec::new(),
            right_indices: Vec::new(),
        };
        if entry.is_left {
            group.left_indices.push(entry.index);
        } else {
            group.right_indices.push(entry.index);
        }
        groups.push(group);
    }
    groups
}

/// True when `[s2, e2)` touches the group region `[s1, e1)`, treating
/// zero-length insertions as points that only conflict in the interior.
fn overlaps_region(s1: usize, e1: usize, s2: usize, e2: usize) -> bool {
    let group_insertion = s1 == e1;
    let change_insertion = s2 == e2;

    match (group_insertion, change_insertion) {
        (true, true) => s1 == s2,
        (true, false) => s1 > s2 && s1 < e2,
        (false, true) => s2 > s1 && s2 < e1,
        (false, false) => s1 < e2 && s2 < e1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merged(base: &str, left: &str, right: &str) -> ConflictedFile {
        merge3(base, left, right, DiffAlgorithm::Histogram)
    }

    #[test]
    fn non_overlapping_changes_combine() {
        let base = "a\nb\nc\nd\n";
        let left = "A\nb\nc\nd\n";
        let right = "a\nb\nc\nD\n";
        let out = merged(base, left, right);
        assert_eq!(out.conflict_count(), 0);
        let text = out.resolve_all(super::super::conflict::Side::Ours);
        assert!(text.contains('A') && text.contains('D'));
        assert!(text.contains('b') && text.contains('c'));
    }

    #[test]
    fn overlapping_different_changes_conflict() {
        let base = "a\nb\nc\n";
        let left = "a\nLEFT\nc\n";
        let right = "a\nRIGHT\nc\n";
        let out = merged(base, left, right);
        assert_eq!(out.conflict_count(), 1);

        let conflict = out
            .segments
            .iter()
            .find_map(|s| match s {
                Segment::Conflict(c) => Some(c),
                _ => None,
            })
            .unwrap();
        assert_eq!(conflict.ours, vec!["LEFT"]);
        assert_eq!(conflict.base.as_deref(), Some(&["b".to_string()][..]));
        assert_eq!(conflict.theirs, vec!["RIGHT"]);
    }

    #[test]
    fn identical_changes_do_not_conflict() {
        let base = "a\nb\nc\n";
        let left = "a\nSAME\nc\n";
        let right = "a\nSAME\nc\n";
        let out = merged(base, left, right);
        assert_eq!(out.conflict_count(), 0);
    }

    #[test]
    fn single_sided_changes_apply_cleanly() {
        let base = "a\nb\nc\n";
        let left = "a\nb\nc\nleft-extra\n";
        let right = "a\nb\nc\n";
        let out = merged(base, left, right);
        assert_eq!(out.conflict_count(), 0);
        assert!(out
            .resolve_all(super::super::conflict::Side::Ours)
            .contains("left-extra"));
    }

    #[test]
    fn competing_insertions_conflict() {
        let base = "a\nb\n";
        let left = "a\nL\nb\n";
        let right = "a\nR\nb\n";
        let out = merged(base, left, right);
        assert_eq!(out.conflict_count(), 1);
    }
}
