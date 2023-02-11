//! Settings for the `flake-annotations` plugin.

use ruff_macros::ConfigurationOptions;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, JsonSchema,
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
    /// Whether to suppress `ANN000`-level violations for arguments matching the
    /// "dummy" variable regex (like `_`).
    pub suppress_dummy_args: Option<bool>,
    #[option(
        default = "false",
        value_type = "bool",
        example = "suppress-none-returning = true"
    )]
    /// Whether to suppress `ANN200`-level violations for functions that meet
    /// either of the following criteria:
    ///
    /// * Contain no `return` statement.
    /// * Explicit `return` statement(s) all return `None` (explicitly or
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
    #[option(
        default = "false",
        value_type = "bool",
        example = "ignore-fully-untyped = true"
    )]
    /// Whether to suppress `ANN*` rules for any declaration
    /// that hasn't been typed at all.
    /// This makes it easier to gradually add types to a codebase.
    pub ignore_fully_untyped: Option<bool>,
}

#[derive(Debug, Default, Hash)]
#[allow(clippy::struct_excessive_bools)]
pub struct Settings {
    pub mypy_init_return: bool,
    pub suppress_dummy_args: bool,
    pub suppress_none_returning: bool,
    pub allow_star_arg_any: bool,
    pub ignore_fully_untyped: bool,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            mypy_init_return: options.mypy_init_return.unwrap_or(false),
            suppress_dummy_args: options.suppress_dummy_args.unwrap_or(false),
            suppress_none_returning: options.suppress_none_returning.unwrap_or(false),
            allow_star_arg_any: options.allow_star_arg_any.unwrap_or(false),
            ignore_fully_untyped: options.ignore_fully_untyped.unwrap_or(false),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            mypy_init_return: Some(settings.mypy_init_return),
            suppress_dummy_args: Some(settings.suppress_dummy_args),
            suppress_none_returning: Some(settings.suppress_none_returning),
            allow_star_arg_any: Some(settings.allow_star_arg_any),
            ignore_fully_untyped: Some(settings.ignore_fully_untyped),
        }
    }
}
