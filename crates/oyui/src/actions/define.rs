use oyui_rune_actions::define_actions;

use crate::config::LineHighlightMode;

define_actions! {
    global {
        quit()
        confirm()
        execute_merge()
        open_command_mode()
        confirm_merge_window_enabled { @getset(bool) }

        // Read-only context about the session.
        left_path(|| -> String)
        right_path(|| -> String)
        base_path(|| -> String)
        view(|| -> String)
        algorithm(|| -> String)
        operation(|| -> String)
        writable(|| -> bool)
        conflict_count(|| -> u32)

        // Switch the active pane and run a command-palette command.
        switch(String)
        command(String)
        clear_error()

        // Transient bottom-bar notifications for scripts.
        notify(String)
        warn(String)
        error(String)
        clear_message()

        // Undo/redo of staging mutations.
        undo()
        redo()

        // Copy text to the system clipboard via OSC 52 (works over SSH).
        copy(String)

        // Toggle the keybinding-help overlay.
        help()
    }
    settings {
        scrolloff { @getset(u32) }
        context_lines { @getset(u32) }
    }
    analysis {
        // Blocking: newline-joined paths whose old/new side matches `pattern`.
        // Prefer `analysis::files_containing_async` for large trees.
        files_containing(|String| -> String)
    }
    ui {
        // Per-view hint bar: `set(view, "key=desc key=desc")`, `clear(view)`.
        hint {
            set(String, String)
            clear(String)
        }
        // Space-separated tree row fields: state, icon, name, stats, path.
        tree_row_format { @getset(String) }
        // Persistent status line shown on the bottom bar.
        status { @getset(String) }
    }
    theme {
        set(String)
        toggle_gradient()
        is_dark(|| -> bool)

        // Name of the active theme, and a newline-separated list of builtins.
        name(|| -> String)
        list(|| -> String)

        // Replace current syntax theme with an embedded
        // theme or via a provided path.
        syntax(String)

        gradient { @getset(bool) }

        bg { @getset(String) }
        fg { @getset(String) }
        cursor_bg { @getset(String) }
        dim { @getset(String) }
        dimmer { @getset(String) }
        staged { @getset(String) }
        unstaged { @getset(String) }
        partial { @getset(String) }
        dir { @getset(String) }
        cmd { @getset(String) }
        add_bg { @getset(String) }
        del_bg { @getset(String) }
        add_fg { @getset(String) }
        del_fg { @getset(String) }
        file_staged_highlight { @getset(LineHighlightMode) }
        file_staged_highlight_opacity { @getset(f64) }
        file_change_highlight { @getset(LineHighlightMode) }
        file_change_highlight_opacity { @getset(f64) }

        // Conflict accent + block underlay. `conflict_bg` empty derives it from the bg.
        conflict_fg { @getset(String) }
        conflict_bg { @getset(String) }
        file_conflict_highlight { @getset(LineHighlightMode) }
        file_conflict_highlight_opacity { @getset(f64) }

        char_hunk_split { @getset(String) }
        char_hunk_split_color { @getset(String) }
        char_line_split { @getset(String) }
        char_line_split_color { @getset(String) }
        char_indicator { @getset(String) }
        char_add_sign { @getset(String) }
        char_del_sign { @getset(String) }
        char_trailing_space_fg { @getset(String) }
        char_tab_fg { @getset(String) }
        char_scroll_fg { @getset(String) }

        char_trailing_space { @getset(String) }
        char_tab { @getset(String) }
        char_scroll_both { @getset(String) }
        char_scroll_left { @getset(String) }
        char_scroll_right { @getset(String) }

        tree_progressive_change_dim { @getset(bool) }
    }
    view {
        file {
            scroll {
                left(u32)
                right(u32)
            }
            cursor {
                up(u32)
                down(u32)
                page_up()
                page_down()
                half_page_up()
                half_page_down()
                top()
                bottom()

                // Read-only cursor context.
                row(|| -> u32)
                row_count(|| -> u32)
                hunk_index(|| -> Option<u32>)
                hunk_count(|| -> u32)
                kind(|| -> String)
                text(|| -> String)
                old_line(|| -> u32)
                new_line(|| -> u32)
                is_staged(|| -> bool)
            }
            nav {
                next_hunk()
                prev_hunk()
            }
            staging {
                toggle()
                toggle_hunk(u32)
                toggle_line()
                split()
                split_at(u32, u32)
                join()
                invert()
                set_hunk(u32, bool)
                stage_all()
                unstage_all()
                state(|| -> String)
            }
            fold {
                toggle()
            }
            path(|| -> String)
            folded(|| -> bool)
            close()

            // Read-only diff/hunk introspection for the open file.
            inspect {
                hunk_has(|| -> bool)
                hunk_index(|| -> Option<u32>)
                hunk_count(|| -> u32)
                hunk_text(|| -> String)
                hunk_marker(|| -> String)
                hunk_is_staged(|| -> bool)
                diff_text(|| -> String)
                new_text(|| -> String)
                old_text(|| -> String)
                staged_text(|| -> String)
                stats(|| -> String)
                is_binary(|| -> bool)
            }
        }
        tree {
            cursor {
                up(u32)
                down(u32)
                page_up()
                page_down()
                half_page_up()
                half_page_down()
                top()
                bottom()
            }
            directory {
                expand()
                collapse()
                expand_all()
                collapse_all()
            }
            staging {
                toggle_selected()
                invert()
                set(String, bool)
                set_matching(String, bool)
                stage_all()
                unstage_all()
            }
            open_selected()
            open_file(String)

            // Read-only selection context.
            selected_index(|| -> u32)
            row_count(|| -> u32)
            selected_path(|| -> String)
            selected_name(|| -> String)
            selected_is_dir(|| -> bool)
            selected_staging(|| -> String)
            is_folded(|String| -> bool)

            // Read-only per-row / per-path introspection.
            inspect {
                files(|| -> String)
                path_at(|u32| -> String)
                is_dir_at(|u32| -> bool)
                staging_at(|u32| -> String)
                file_diff_text(|String| -> String)
                file_stats(|String| -> String)
                file_staging(|String| -> String)
            }
        }
    }
}
