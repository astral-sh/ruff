//! Settings for the `flake-annotations` plugin.

use ruff_macros::CacheKey;

#[derive(Debug, Default, CacheKey)]
#[allow(clippy::struct_excessive_bools)]
pub struct Settings {
    pub mypy_init_return: bool,
    pub suppress_dummy_args: bool,
    pub suppress_none_returning: bool,
    pub allow_star_arg_any: bool,
    pub ignore_fully_untyped: bool,
}
