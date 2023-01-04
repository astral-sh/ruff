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
        value_type = "Vec<String>",
        example = "hardcoded_tmp_directory = [\"/foo/bar\"]"
    )]
    /// List of directories that are considered temporary.
    pub hardcoded_tmp_directory: Option<Vec<String>>,
    #[option(
        default = "[]",
        value_type = "Vec<String>",
        example = "extend_hardcoded_tmp_directory = [\"/foo/bar\"]"
    )]
    /// List of directories that are considered temporary.
    /// These directories are added to the list in
    /// `hardcoded_tmp_directory`.
    pub hardcoded_tmp_directory_extend: Option<Vec<String>>,
}

#[derive(Debug, Hash)]
pub struct Settings {
    pub hardcoded_tmp_directory: Vec<String>,
    pub hardcoded_tmp_directory_extend: Vec<String>,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            hardcoded_tmp_directory: options
                .hardcoded_tmp_directory
                .unwrap_or_else(default_tmp_dirs),
            hardcoded_tmp_directory_extend: options
                .hardcoded_tmp_directory_extend
                .unwrap_or_default(),
        }
    }
}
impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            hardcoded_tmp_directory: Some(settings.hardcoded_tmp_directory),
            hardcoded_tmp_directory_extend: Some(settings.hardcoded_tmp_directory_extend),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hardcoded_tmp_directory: default_tmp_dirs(),
            hardcoded_tmp_directory_extend: Vec::new(),
        }
    }
}

impl Settings {
    /// Returns an iterator over all directories that are considered temporary.
    pub fn all_hardcoded_tmp_directories(&'_ self) -> impl Iterator<Item = &'_ String> {
        self.hardcoded_tmp_directory
            .iter()
            .chain(self.hardcoded_tmp_directory_extend.iter())
    }
}
