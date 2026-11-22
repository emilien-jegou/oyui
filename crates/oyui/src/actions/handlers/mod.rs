use crate::actions::*;
use crate::app::SessionContext;
use std::sync::Arc;
use typed_builder::TypedBuilder;

pub mod analysis_handler;
pub mod clipboard;
pub mod global_handler;
pub mod history;
pub mod settings_handler;
pub mod theme_handler;
pub mod ui_handler;
pub mod view_handlers;

/// Staging-action handle over the shared session state.
///
/// Finding 6.4: this used to duplicate every shared `Arc` (`ui`, `tree`,
/// `theme`, …) and `generate()` cloned those sixteen fields into seven
/// handler variants — many owners, no single owner. Now the struct carries
/// one `Arc` and derefs to the session context, so every handler method
/// keeps its `self.ui`-style access while the ownership stays in one place.
#[derive(Clone, TypedBuilder)]
pub struct AppActionsHandler {
    pub ctx: Arc<SessionContext>,
}

impl std::ops::Deref for AppActionsHandler {
    type Target = SessionContext;

    fn deref(&self) -> &SessionContext {
        &self.ctx
    }
}

pub fn generate(actions_handler: AppActionsHandler) -> BoxedHandler {
    let theme_handler = theme_handler::AppThemeActionsHandler {
        ctx: actions_handler.ctx.clone(),
    };

    build_handler! {
        global: actions_handler.clone(),
        session: actions_handler.clone(),
        commands: actions_handler.clone(),
        theme: theme_handler.clone(),
        settings: actions_handler.clone(),
        analysis: actions_handler.clone(),
        ui: actions_handler.clone(),
        view {
            tree: actions_handler.clone(),
            file: actions_handler.clone(),
        }
    }
    .build()
}
