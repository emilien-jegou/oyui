//! Keyboard input handling and keybind dispatch for the main event loop.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

use crate::app::ui_state::MessageLevel;
use crate::app::{App, CommandMode};

impl App {
    /// Handles one key event; returns true when the app must abort.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        // The help overlay is modal: it swallows input until dismissed.
        if self.ui.lock().help.is_some() {
            self.handle_help_key(key);
            return false;
        }

        // One lock for all UI state; dropped before anything that re-locks it
        // (parking_lot mutexes are not reentrant).
        let mut ui = self.ui.lock();
        let view_before = ui.current;
        if let CommandMode::Active(buf) = &mut ui.command_mode {
            if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat {
                match key.code {
                    KeyCode::Enter => {
                        let cmd = buf.clone();
                        drop(ui);
                        self.execute_command(&cmd);
                        self.ui.lock().command_mode = CommandMode::Normal;
                    }
                    KeyCode::Esc => {
                        ui.command_mode = CommandMode::Normal;
                    }
                    KeyCode::Backspace => {
                        buf.pop();
                    }
                    KeyCode::Char(c) => {
                        buf.push(c);
                    }
                    _ => {}
                }
            }
        } else if matches!(ui.command_mode, CommandMode::ConfirmMerge) {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Enter => {
                        ui.command_mode = CommandMode::Normal;
                        drop(ui);
                        self.handler.dispatch(&crate::actions::Action(
                            crate::actions::Actions::global(
                                crate::actions::GlobalActions::execute_merge,
                            ),
                        ));
                    }
                    KeyCode::Char('q') | KeyCode::Esc => {
                        ui.command_mode = CommandMode::Normal;
                    }
                    _ => {}
                }
            }
        } else {
            drop(ui);
            // Standard keybind handling
            let active_mode = if self.ui.lock().current == crate::view::ViewKind::File {
                crate::actions::keybinds::KeybindMode::View(crate::actions::keybinds::View::File)
            } else {
                crate::actions::keybinds::KeybindMode::View(crate::actions::keybinds::View::Tree)
            };

            let matched_targets: Vec<_> = self
                .config
                .keybinds
                .bindings
                .iter()
                .filter(|(mode, kb, _)| {
                    (*mode == crate::actions::keybinds::KeybindMode::Global || *mode == active_mode)
                        && kb.matches(&key)
                })
                .flat_map(|(_, _, targets)| targets.iter().cloned())
                .collect();

            for target in matched_targets {
                match target {
                    // `q` quits gracefully: the VCS reads a zero exit as
                    // "tool closed, keep the target unchanged".
                    crate::actions::keybinds::ActionTarget::Static(action) => {
                        self.handler.dispatch(&action);
                    }
                    crate::actions::keybinds::ActionTarget::Dynamic(id) => {
                        tracing::debug!("Matched script keybind, executing callback");
                        if let Err(e) = self.config.call_callback(id) {
                            tracing::error!("Script keybind execution error: {}", e);
                            // Surface runtime failures as a transient notification
                            // rather than pinning the compile-error overlay.
                            self.set_message(MessageLevel::Error, e.to_string());
                        }
                    }
                }
            }

            if self.ui.lock().current != view_before {
                if let Err(e) = self.config.call_event("view_changed") {
                    self.set_message(MessageLevel::Error, e.to_string());
                }
            }
        }
        false
    }

    /// Handles keys while the keybinding help overlay is open.
    fn handle_help_key(&self, key: KeyEvent) {
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return;
        }

        let mut ui = self.ui.lock();

        if matches!(
            key.code,
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?')
        ) {
            ui.help = None;
            return;
        }

        let Some(help) = ui.help.as_mut() else {
            return;
        };

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => help.scroll = help.scroll.saturating_add(1),
            KeyCode::Char('k') | KeyCode::Up => help.scroll = help.scroll.saturating_sub(1),
            KeyCode::PageDown | KeyCode::Char('d') => help.scroll = help.scroll.saturating_add(10),
            KeyCode::PageUp | KeyCode::Char('u') => help.scroll = help.scroll.saturating_sub(10),
            KeyCode::Char('g') | KeyCode::Home => help.scroll = 0,
            KeyCode::Char('G') | KeyCode::End => help.scroll = usize::MAX,
            _ => {}
        }
    }
}
