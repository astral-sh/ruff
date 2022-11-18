//! Options that the user can provide via pyproject.toml.

use fnv::FnvHashMap;
use serde::{Deserialize, Serialize};

use crate::checks_gen::CheckCodePrefix;
use crate::settings::types::PythonVersion;
use crate::{
    flake8_annotations, flake8_bugbear, flake8_quotes, flake8_tidy_imports, isort, mccabe,
    pep8_naming,
};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    pub dummy_variable_rgx: Option<String>,
    pub exclude: Option<Vec<String>>,
    pub extend_exclude: Option<Vec<String>>,
    pub extend_ignore: Option<Vec<CheckCodePrefix>>,
    pub extend_select: Option<Vec<CheckCodePrefix>>,
    pub fix: Option<bool>,
    pub fixable: Option<Vec<CheckCodePrefix>>,
    pub ignore: Option<Vec<CheckCodePrefix>>,
    pub line_length: Option<usize>,
    pub select: Option<Vec<CheckCodePrefix>>,
    pub show_source: Option<bool>,
    pub src: Option<Vec<String>>,
    pub target_version: Option<PythonVersion>,
    pub unfixable: Option<Vec<CheckCodePrefix>>,
    // Plugins
    pub flake8_annotations: Option<flake8_annotations::settings::Options>,
    pub flake8_bugbear: Option<flake8_bugbear::settings::Options>,
    pub flake8_quotes: Option<flake8_quotes::settings::Options>,
    pub flake8_tidy_imports: Option<flake8_tidy_imports::settings::Options>,
    pub isort: Option<isort::settings::Options>,
    pub mccabe: Option<mccabe::settings::Options>,
    pub pep8_naming: Option<pep8_naming::settings::Options>,
    // Tables are required to go last.
    pub per_file_ignores: Option<FnvHashMap<String, Vec<CheckCodePrefix>>>,
}
