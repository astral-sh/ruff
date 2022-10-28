//! Structs to render user-facing settings.

use std::path::PathBuf;

use regex::Regex;

use crate::checks_gen::CheckCodePrefix;
use crate::settings::types::{FilePattern, PerFileIgnore, PythonVersion};
use crate::{flake8_quotes, Configuration};

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
    pub per_file_ignores: Vec<PerFileIgnore>,
    pub select: Vec<CheckCodePrefix>,
    pub target_version: PythonVersion,
    // Plugins
    pub flake8_quotes: flake8_quotes::settings::Settings,
    // Non-settings exposed to the user
    pub project_root: Option<PathBuf>,
    pub pyproject: Option<PathBuf>,
}

impl UserConfiguration {
    pub fn from_configuration(
        settings: Configuration,
        project_root: Option<PathBuf>,
        pyproject: Option<PathBuf>,
    ) -> Self {
        Self {
            dummy_variable_rgx: settings.dummy_variable_rgx,
            exclude: settings
                .exclude
                .into_iter()
                .map(Exclusion::from_file_pattern)
                .collect(),
            extend_exclude: settings
                .extend_exclude
                .into_iter()
                .map(Exclusion::from_file_pattern)
                .collect(),
            extend_ignore: settings.extend_ignore,
            extend_select: settings.extend_select,
            ignore: settings.ignore,
            line_length: settings.line_length,
            per_file_ignores: settings.per_file_ignores,
            select: settings.select,
            target_version: settings.target_version,
            flake8_quotes: settings.flake8_quotes,
            project_root,
            pyproject,
        }
    }
}
