//! Settings for the `pycodestyle` plugin.

use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, CombineOptions, ConfigurationOptions};

use crate::line_width::LineLength;

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", rename = "Pycodestyle")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    #[option(
        default = "None",
        value_type = "int",
        example = r#"
            max-doc-length = 88
        "#
    )]
    /// The maximum line length to allow for line-length violations within
    /// documentation (`W505`), including standalone comments.
    pub max_doc_length: Option<LineLength>,
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            ignore-overlong-task-comments = true
        "#
    )]
    /// Whether line-length violations (`E501`) should be triggered for
    /// comments starting with `task-tags` (by default: \["TODO", "FIXME",
    /// and "XXX"\]).
    pub ignore_overlong_task_comments: Option<bool>,
}

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub max_doc_length: Option<LineLength>,
    pub ignore_overlong_task_comments: bool,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            max_doc_length: options.max_doc_length,
            ignore_overlong_task_comments: options
                .ignore_overlong_task_comments
                .unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            max_doc_length: settings.max_doc_length,
            ignore_overlong_task_comments: Some(settings.ignore_overlong_task_comments),
        }
    }
}
