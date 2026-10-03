//! Rune language server for authoring `.rn` config scripts.

use super::RuneHost;
use crate::actions::BoxedHandler;
use crate::commands::CommandError;
use rune::{languageserver, Options};

/// Runs the config-script language server over stdio.
pub async fn run_lsp() -> Result<(), CommandError> {
    tracing::info!("Starting language server...");
    let context = RuneHost::lsp_context(BoxedHandler::empty())
        .map_err(|e| CommandError::Runtime(Box::new(e)))?;

    let options = Options::from_default_env().map_err(|e| CommandError::Runtime(Box::new(e)))?;

    languageserver::run(context, options).await.map_err(|e| {
        CommandError::Runtime(
            format!("An unexpected error happened while running the lsp: {e:?}").into(),
        )
    })?;

    Ok(())
}
