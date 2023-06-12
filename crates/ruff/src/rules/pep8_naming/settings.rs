//! Settings for the `pep8-naming` plugin.

use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, CombineOptions, ConfigurationOptions};

use crate::settings::types::IdenifierMatcher;

const IGNORE_NAMES: [&str; 12] = [
    "setUp",
    "tearDown",
    "setUpClass",
    "tearDownClass",
    "setUpModule",
    "tearDownModule",
    "asyncSetUp",
    "asyncTearDown",
    "setUpTestData",
    "failureException",
    "longMessage",
    "maxDiff",
];

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, CombineOptions,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Pep8NamingOptions"
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    #[option(
        default = r#"["setUp", "tearDown", "setUpClass", "tearDownClass", "setUpModule", "tearDownModule", "asyncSetUp", "asyncTearDown", "setUpTestData", "failureException", "longMessage", "maxDiff"]"#,
        value_type = "list[str]",
        example = r#"
            ignore-names = ["callMethod"]
        "#
    )]
    /// A list of names to ignore when considering `pep8-naming` violations.
    pub ignore_names: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            # Allow Pydantic's `@validator` decorator to trigger class method treatment.
            classmethod-decorators = ["pydantic.validator"]
        "#
    )]
    /// A list of decorators that, when applied to a method, indicate that the
    /// method should be treated as a class method (in addition to the builtin
    /// `@classmethod`).
    ///
    /// For example, Ruff will expect that any method decorated by a decorator
    /// in this list takes a `cls` argument as its first argument.
    pub classmethod_decorators: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            # Allow a shorthand alias, `@stcmthd`, to trigger static method treatment.
            staticmethod-decorators = ["stcmthd"]
        "#
    )]
    /// A list of decorators that, when applied to a method, indicate that the
    /// method should be treated as a static method (in addition to the builtin
    /// `@staticmethod`).
    ///
    /// For example, Ruff will expect that any method decorated by a decorator
    /// in this list has no `self` or `cls` argument.
    pub staticmethod_decorators: Option<Vec<String>>,
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub ignore_names: Vec<IdenifierMatcher>,
    pub classmethod_decorators: Vec<String>,
    pub staticmethod_decorators: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ignore_names: IGNORE_NAMES
                .iter()
                .map(|name| IdenifierMatcher::from(*name))
                .collect(),
            classmethod_decorators: Vec::new(),
            staticmethod_decorators: Vec::new(),
        }
    }
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            ignore_names: match options.ignore_names {
                Some(names) => names
                    .into_iter()
                    .map(|name| IdenifierMatcher::from(name))
                    .collect(),
                None => IGNORE_NAMES
                    .into_iter()
                    .map(|name| IdenifierMatcher::from(name))
                    .collect(),
            },
            classmethod_decorators: options.classmethod_decorators.unwrap_or_default(),
            staticmethod_decorators: options.staticmethod_decorators.unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            ignore_names: Some(
                settings
                    .ignore_names
                    .into_iter()
                    .map(String::from)
                    .collect(),
            ),
            classmethod_decorators: Some(settings.classmethod_decorators),
            staticmethod_decorators: Some(settings.staticmethod_decorators),
        }
    }
}
