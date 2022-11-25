//! Structs to render user-facing settings.

use std::path::PathBuf;

use regex::Regex;

use crate::checks_gen::CheckCodePrefix;
use crate::settings::types::{FilePattern, PerFileIgnore, PythonVersion};
use crate::{
    flake8_annotations, flake8_quotes, flake8_tidy_imports, isort, pep8_naming, Configuration,
};

/// Struct to render user-facing configuration.
#[derive(Debug)]
pub struct UserConfiguration {
    pub dummy_variable_rgx: Regex,
    pub exclude: Vec<FilePattern>,
    pub extend_exclude: Vec<FilePattern>,
    pub extend_ignore: Vec<CheckCodePrefix>,
    pub extend_select: Vec<CheckCodePrefix>,
    pub fix: bool,
    pub fixable: Vec<CheckCodePrefix>,
    pub ignore: Vec<CheckCodePrefix>,
    pub line_length: usize,
    pub per_file_ignores: Vec<PerFileIgnore>,
    pub select: Vec<CheckCodePrefix>,
    pub show_source: bool,
    pub src: Vec<PathBuf>,
    pub target_version: PythonVersion,
    pub unfixable: Vec<CheckCodePrefix>,
    // Plugins
    pub flake8_annotations: flake8_annotations::settings::Settings,
    pub flake8_quotes: flake8_quotes::settings::Settings,
    pub flake8_tidy_imports: flake8_tidy_imports::settings::Settings,
    pub isort: isort::settings::Settings,
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
            exclude: configuration.exclude,
            extend_exclude: configuration.extend_exclude,
            extend_ignore: configuration.extend_ignore,
            extend_select: configuration.extend_select,
            fix: configuration.fix,
            fixable: configuration.fixable,
            unfixable: configuration.unfixable,
            ignore: configuration.ignore,
            line_length: configuration.line_length,
            per_file_ignores: configuration.per_file_ignores,
            select: configuration.select,
            src: configuration.src,
            target_version: configuration.target_version,
            show_source: configuration.show_source,
            flake8_annotations: configuration.flake8_annotations,
            flake8_quotes: configuration.flake8_quotes,
            flake8_tidy_imports: configuration.flake8_tidy_imports,
            isort: configuration.isort,
            pep8_naming: configuration.pep8_naming,
            project_root,
            pyproject,
        }
    }
}
