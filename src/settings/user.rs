//! Structs to render user-facing settings.

use std::collections::BTreeMap;
use std::path::PathBuf;

use regex::Regex;

use crate::checks_gen::CheckCodePrefix;
use crate::settings::types::{FilePattern, PythonVersion};
use crate::{flake8_quotes, pep8_naming, Configuration};

/// Struct to render user-facing exclusion patterns.
#[derive(Debug)]
#[allow(dead_code)]
pub struct Exclusion {
    basename: Option<String>,
    absolute: Option<String>,
}

impl Exclusion {
    pub fn from_file_pattern(file_pattern: FilePattern) -> Self {
        match file_pattern {
            FilePattern::Simple(basename) => Exclusion {
                basename: Some(basename.to_string()),
                absolute: None,
            },
            FilePattern::Complex(absolute, basename) => Exclusion {
                basename: basename.map(|pattern| pattern.to_string()),
                absolute: Some(absolute.to_string()),
            },
        }
    }
}

/// Struct to render user-facing configuration.
#[derive(Debug)]
pub struct UserConfiguration {
    pub dummy_variable_rgx: Regex,
    pub exclude: Vec<Exclusion>,
    pub extend_exclude: Vec<Exclusion>,
    pub extend_ignore: Vec<CheckCodePrefix>,
    pub extend_select: Vec<CheckCodePrefix>,
    pub ignore: Vec<CheckCodePrefix>,
    pub line_length: usize,
    pub per_file_ignores: BTreeMap<String, Vec<CheckCodePrefix>>,
    pub select: Vec<CheckCodePrefix>,
    pub target_version: PythonVersion,
    // Plugins
    pub flake8_quotes: flake8_quotes::settings::Settings,
    pub pep8_naming: pep8_naming::settings::Settings,
    // Non-settings exposed to the user
    pub project_root: Option<PathBuf>,
    pub pyproject: Option<PathBuf>,
}

impl UserConfiguration {
    pub fn from_configuration(
        configuration: Configuration,
        project_root: Option<PathBuf>,
        pyproject: Option<PathBuf>,
    ) -> Self {
        Self {
            dummy_variable_rgx: configuration.dummy_variable_rgx,
            exclude: configuration
                .exclude
                .into_iter()
                .map(Exclusion::from_file_pattern)
                .collect(),
            extend_exclude: configuration
                .extend_exclude
                .into_iter()
                .map(Exclusion::from_file_pattern)
                .collect(),
            extend_ignore: configuration.extend_ignore,
            extend_select: configuration.extend_select,
            ignore: configuration.ignore,
            line_length: configuration.line_length,
            per_file_ignores: configuration.per_file_ignores,
            select: configuration.select,
            target_version: configuration.target_version,
            flake8_quotes: configuration.flake8_quotes,
            pep8_naming: configuration.pep8_naming,
            project_root,
            pyproject,
        }
    }
}
