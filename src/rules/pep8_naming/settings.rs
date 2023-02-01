//! Settings for the `pep8-naming` plugin.

use ruff_macros::ConfigurationOptions;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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

const CLASSMETHOD_DECORATORS: [&str; 1] = ["classmethod"];

const STATICMETHOD_DECORATORS: [&str; 1] = ["staticmethod"];

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Pep8NamingOptions"
)]
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
        default = r#"["classmethod"]"#,
        value_type = "list[str]",
        example = r#"
            # Allow Pydantic's `@validator` decorator to trigger class method treatment.
            classmethod-decorators = ["classmethod", "pydantic.validator"]
        "#
    )]
    /// A list of decorators that, when applied to a method, indicate that the
    /// method should be treated as a class method. For example, Ruff will
    /// expect that any method decorated by a decorator in this list takes a
    /// `cls` argument as its first argument.
    pub classmethod_decorators: Option<Vec<String>>,
    #[option(
        default = r#"["staticmethod"]"#,
        value_type = "list[str]",
        example = r#"
            # Allow a shorthand alias, `@stcmthd`, to trigger static method treatment.
            staticmethod-decorators = ["staticmethod", "stcmthd"]
        "#
    )]
    /// A list of decorators that, when applied to a method, indicate that the
    /// method should be treated as a static method. For example, Ruff will
    /// expect that any method decorated by a decorator in this list has no
    /// `self` or `cls` argument.
    pub staticmethod_decorators: Option<Vec<String>>,
}

#[derive(Debug, Hash)]
pub struct Settings {
    pub ignore_names: Vec<String>,
    pub classmethod_decorators: Vec<String>,
    pub staticmethod_decorators: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ignore_names: IGNORE_NAMES.map(String::from).to_vec(),
            classmethod_decorators: CLASSMETHOD_DECORATORS.map(String::from).to_vec(),
            staticmethod_decorators: STATICMETHOD_DECORATORS.map(String::from).to_vec(),
        }
    }
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            ignore_names: options
                .ignore_names
                .unwrap_or_else(|| IGNORE_NAMES.map(String::from).to_vec()),
            classmethod_decorators: options
                .classmethod_decorators
                .unwrap_or_else(|| CLASSMETHOD_DECORATORS.map(String::from).to_vec()),
            staticmethod_decorators: options
                .staticmethod_decorators
                .unwrap_or_else(|| STATICMETHOD_DECORATORS.map(String::from).to_vec()),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            ignore_names: Some(settings.ignore_names),
            classmethod_decorators: Some(settings.classmethod_decorators),
            staticmethod_decorators: Some(settings.staticmethod_decorators),
        }
    }
}
