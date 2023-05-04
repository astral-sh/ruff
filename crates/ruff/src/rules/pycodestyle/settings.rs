//! Settings for the `pycodestyle` plugin.

use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, CombineOptions, ConfigurationOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, CacheKey)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct TabSize(pub u8);

impl From<TabSize> for usize {
    fn from(tab_size: TabSize) -> Self {
        tab_size.0 as usize
    }
}

impl From<u8> for TabSize {
    fn from(tab_size: u8) -> Self {
        Self(tab_size)
    }
}

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
    pub max_doc_length: Option<usize>,
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
    #[option(
        default = "4",
        value_type = "int",
        example = r#"
            tabulation-length = 8
        "#
    )]
    /// The tabulation length to use when enforcing long-lines violations (like
    /// `E501`).
    pub tab_size: Option<TabSize>,
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub max_doc_length: Option<usize>,
    pub ignore_overlong_task_comments: bool,
    pub tab_size: TabSize,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        let default = Self::default();
        Self {
            max_doc_length: options.max_doc_length,
            ignore_overlong_task_comments: options
                .ignore_overlong_task_comments
                .unwrap_or(default.ignore_overlong_task_comments),
            tab_size: options.tab_size.unwrap_or(default.tab_size),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            max_doc_length: settings.max_doc_length,
            ignore_overlong_task_comments: Some(settings.ignore_overlong_task_comments),
            tab_size: Some(settings.tab_size),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_doc_length: None,
            ignore_overlong_task_comments: false,
            tab_size: TabSize(4),
        }
    }
}
