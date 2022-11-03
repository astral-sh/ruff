//! Settings for the `pep8-naming` plugin.

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

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    pub ignore_names: Option<Vec<String>>,
    pub classmethod_decorators: Option<Vec<String>>,
    pub staticmethod_decorators: Option<Vec<String>>,
}

#[derive(Debug, Hash)]
pub struct Settings {
    pub ignore_names: Vec<String>,
    pub classmethod_decorators: Vec<String>,
    pub staticmethod_decorators: Vec<String>,
}

impl Settings {
    pub fn from_options(options: Options) -> Self {
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

impl Default for Settings {
    fn default() -> Self {
        Self {
            ignore_names: IGNORE_NAMES.map(String::from).to_vec(),
            classmethod_decorators: CLASSMETHOD_DECORATORS.map(String::from).to_vec(),
            staticmethod_decorators: STATICMETHOD_DECORATORS.map(String::from).to_vec(),
        }
    }
}
