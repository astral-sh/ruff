use crate::display_settings;
use ruff_macros::CacheKey;
use ruff_python_ast::name::Name;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub functions_names: Vec<Name>,
}

pub fn default_func_names() -> Vec<Name> {
    vec![
        Name::new_static("_"),
        Name::new_static("gettext"),
        Name::new_static("ngettext"),
    ]
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            functions_names: default_func_names(),
        }
    }
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_gettext",
            fields = [
                self.functions_names | array
            ]
        }
        Ok(())
    }
}
