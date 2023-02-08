//! Settings for the `flake8-bandit` plugin.

use ruff_macros::ConfigurationOptions;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn default_tmp_dirs() -> Vec<String> {
    ["/tmp", "/var/tmp", "/dev/shm"]
        .map(std::string::ToString::to_string)
        .to_vec()
}

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8BanditOptions"
)]
pub struct Options {
    #[option(
        default = "[\"/tmp\", \"/var/tmp\", \"/dev/shm\"]",
        value_type = "list[str]",
        example = "hardcoded-tmp-directory = [\"/foo/bar\"]"
    )]
    /// A list of directories to consider temporary.
    pub hardcoded_tmp_directory: Option<Vec<String>>,
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = "extend-hardcoded-tmp-directory = [\"/foo/bar\"]"
    )]
    /// A list of directories to consider temporary, in addition to those
    /// specified by `hardcoded-tmp-directory`.
    pub hardcoded_tmp_directory_extend: Option<Vec<String>>,
    #[option(
        default = "false",
        value_type = "bool",
        example = "check-typed-exception = true"
    )]
    /// Whether to disallow `try`-`except`-`pass` (`S110`) for specific
    /// exception types. By default, `try`-`except`-`pass` is only
    /// disallowed for `Exception` and `BaseException`.
    pub check_typed_exception: Option<bool>,
}

#[derive(Debug, Hash)]
pub struct Settings {
    pub hardcoded_tmp_directory: Vec<String>,
    pub check_typed_exception: bool,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            hardcoded_tmp_directory: options
                .hardcoded_tmp_directory
                .unwrap_or_else(default_tmp_dirs)
                .into_iter()
                .chain(
                    options
                        .hardcoded_tmp_directory_extend
                        .unwrap_or_default()
                        .into_iter(),
                )
                .collect(),
            check_typed_exception: options.check_typed_exception.unwrap_or(false),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            hardcoded_tmp_directory: Some(settings.hardcoded_tmp_directory),
            hardcoded_tmp_directory_extend: None,
            check_typed_exception: Some(settings.check_typed_exception),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hardcoded_tmp_directory: default_tmp_dirs(),
            check_typed_exception: false,
        }
    }
}
