use crate::tree::StagingState;
use std::error::Error;
use std::fs;
use std::path::Path;

pub fn perform_merge(
    base: &Path,
    left: &Path,
    right: &Path,
    output: &Path,
    state: StagingState, // New: Tell the merge function what the user chose
) -> Result<(), Box<dyn Error>> {
    match state {
        // User staged the change (take local/left)
        StagingState::Staged => {
            fs::copy(left, output)?;
        }
        // User did not stage (take remote/right)
        StagingState::Unstaged => {
            fs::copy(right, output)?;
        }
        // Partially staged: Perform the 3-way merge
        StagingState::PartiallyStaged => {
            let b = fs::read_to_string(base)?;
            let l = fs::read_to_string(left)?;
            let r = fs::read_to_string(right)?;

            let merge = diffy::MergeOptions::new()
                .set_conflict_style(diffy::ConflictStyle::Diff3)
                .merge(&b, &l, &r);

            let content = match merge {
                Ok(clean) => clean,
                Err(conflicted) => conflicted,
            };
            fs::write(output, content)?;
        }
    }
    Ok(())
}
