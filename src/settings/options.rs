//! Options that the user can provide via pyproject.toml.

use serde::Deserialize;

use crate::checks_gen::CheckCodePrefix;
use crate::settings::types::{PythonVersion, StrCheckCodePair};
use crate::{flake8_quotes, pep8_naming};

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
    // Plugins
    pub flake8_quotes: Option<flake8_quotes::settings::Options>,
    pub pep8_naming: Option<pep8_naming::settings::Options>,
}
