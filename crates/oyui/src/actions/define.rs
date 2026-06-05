use oyui_rune_actions::define_actions;

use crate::config::LineHighlightMode;

define_actions! {
    global {
        quit(),
        confirm(),
        open_command_mode(),
    },
    theme {
        @getset(String)
        toggle_gradient(),

        bg { @getset(String) }
        fg { @getset(String) }
        cursor_bg { @getset(String) }
        dim { @getset(String) }
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

        char_hunk_split { @getset(String) }
        char_hunk_split_color { @getset(String) }
        char_indicator { @getset(String) }
        char_add_sign { @getset(String) }
        char_del_sign { @getset(String) }
    },
    view {
        file {
            scroll {
                left(u32),
                right(u32),
            },
            cursor {
                up(u32),
                down(u32),
                page_up(),
                page_down(),
                half_page_up(),
                half_page_down(),
                top(),
                bottom(),
            },
            nav {
                next_hunk(),
                prev_hunk(),
            },
            staging {
                toggle(),
                toggle_hunk(u32),
                split(),
                invert(),
            },
            fold {
                toggle(),
            },
            close(),
        },
        tree {
            cursor {
                up(u32),
                down(u32),
                page_up(),
                page_down(),
                half_page_up(),
                half_page_down(),
                top(),
                bottom(),
            },
            directory {
                expand(),
                collapse(),
            },
            staging {
                toggle_selected(),
                invert(),
            },
            open_selected(),
            open_file(String),
        }
    }
}
