use crate::actions::BoxedHandler;
use crate::{commands::CommandError, config::script};
use rune::{languageserver, Options};

pub async fn run_lsp() -> Result<(), CommandError> {
    tracing::info!("Starting language server...");
    let context = script::build_context(BoxedHandler::empty())
        .map_err(|e| CommandError::Runtime(Box::new(e)))?;

    let options = Options::from_default_env().map_err(|e| CommandError::Runtime(Box::new(e)))?;

    languageserver::run(context, options).await.map_err(|e| {
        CommandError::Runtime(
            format!("An unexpected error happened while running the lsp: {e:?}").into(),
        )
    })?;

    Ok(())
}
