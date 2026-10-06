macro_rules! impl_opt_color_getset {
    ($field:ident) => {
        paste::paste! {
            impl [< Theme $field:camel ActionsHandler >] for AppThemeActionsHandler {
                fn get(&self) -> String {
                    if let Some(color) = self.theme.read().ui.$field {
                        color.to_string_val()
                    } else {
                        String::new()
                    }
                }

                fn set(&self, val: String) {
                    if val.is_empty() || val == "none" {
                        self.theme.write().ui.$field = None;
                        self.notify_ui_changed();
                    } else {
                        let parsed = {
                            let theme = self.theme.read();
                            utils::parse_color_val(&val, &theme.ui, &theme.tm_theme, &self.color_mode)
                        };
                        match parsed {
                            Some(c) => {
                                self.theme.write().ui.$field = Some(c);
                                self.notify_ui_changed();
                            }
                            None => self.fail(format!(
                                "invalid color '{val}' for {}",
                                stringify!($field)
                            )),
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
                    let color = self.theme.read().ui.$field;
                    color.to_string_val()
                }

                fn set(&self, val: String) {
                    let parsed = {
                        let theme = self.theme.read();
                        utils::parse_color_val(&val, &theme.ui, &theme.tm_theme, &self.color_mode)
                    };
                    match parsed {
                        Some(c) => {
                            self.theme.write().ui.$field = c;
                            self.notify_ui_changed();
                        }
                        None => self.fail(format!(
                            "invalid color '{val}' for {}",
                            stringify!($field)
                        )),
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
                    self.theme.read().ui.$field.clone()
                }

                fn set(&self, val: $ty) {
                    self.theme.write().ui.$field = val;
                    self.notify_ui_changed();
                }
            }
        }
    };
}

pub(crate) use impl_color_getset;
pub(crate) use impl_opt_color_getset;
pub(crate) use impl_ty_getset;
