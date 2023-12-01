//! Settings for the `flake8-unused-arguments` plugin.

use ruff_macros::CacheKey;

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub ignore_variadic_names: bool,
}
