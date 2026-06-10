use rune::{languageserver, Options};
use std::sync::Arc;

use crate::actions::state::TuiState;
use crate::diff_cache::DiffCache;
use crate::tree::FileTree;
use crate::view::View;
use crate::{commands::CommandError, config::script};
use crate::actions::handlers;

pub async fn run_lsp() -> Result<(), CommandError> {
    tracing::info!("Starting language server...");

    // TODO: to simplify the lsp we should create stub implementation for the context
    // best way to do that is by first modifying define_actions macro to allow omitting values.
    let state = Arc::new(parking_lot::RwLock::new(TuiState::new("weywot")));
    let tree = Arc::new(parking_lot::RwLock::new(FileTree::default()));
    let cache = Arc::new(parking_lot::RwLock::new(DiffCache::default()));
    let view = View::default();

    let handler = handlers::generate(state, tree, cache, view, "".into());
    let context = script::build_context(handler)
        .map_err(|e| CommandError::Runtime(Box::new(e)))?;

    let options = Options::from_default_env().map_err(|e| CommandError::Runtime(Box::new(e)))?;

    languageserver::run(context, options).await.map_err(|e| {
        CommandError::Runtime(
            format!("An unexpected error happened while running the lsp: {e:?}").into(),
        )
    })?;

    Ok(())
}
