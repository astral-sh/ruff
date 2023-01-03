//! Settings for the `pycodestyle` plugin.

use ruff_macros::ConfigurationOptions;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", rename = "Pycodestyle")]
pub struct Options {
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            ignore-overlong-task-comments = true
        "#
    )]
    /// Whether or not line-length checks (`E501`) should be triggered for
    /// comments starting with `task-tags` (by default: ["TODO", "FIXME",
    /// and "XXX"]).
    pub ignore_overlong_task_comments: Option<bool>,
}

#[derive(Debug, Default, Hash)]
pub struct Settings {
    pub ignore_overlong_task_comments: bool,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            ignore_overlong_task_comments: options
                .ignore_overlong_task_comments
                .unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            ignore_overlong_task_comments: Some(settings.ignore_overlong_task_comments),
        }
    }
}
