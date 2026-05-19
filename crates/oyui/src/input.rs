use std::error::Error;

use crate::app::{App, CommandMode, ExitAction, ViewMode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Returns true if the app should quit
pub fn handle_key(app: &mut App, key: KeyEvent) -> Result<ExitAction, Box<dyn Error>> {
    match &app.command_mode {
        CommandMode::Active(_) => Ok(handle_command_input(app, key)),
        CommandMode::Normal => Ok(handle_normal_input(app, key)),
        CommandMode::ConfirmMerge => {
            match key.code {
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(ExitAction::QuitWithAbort)
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    let exit_action = app.confirm_and_write_merge()?;
                    if exit_action != ExitAction::KeepRunning {
                        return Ok(exit_action);
                    }
                    return Ok(ExitAction::QuitAndMerge);
                }
                KeyCode::Enter => {
                    let _ = app.confirm_and_write_merge();
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
    }
}

fn handle_normal_input(app: &mut App, key: KeyEvent) -> ExitAction {
    match key.code {
        // Quit
        KeyCode::Char('q') => return ExitAction::QuitWithAbort,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return ExitAction::QuitWithAbort;
        }

        // Navigation
        KeyCode::Char('j') | KeyCode::Down => app.move_down(),
        KeyCode::Char('k') | KeyCode::Up => app.move_up(),

        // Expand/collapse directory
        KeyCode::Char('l') | KeyCode::Right => {
            if let Some(row) = app.selected_row() {
                if row.is_dir {
                    app.set_folded(false);
                } else {
                    app.open_file_view();
                }
            }
        }
        KeyCode::Char('h') | KeyCode::Left => {
            if let Some(row) = app.selected_row() {
                if row.is_dir {
                    app.set_folded(true);
                }
            }
        }

        KeyCode::Enter => {
            app.command_mode = CommandMode::ConfirmMerge;
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.command_mode = CommandMode::ConfirmMerge;
        }

        KeyCode::Esc => app.command_mode = CommandMode::Normal,

        // Stage / unstage
        KeyCode::Char(' ') => app.toggle_stage_selected(),

        // Enter command mode
        KeyCode::Char(':') => {
            app.command_mode = CommandMode::Active(String::new());
        }

        _ => {}
    }
    ExitAction::KeepRunning
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

/// Handle ESC in file view — return to tree
pub fn handle_file_view_key(app: &mut App, key: KeyEvent) -> ExitAction {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return ExitAction::QuitWithAbort;
        }
        KeyCode::Char('q') => return ExitAction::QuitWithAbort,
        KeyCode::Esc | KeyCode::Char('h') => app.view_mode = ViewMode::Tree,
        KeyCode::Char('j') | KeyCode::Down => {
            let i = app.file_scroll_state.selected().unwrap_or(0);
            app.file_scroll_state.select(Some(i + 1));
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let i = app.file_scroll_state.selected().unwrap_or(0);
            if i > 0 {
                app.file_scroll_state.select(Some(i - 1));
            }
        }
        _ => {}
    }
    ExitAction::KeepRunning
}
