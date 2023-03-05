//! Settings for the `flake8-type-checking` plugin.

use ruff_macros::{CacheKey, ConfigurationOptions};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8TypeCheckingOptions"
)]
pub struct Options {
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            strict = true
        "#
    )]
    /// Enforce TC001, TC002, and TC003 rules even when valid runtime imports
    /// are present for the same module.
    /// See flake8-type-checking's [strict](https://github.com/snok/flake8-type-checking#strict) option.
    pub strict: Option<bool>,
    #[option(
        default = "[\"typing\"]",
        value_type = "list[str]",
        example = r#"
            exempt-modules = ["typing", "typing_extensions"]
        "#
    )]
    /// Exempt certain modules from needing to be moved into type-checking
    /// blocks.
    pub exempt_modules: Option<Vec<String>>,
    #[option(
        default = "[\"pydantic.BaseModel\"]",
        value_type = "list[str]",
        example = r#"
            runtime-evaluated-baseclasses = ["pydantic.BaseModel"]
        "#
    )]
    /// Exempt type annotations of certain classes with base classes from needing to be moved into type-checking
    /// blocks.
    pub runtime_evaluated_baseclasses: Option<Vec<String>>,
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            runtime-evaluated-decorators = ["attrs.define", "attrs.frozen"]
        "#
    )]
    /// Exempt type annotations of certain classes with decorators from needing to be moved into type-checking
    /// blocks.
    pub runtime_evaluated_decorators: Option<Vec<String>>,
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub strict: bool,
    pub exempt_modules: Vec<String>,
    pub runtime_evaluated_baseclasses: Vec<String>,
    pub runtime_evaluated_decorators: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            strict: false,
            exempt_modules: vec!["typing".to_string()],
            runtime_evaluated_baseclasses: vec!["pydantic.BaseModel".to_string()],
            runtime_evaluated_decorators: vec![],
        }
    }
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            strict: options.strict.unwrap_or(false),
            exempt_modules: options
                .exempt_modules
                .unwrap_or_else(|| vec!["typing".to_string()]),
            runtime_evaluated_baseclasses: options
                .runtime_evaluated_baseclasses
                .unwrap_or_else(|| vec!["pydantic.BaseModel".to_string()]),
            runtime_evaluated_decorators: options.runtime_evaluated_decorators.unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            strict: Some(settings.strict),
            exempt_modules: Some(settings.exempt_modules),
            runtime_evaluated_baseclasses: Some(settings.runtime_evaluated_baseclasses),
            runtime_evaluated_decorators: Some(settings.runtime_evaluated_decorators),
        }
    }
}
