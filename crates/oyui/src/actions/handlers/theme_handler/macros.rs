macro_rules! impl_opt_color_getset {
    ($field:ident) => {
        paste::paste! {
            impl [< Theme $field:camel ActionsHandler >] for AppThemeActionsHandler {
                fn get(&self) -> String {
                    if let Some(color) = self.state.theme.read().ui.$field {
                        color.to_string_val()
                    } else {
                        String::new()
                    }
                }

                fn set(&self, val: String) {
                    if val.is_empty() || val == "none" {
                        self.state.theme.write().ui.$field = None;
                    } else {
                        let parsed = {
                            let theme = self.state.theme.read();
                            utils::parse_color_val(&val, &theme.ui, &theme.tm_theme, &self.color_mode)
                        };
                        if let Some(c) = parsed {
                            self.state.theme.write().ui.$field = Some(c);
                        }
                    }
                }
            }
        }
    };
}

macro_rules! impl_color_getset {
    ($field:ident) => {
        paste::paste! {
            impl [< Theme $field:camel ActionsHandler >] for AppThemeActionsHandler {
                fn get(&self) -> String {
                    let color = self.state.theme.read().ui.$field;
                    color.to_string_val()
                }

                fn set(&self, val: String) {
                    let parsed = {
                        let theme = self.state.theme.read();
                        utils::parse_color_val(&val, &theme.ui, &theme.tm_theme, &self.color_mode)
                    };
                    if let Some(c) = parsed {
                        self.state.theme.write().ui.$field = c;
                    }
                }
            }
        }
    };
}

macro_rules! impl_ty_getset {
    ($field:ident, $ty:ty) => {
        paste::paste! {
            impl [< Theme $field:camel ActionsHandler >] for AppThemeActionsHandler {
                fn get(&self) -> $ty {
                    self.state.theme.read().ui.$field.clone()
                }

                fn set(&self, val: $ty) {
                    self.state.theme.write().ui.$field = val;
                }
            }
        }
    };
}

pub(crate) use impl_opt_color_getset;
pub(crate) use impl_color_getset;
pub(crate) use impl_ty_getset;
