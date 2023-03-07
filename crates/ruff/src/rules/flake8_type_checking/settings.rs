//! Settings for the `flake8-type-checking` plugin.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, ConfigurationOptions};

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
        default = "[]",
        value_type = "list[str]",
        example = r#"
            runtime-evaluated-base-classes = ["pydantic.BaseModel"]
        "#
    )]
    /// Exempt classes that list any of the enumerated classes as a base class
    /// from needing to be moved into type-checking blocks.
    pub runtime_evaluated_base_classes: Option<Vec<String>>,
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            runtime-evaluated-decorators = ["attrs.define", "attrs.frozen"]
        "#
    )]
    /// Exempt classes decorated with any of the enumerated decorators from
    /// needing to be moved into type-checking blocks.
    pub runtime_evaluated_decorators: Option<Vec<String>>,
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub strict: bool,
    pub exempt_modules: Vec<String>,
    pub runtime_evaluated_base_classes: Vec<String>,
    pub runtime_evaluated_decorators: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            strict: false,
            exempt_modules: vec!["typing".to_string()],
            runtime_evaluated_base_classes: vec![],
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
            runtime_evaluated_base_classes: options
                .runtime_evaluated_base_classes
                .unwrap_or_default(),
            runtime_evaluated_decorators: options.runtime_evaluated_decorators.unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            strict: Some(settings.strict),
            exempt_modules: Some(settings.exempt_modules),
            runtime_evaluated_base_classes: Some(settings.runtime_evaluated_base_classes),
            runtime_evaluated_decorators: Some(settings.runtime_evaluated_decorators),
        }
    }
}
