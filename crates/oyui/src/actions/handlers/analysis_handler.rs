//! File-content analysis helpers for scripted bulk operations.
//!
//! These read from disk on the calling (main) thread. Keep usage scoped; a
//! future `task::` namespace will move scanning off-thread.

use crate::actions::handlers::AppActionsHandler;
use crate::actions::*;
use crate::app::ui_state::MessageLevel;

impl AnalysisActionsHandler for AppActionsHandler {
    fn files_containing(&self, pattern: String) -> String {
        let Ok(re) = regex::Regex::new(&pattern) else {
            self.set_message(MessageLevel::Error, format!("invalid regex '{pattern}'"));
            return String::new();
        };
        let tree = self.tree.read();
        tree.files()
            .filter(|f| {
                [&f.left_path, &f.right_path]
                    .into_iter()
                    .flatten()
                    .any(|p| {
                        std::fs::read_to_string(p)
                            .map(|c| re.is_match(&c))
                            .unwrap_or(false)
                    })
            })
            .map(|f| f.path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }
}
