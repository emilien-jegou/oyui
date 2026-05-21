use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::error::Error;

use crate::app::{commands, App, CommandMode, ExitAction};
use crate::view::{ViewAction, ViewKind};
use crate::worker::tasks;

#[tracing::instrument(level = "debug", skip(app))]
pub fn handle_key(app: &mut App, key: KeyEvent) -> Result<ExitAction, Box<dyn Error>> {
    match &app.command_mode {
        CommandMode::Active(_) => Ok(handle_command_input(app, key)),
        CommandMode::ConfirmMerge => {
            match key.code {
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    tracing::debug!("User aborted merge with Ctrl-C");
                    return Ok(ExitAction::QuitWithAbort);
                }
                KeyCode::Enter => {
                    tracing::debug!("User confirmed merge");
                    let _ = app.confirm_merge();
                    return Ok(ExitAction::QuitAndMerge);
                }
                KeyCode::Char('q') | KeyCode::Esc => {
                    tracing::debug!("User cancelled merge confirmation");
                    app.command_mode = CommandMode::Normal;
                }
                KeyCode::Char(':') => {
                    app.command_mode = CommandMode::Active(String::new());
                }
                _ => {}
            };
            Ok(ExitAction::KeepRunning)
        }
        CommandMode::Normal => {
            let action = app.view.handle_input(key, &app.tree, &app.cache);
            Ok(handle_view_action(app, action))
        }
    }
}

#[tracing::instrument(level = "debug", skip(app))]
fn handle_view_action(app: &mut App, action: ViewAction) -> ExitAction {
    match action {
        ViewAction::None => ExitAction::KeepRunning,
        ViewAction::QuitWithAbort => ExitAction::QuitWithAbort,
        ViewAction::OpenCommandMode => {
            app.command_mode = CommandMode::Active(String::new());
            ExitAction::KeepRunning
        }
        ViewAction::ConfirmMerge => {
            app.command_mode = CommandMode::ConfirmMerge;
            ExitAction::KeepRunning
        }
        ViewAction::ToggleStageSelected => {
            if let Some(row) = app.view.tree_view.selected_row(&app.tree, &app.cache) {
                let new_state = row.staging_state.toggle();
                tracing::debug!(path = %row.path.display(), ?new_state, "Toggling stage state");
                commands::set_state_for_path(&mut app.tree, &row.path, new_state);
            }
            ExitAction::KeepRunning
        }
        ViewAction::InvertSelection => {
            tracing::debug!("Inverting all staging selections");
            for node in &mut app.tree.nodes {
                node.invert_state_recursive();
            }
            ExitAction::KeepRunning
        }
        ViewAction::OpenFileView {
            path,
            left_path,
            right_path,
        } => {
            tracing::info!(path = %path.display(), "Opening file view");
            app.view.current = ViewKind::File;
            app.view.file_view.current_path = Some(path.clone());

            if let (Some(left), Some(right)) = (left_path, right_path) {
                if matches!(
                    app.cache.diffs.get(&path),
                    crate::commons::lazy::Lazy::Unstarted
                ) {
                    tracing::debug!(path = %path.display(), "Queueing full diff calculation");
                    app.cache.diffs.mark_started(path.clone());

                    // Sending the newly refactored Worker Request
                    let _ = app.worker.send(tasks::full_diff::FullDiffReq {
                        node_path: path.clone(),
                        left_path: left,
                        right_path: right,
                    });
                }
            }
            ExitAction::KeepRunning
        }
        ViewAction::CloseFileView => {
            tracing::debug!("Closing file view");
            app.view.current = ViewKind::Tree;
            ExitAction::KeepRunning
        }
    }
}

#[tracing::instrument(level = "trace", skip(app))]
fn handle_command_input(app: &mut App, key: KeyEvent) -> ExitAction {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return ExitAction::QuitWithAbort;
        }
        KeyCode::Esc => {
            tracing::trace!("Exited command mode via Esc");
            app.command_mode = CommandMode::Normal;
        }
        KeyCode::Enter => {
            let cmd = match &app.command_mode {
                CommandMode::Active(s) => s.clone(),
                _ => String::new(),
            };
            app.command_mode = CommandMode::Normal;

            tracing::info!(cmd = %cmd, "Executing command");
            app.execute_command(&cmd);
        }
        KeyCode::Backspace => {
            if let CommandMode::Active(ref mut buf) = app.command_mode {
                buf.pop();
            }
        }
        KeyCode::Char(c) => {
            if let CommandMode::Active(ref mut buf) = app.command_mode {
                buf.push(c);
            }
        }
        _ => {}
    }
    ExitAction::KeepRunning
}
