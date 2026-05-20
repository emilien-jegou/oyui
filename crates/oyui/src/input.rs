use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::error::Error;

use crate::app::{commands, App, CommandMode, ExitAction};
use crate::view::{ViewAction, ViewKind};
use core_lib::worker::WorkerRequest;

pub fn handle_key(app: &mut App, key: KeyEvent) -> Result<ExitAction, Box<dyn Error>> {
    match &app.command_mode {
        CommandMode::Active(_) => Ok(handle_command_input(app, key)),
        CommandMode::ConfirmMerge => {
            match key.code {
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(ExitAction::QuitWithAbort)
                }
                KeyCode::Enter => {
                    let _ = app.confirm_merge();
                    return Ok(ExitAction::QuitAndMerge);
                }
                KeyCode::Char('q') | KeyCode::Esc => {
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
                commands::set_state_for_path(&mut app.tree, &row.path, new_state);
            }
            ExitAction::KeepRunning
        }
        ViewAction::InvertSelection => {
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
            app.view.current = ViewKind::File;
            app.view.file_view.current_path = Some(path.clone());

            if let (Some(left), Some(right)) = (left_path, right_path) {
                if matches!(app.cache.diffs.get(&path), core_lib::lazy::Lazy::Unstarted) {
                    app.cache.diffs.mark_started(path.clone());
                    let _ = app.worker_tx.send(WorkerRequest::ComputeFullDiff {
                        node_path: path.clone(),
                        left_path: left,
                        right_path: right,
                    });
                }
            }
            ExitAction::KeepRunning
        }
        ViewAction::CloseFileView => {
            app.view.current = ViewKind::Tree;
            ExitAction::KeepRunning
        }
    }
}

fn handle_command_input(app: &mut App, key: KeyEvent) -> ExitAction {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return ExitAction::QuitWithAbort;
        }
        KeyCode::Esc => {
            app.command_mode = CommandMode::Normal;
        }
        KeyCode::Enter => {
            let cmd = match &app.command_mode {
                CommandMode::Active(s) => s.clone(),
                _ => String::new(),
            };
            app.command_mode = CommandMode::Normal;
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
