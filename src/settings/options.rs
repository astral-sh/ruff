//! Options that the user can provide via pyproject.toml.

use serde::Deserialize;

use crate::checks_gen::CheckCodePrefix;
use crate::flake8_quotes;
use crate::settings::types::{PythonVersion, StrCheckCodePair};

#[derive(Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    pub line_length: Option<usize>,
    pub exclude: Option<Vec<String>>,
    #[serde(default)]
    pub extend_exclude: Vec<String>,
    pub select: Option<Vec<CheckCodePrefix>>,
    #[serde(default)]
    pub extend_select: Vec<CheckCodePrefix>,
    #[serde(default)]
    pub ignore: Vec<CheckCodePrefix>,
    #[serde(default)]
    pub extend_ignore: Vec<CheckCodePrefix>,
    #[serde(default)]
    pub per_file_ignores: Vec<StrCheckCodePair>,
    pub dummy_variable_rgx: Option<String>,
    pub target_version: Option<PythonVersion>,
    pub flake8_quotes: Option<flake8_quotes::settings::Options>,
}
