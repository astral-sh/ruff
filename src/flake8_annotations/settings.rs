//! Settings for the `flake-annotations` plugin.

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    /// Allow omission of a return type hint for `__init__` if at least one
    /// argument is annotated.
    pub mypy_init_return: Option<bool>,
    /// Suppress ANN000-level errors for dummy arguments, like `_`.
    pub suppress_dummy_args: Option<bool>,
    /// Suppress ANN200-level errors for functions that meet one of the
    /// following criteria:
    /// - Contain no `return` statement
    /// - Explicit `return` statement(s) all return `None` (explicitly or
    ///   implicitly).
    pub suppress_none_returning: Option<bool>,
    /// Suppress ANN401 for dynamically typed *args and **kwargs.
    pub allow_star_arg_any: Option<bool>,
}

#[derive(Debug, Hash, Default)]
pub struct Settings {
    pub mypy_init_return: bool,
    pub suppress_dummy_args: bool,
    pub suppress_none_returning: bool,
    pub allow_star_arg_any: bool,
}

impl Settings {
    pub fn from_options(options: Options) -> Self {
        Self {
            mypy_init_return: options.mypy_init_return.unwrap_or_default(),
            suppress_dummy_args: options.suppress_dummy_args.unwrap_or_default(),
            suppress_none_returning: options.suppress_none_returning.unwrap_or_default(),
            allow_star_arg_any: options.allow_star_arg_any.unwrap_or_default(),
        }
    }
}
