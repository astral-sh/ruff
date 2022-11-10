//! Options that the user can provide via pyproject.toml.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::checks_gen::CheckCodePrefix;
use crate::settings::types::PythonVersion;
use crate::{flake8_annotations, flake8_quotes, isort, pep8_naming};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    pub dummy_variable_rgx: Option<String>,
    pub exclude: Option<Vec<String>>,
    pub extend_exclude: Option<Vec<String>>,
    pub extend_ignore: Option<Vec<CheckCodePrefix>>,
    pub extend_select: Option<Vec<CheckCodePrefix>>,
    pub fix: Option<bool>,
    pub ignore: Option<Vec<CheckCodePrefix>>,
    pub line_length: Option<usize>,
    pub per_file_ignores: Option<BTreeMap<String, Vec<CheckCodePrefix>>>,
    pub select: Option<Vec<CheckCodePrefix>>,
    pub src_paths: Option<Vec<String>>,
    pub target_version: Option<PythonVersion>,
    // Plugins
    pub flake8_annotations: Option<flake8_annotations::settings::Options>,
    pub flake8_quotes: Option<flake8_quotes::settings::Options>,
    pub isort: Option<isort::settings::Options>,
    pub pep8_naming: Option<pep8_naming::settings::Options>,
}
