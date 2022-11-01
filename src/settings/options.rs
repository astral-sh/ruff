//! Options that the user can provide via pyproject.toml.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::checks_gen::CheckCodePrefix;
use crate::settings::types::PythonVersion;
use crate::{flake8_quotes, pep8_naming};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    pub line_length: Option<usize>,
    pub exclude: Option<Vec<String>>,
    pub extend_exclude: Option<Vec<String>>,
    pub select: Option<Vec<CheckCodePrefix>>,
    pub extend_select: Option<Vec<CheckCodePrefix>>,
    pub ignore: Option<Vec<CheckCodePrefix>>,
    pub extend_ignore: Option<Vec<CheckCodePrefix>>,
    pub per_file_ignores: Option<BTreeMap<String, Vec<CheckCodePrefix>>>,
    pub dummy_variable_rgx: Option<String>,
    pub target_version: Option<PythonVersion>,
    // Plugins
    pub flake8_quotes: Option<flake8_quotes::settings::Options>,
    pub pep8_naming: Option<pep8_naming::settings::Options>,
}
