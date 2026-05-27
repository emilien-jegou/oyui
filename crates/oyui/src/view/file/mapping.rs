use super::FileViewData;
use crate::diff::FileDiff;

impl FileViewData {
    pub(crate) fn get_line_map(&self, diff: &FileDiff, new_lines_len: usize) -> Vec<usize> {
        let mut line_map = Vec::new();
        let mut current_new = 0;

        for (i, hunk) in diff.hunks.iter().enumerate() {
            let hunk_new_start = hunk.after_lines.start;
            let context_start = hunk_new_start.saturating_sub(self.context_lines);

            if self.is_folded && current_new < context_start {
                line_map.push(current_new);
                current_new = context_start;
            }

            while current_new < hunk_new_start && current_new < new_lines_len {
                line_map.push(current_new);
                current_new += 1;
            }

            for diff_line in &hunk.lines {
                match diff_line {
                    crate::diff::DiffLine::Context { new_line_idx, .. } => {
                        line_map.push(current_new);
                        current_new = *new_line_idx + 1;
                    }
                    crate::diff::DiffLine::Deletion { .. } => {
                        line_map.push(current_new);
                    }
                    crate::diff::DiffLine::Addition { new_line_idx, .. } => {
                        line_map.push(current_new);
                        current_new = *new_line_idx + 1;
                    }
                }
            }

            if self.is_folded {
                let next_hunk_start = diff
                    .hunks
                    .get(i + 1)
                    .map(|h| h.after_lines.start)
                    .unwrap_or(new_lines_len);
                let context_end = current_new
                    .saturating_add(self.context_lines)
                    .min(next_hunk_start);

                while current_new < context_end && current_new < new_lines_len {
                    line_map.push(current_new);
                    current_new += 1;
                }
            }
        }

        if !self.is_folded {
            while current_new < new_lines_len {
                line_map.push(current_new);
                current_new += 1;
            }
        } else if current_new < new_lines_len {
            line_map.push(current_new);
        }

        line_map
    }
}
