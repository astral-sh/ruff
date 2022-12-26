//! Settings for the `flake-annotations` plugin.

use ruff_macros::ConfigurationOptions;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8AnnotationsOptions"
)]
pub struct Options {
    #[option(
        default = "false",
        value_type = "bool",
        example = "mypy-init-return = true"
    )]
    /// Whether to allow the omission of a return type hint for `__init__` if at
    /// least one argument is annotated.
    pub mypy_init_return: Option<bool>,
    #[option(
        default = "false",
        value_type = "bool",
        example = "suppress-dummy-args = true"
    )]
    /// Whether to suppress `ANN000`-level errors for arguments matching the
    /// "dummy" variable regex (like `_`).
    pub suppress_dummy_args: Option<bool>,
    #[option(
        default = "false",
        value_type = "bool",
        example = "suppress-none-returning = true"
    )]
    /// Whether to suppress `ANN200`-level errors for functions that meet either
    /// of the following criteria:
    ///
    /// - Contain no `return` statement.
    /// - Explicit `return` statement(s) all return `None` (explicitly or
    ///   implicitly).
    pub suppress_none_returning: Option<bool>,
    #[option(
        default = "false",
        value_type = "bool",
        example = "allow-star-arg-any = true"
    )]
    /// Whether to suppress `ANN401` for dynamically typed `*args` and
    /// `**kwargs` arguments.
    pub allow_star_arg_any: Option<bool>,
}

#[derive(Debug, Hash, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Settings {
    pub mypy_init_return: bool,
    pub suppress_dummy_args: bool,
    pub suppress_none_returning: bool,
    pub allow_star_arg_any: bool,
}

impl Settings {
    #[allow(clippy::needless_pass_by_value)]
    pub fn from_options(options: Options) -> Self {
        Self {
            mypy_init_return: options.mypy_init_return.unwrap_or_default(),
            suppress_dummy_args: options.suppress_dummy_args.unwrap_or_default(),
            suppress_none_returning: options.suppress_none_returning.unwrap_or_default(),
            allow_star_arg_any: options.allow_star_arg_any.unwrap_or_default(),
        }
    }
}
