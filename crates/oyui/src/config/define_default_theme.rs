use super::theme::{Color, UiTheme};
use syntect::highlighting::Theme;

macro_rules! define_default_theme {
    (
        $(
            $name:ident: {
                $( list: $list:expr, )?
                $( setting: $setting:ident, )?
                light: $light:expr,
                dark: $dark:expr $(,)?
            }
        ),* $(,)?
    ) => {
        pub trait ThemeColors {
            fn theme(&self) -> &Theme;
            fn is_dark_theme(&self) -> bool;

            fn bg(&self) -> Color {
                syn_to_color(self.theme().settings.background)
                    .unwrap_or(Color::Reset)
            }

            $(
                fn $name(&self) -> Color {
                    let theme = self.theme();
                    let mut extracted: Option<Color> = None;

                    $(
                        if extracted.is_none() {
                            extracted = extract_scope_color(theme, &$list);
                        }
                    )?

                    $(
                        if extracted.is_none() {
                            extracted = syn_to_color(theme.settings.$setting);
                        }
                    )?

                    extracted.unwrap_or_else(|| {
                        if self.is_dark_theme() { $dark } else { $light }
                    })
                }
            )*

            fn derive_ui_theme(&self) -> UiTheme {
                let bg = self.bg();
                let fg = self.fg();
                let staged = self.staged();
                let del_fg = self.del_fg();
                let dim = self.dim();
                let dimmer = self.dimmer();

                let cursor_bg = syn_to_color(self.theme().settings.line_highlight)
                    .unwrap_or_else(|| blend(fg, bg, 1.).unwrap_or(fg));

                UiTheme::builder()
                  .bg(bg.clone())
                  .cursor_bg(cursor_bg)
                  .fg(fg)
                  .dim(dim)
                  .dimmer(dimmer)
                  .staged(staged)
                  .unstaged(dim)
                  .partial(self.partial())
                  .dir(self.dir())
                  .cmd(self.cmd())
                  .add_bg(blend(staged, bg, 1.).unwrap_or(staged))
                  .del_bg(blend(del_fg, bg, 1.).unwrap_or(del_fg))
                  .add_fg(staged)
                  .del_fg(del_fg)
                  .char_trailing_space_fg(dimmer)
                  .char_tab_fg(dimmer)
                  .char_scroll_fg(dimmer)
                  .build()
            }
        }
    };
}

define_default_theme! {
    fg: {
        setting: foreground,
        light: Color::Black,
        dark: Color::White,
    },
    dim: {
        list: ["comment", "punctuation"],
        setting: gutter_foreground,
        light: Color::DarkGray,
        dark: Color::Gray,
    },
    dimmer: {
        setting: gutter_foreground,
        light: Color::Gray,
        dark: Color::DarkGray,
    },
    staged: {
        list: ["markup.inserted", "string", "entity.name.string"],
        light: Color::Green,
        dark: Color::LightGreen,
    },
    partial: {
        list: ["markup.changed", "constant.numeric", "support.type"],
        light: Color::Yellow,
        dark: Color::LightYellow,
    },
    del_fg: {
        list: ["markup.deleted", "invalid", "keyword.operator"],
        light: Color::Red,
        dark: Color::LightRed,
    },
    dir: {
        list: ["entity.name.type", "entity.name.class", "storage"],
        light: Color::Blue,
        dark: Color::LightBlue,
    },
    cmd: {
        list: ["keyword.control", "variable", "entity.name.function"],
        light: Color::Magenta,
        dark: Color::LightMagenta,
    }
}

pub struct ThemeContext<'a> {
    pub theme: &'a Theme,
    pub is_dark: bool,
}

impl<'a> ThemeContext<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        let bg_color = syn_to_color(theme.settings.background).unwrap_or(Color::Reset);

        Self {
            theme,
            is_dark: is_dark(bg_color).unwrap_or(true),
        }
    }
}

impl<'a> ThemeColors for ThemeContext<'a> {
    fn theme(&self) -> &Theme {
        self.theme
    }

    fn is_dark_theme(&self) -> bool {
        self.is_dark
    }
}

pub fn derive_ui_theme(theme: &Theme) -> UiTheme {
    ThemeContext::new(theme).derive_ui_theme()
}

pub fn is_dark(bg: Color) -> Option<bool> {
    let (r, g, b) = bg.try_as_rgb()?;
    let luminance = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    Some(luminance < 128.0)
}

pub fn extract_scope_color(theme: &Theme, target_scopes: &[&str]) -> Option<Color> {
    for &target in target_scopes {
        let matched_color = theme
            .scopes
            .iter()
            .filter(|item| format!("{:?}", item.scope).contains(target))
            .find_map(|item| item.style.foreground);

        if let Some(c) = matched_color {
            return Some(Color::Rgb(c.r, c.g, c.b));
        }
    }
    None
}

fn blend(fg: Color, bg: Color, alpha: f32) -> Option<Color> {
    let (fr, fg_g, fb) = fg.try_as_rgb()?;
    let (br, bg_g, bb) = bg.try_as_rgb()?;

    let blend_channel = |f: u8, b: u8| ((f as f32 * alpha) + (b as f32 * (1.0 - alpha))) as u8;

    Some(Color::Rgb(
        blend_channel(fr, br),
        blend_channel(fg_g, bg_g),
        blend_channel(fb, bb),
    ))
}

fn syn_to_color(opt: Option<syntect::highlighting::Color>) -> Option<Color> {
    opt.map(|c| Color::Rgb(c.r, c.g, c.b))
}
