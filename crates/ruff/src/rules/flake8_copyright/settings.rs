//! Settings for the `flake8-copyright` plugin.

use once_cell::sync::Lazy;
use regex::Regex;
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
        default = r#"(?i)Copyright\s+(\(C\)\s+)?\d{4}([-,]\d{4})*"#,
        value_type = "str",
        example = r#"notice-rgx = "(?i)Copyright \\(C\\) \\d{4}""#
    )]
    /// The regular expression used to match the copyright notice, compiled
    /// with the [`regex`](https://docs.rs/regex/latest/regex/) crate.
    ///
    /// Defaults to `(?i)Copyright\s+(\(C\)\s+)?\d{4}(-\d{4})*`, which matches
    /// the following:
    /// - `Copyright 2023`
    /// - `Copyright (C) 2023`
    /// - `Copyright 2021-2023`
    /// - `Copyright (C) 2021-2023`
    pub notice_rgx: Option<String>,
    #[option(default = "None", value_type = "str", example = r#"author = "Ruff""#)]
    /// Author to enforce within the copyright notice. If provided, the
    /// author must be present immediately following the copyright notice.
    pub author: Option<String>,
    #[option(
        default = r#"0"#,
        value_type = "int",
        example = r#"
            # Avoid enforcing a header on files smaller than 1024 bytes.
            min-file-size = 1024
        "#
    )]
    /// A minimum file size (in bytes) required for a copyright notice to
    /// be enforced. By default, all files are validated.
    pub min_file_size: Option<usize>,
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub notice_rgx: Regex,
    pub author: Option<String>,
    pub min_file_size: usize,
}

static COPYRIGHT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)Copyright\s+(\(C\)\s+)?\d{4}(-\d{4})*").unwrap());

impl Default for Settings {
    fn default() -> Self {
        Self {
            notice_rgx: COPYRIGHT.clone(),
            author: None,
            min_file_size: 0,
        }
    }
}

impl TryFrom<Options> for Settings {
    type Error = anyhow::Error;

    fn try_from(value: Options) -> Result<Self, Self::Error> {
        Ok(Self {
            notice_rgx: value
                .notice_rgx
                .map(|pattern| Regex::new(&pattern))
                .transpose()?
                .unwrap_or_else(|| COPYRIGHT.clone()),
            author: value.author,
            min_file_size: value.min_file_size.unwrap_or_default(),
        })
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            notice_rgx: Some(settings.notice_rgx.to_string()),
            author: settings.author,
            min_file_size: Some(settings.min_file_size),
        }
    }
}
