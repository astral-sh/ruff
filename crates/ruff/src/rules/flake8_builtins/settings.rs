//! Settings for the `flake8-builtins` plugin.

use ruff_macros::CacheKey;

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub builtins_ignorelist: Vec<String>,
}
