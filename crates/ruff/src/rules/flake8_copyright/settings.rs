//! Settings for the `flake8-copyright` plugin.

use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, CombineOptions, ConfigurationOptions};
#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, CombineOptions,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8CopyrightOptions"
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    #[option(
        default = "(?i)Copyright \\(C\\) \\d{4}",
        value_type = "str",
        example = r#"copyright-regexp = "(?i)Copyright \\(C\\) \\d{4}""#
    )]
    /// Regexp to check for copyright headers.
    ///
    /// Default is "Copyright\s+(\(C\)\s+)?\d{4}([-,]\d{4})*\s+%(author)s"
    /// E.g # Copyright (c) 2023 ruff
    pub copyright_regexp: Option<String>,
    #[option(
        default = "",
        value_type = "str",
        example = r#"copyright-author = "ruff""#
    )]
    /// Author to check for specifically within the header
    ///
    /// Default is ""
    /// E.g # Copyright (c) 2023 ruff
    pub copyright_author: Option<String>,
    #[option(
        default = r#"0"#,
        value_type = "int",
        example = r#"
            # Ensure copyright exists within 1024 characters of header
            copyright-min-file-size = 1024
        "#
    )]
    /// Minimum number of characters in a file before requiring a copyright notice
    pub copyright_min_file_size: Option<u32>,
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub copyright_regexp: String,
    pub copyright_author: String,
    pub copyright_min_file_size: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            copyright_regexp: String::from("(?i)Copyright \\(C\\) \\d{4}"),
            copyright_author: String::new(),
            copyright_min_file_size: 0,
        }
    }
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            copyright_regexp: options.copyright_regexp.unwrap_or_default(),
            copyright_author: options.copyright_author.unwrap_or_default(),
            copyright_min_file_size: options.copyright_min_file_size.unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            copyright_regexp: Some(settings.copyright_regexp),
            copyright_author: Some(settings.copyright_author),
            copyright_min_file_size: Some(settings.copyright_min_file_size),
        }
    }
}
