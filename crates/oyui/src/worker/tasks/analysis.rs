//! Off-thread regex scan over file contents, answered to its caller.
//!
//! `Reply` in the doc link below is what the caller holds; see
//! `oyui-tasker`'s `ask`/`reply` pairing.
use crate::tree::FileTree;
use oyui_tasker::{Listener, TaskerContext};
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;

/// Request to regex-scan the working tree off the UI thread.
///
/// Carries no correlation id: whoever asked holds the reply the registry
/// handed them, so the answer is routed back to that caller and nobody else.
#[derive(Clone)]
pub struct AnalysisReq {
    pub pattern: String,
}

/// Result of an [`AnalysisReq`], delivered only to its caller.
#[derive(Clone)]
pub struct AnalysisRes {
    pub matches: Vec<String>,
    /// Set when the pattern was invalid or the scan could not run.
    pub error: Option<String>,
}

/// Listener that regex-scans the tree's file contents off the UI thread.
pub struct Analysis;

/// Worker context slice needed to enumerate the tree's files.
#[derive(TaskerContext)]
pub struct AnalysisCtx {
    pub tree: Arc<RwLock<FileTree>>,
}

impl Listener<AnalysisReq> for Analysis {
    type Sender = crate::worker::EventSender;
    type Context = AnalysisCtx;

    #[tracing::instrument(skip_all, fields(pattern = %event.pattern))]
    async fn handle(
        event: AnalysisReq,
        ctx: Self::Context,
        tx: crate::worker::EventSender,
    ) -> eyre::Result<()> {
        let files: Vec<(PathBuf, Option<PathBuf>, Option<PathBuf>)> = {
            let tree = ctx.tree.read();
            tree.files()
                .map(|f| (f.path.clone(), f.left_path.clone(), f.right_path.clone()))
                .collect()
        };

        let pattern = event.pattern;
        let (matches, error) =
            tokio::task::spawn_blocking(move || match regex::Regex::new(&pattern) {
                Ok(re) => (matching_files(&files, &re), None),
                Err(e) => (Vec::new(), Some(format!("invalid regex '{pattern}': {e}"))),
            })
            .await
            .map_err(|join_err| eyre::eyre!("analysis task panicked: {join_err}"))?;

        // The answer goes to whoever asked, so an empty or failed scan never
        // reaches a consumer that was not waiting for it.
        tx.reply(AnalysisRes { matches, error })?;
        Ok(())
    }
}

/// Returns the node paths whose old or new side matches `re`.
pub(crate) fn matching_files(
    files: &[(PathBuf, Option<PathBuf>, Option<PathBuf>)],
    re: &regex::Regex,
) -> Vec<String> {
    files
        .iter()
        .filter(|(_, left, right)| {
            [left, right].into_iter().flatten().any(|p| {
                std::fs::read_to_string(p)
                    .map(|c| re.is_match(&c))
                    .unwrap_or(false)
            })
        })
        .map(|(path, _, _)| path.display().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn matching_files_reads_both_sides() {
        let dir = std::env::temp_dir().join(format!("oyui_analysis_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let left = dir.join("left.txt");
        let right = dir.join("right.txt");
        fs::write(&left, "nothing here").unwrap();
        fs::write(&right, "has needle").unwrap();

        let files = vec![(
            PathBuf::from("node.txt"),
            Some(left.clone()),
            Some(right.clone()),
        )];
        let re = regex::Regex::new("needle").unwrap();

        assert_eq!(matching_files(&files, &re), vec!["node.txt".to_string()]);

        let _ = fs::remove_dir_all(&dir);
    }
}
