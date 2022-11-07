//! User-provided program settings, taking into account pyproject.toml and
//! command-line options. Structure mirrors the user-facing representation of
//! the various parameters.

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use regex::Regex;

use crate::checks_gen::CheckCodePrefix;
use crate::settings::pyproject::load_options;
use crate::settings::types::{FilePattern, PerFileIgnore, PythonVersion};
use crate::{flake8_annotations, flake8_quotes, pep8_naming};

#[derive(Debug)]
pub struct Configuration {
    pub dummy_variable_rgx: Regex,
    pub exclude: Vec<FilePattern>,
    pub extend_exclude: Vec<FilePattern>,
    pub extend_ignore: Vec<CheckCodePrefix>,
    pub extend_select: Vec<CheckCodePrefix>,
    pub fix: bool,
    pub ignore: Vec<CheckCodePrefix>,
    pub line_length: usize,
    pub per_file_ignores: Vec<PerFileIgnore>,
    pub select: Vec<CheckCodePrefix>,
    pub target_version: PythonVersion,
    // Plugins
    pub flake8_annotations: flake8_annotations::settings::Settings,
    pub flake8_quotes: flake8_quotes::settings::Settings,
    pub pep8_naming: pep8_naming::settings::Settings,
}

static DEFAULT_EXCLUDE: Lazy<Vec<FilePattern>> = Lazy::new(|| {
    vec![
        FilePattern::Simple(".bzr"),
        FilePattern::Simple(".direnv"),
        FilePattern::Simple(".eggs"),
        FilePattern::Simple(".git"),
        FilePattern::Simple(".hg"),
        FilePattern::Simple(".mypy_cache"),
        FilePattern::Simple(".nox"),
        FilePattern::Simple(".pants.d"),
        FilePattern::Simple(".ruff_cache"),
        FilePattern::Simple(".svn"),
        FilePattern::Simple(".tox"),
        FilePattern::Simple(".venv"),
        FilePattern::Simple("__pypackages__"),
        FilePattern::Simple("_build"),
        FilePattern::Simple("buck-out"),
        FilePattern::Simple("build"),
        FilePattern::Simple("dist"),
        FilePattern::Simple("node_modules"),
        FilePattern::Simple("venv"),
    ]
});

static DEFAULT_DUMMY_VARIABLE_RGX: Lazy<Regex> =
    Lazy::new(|| Regex::new("^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$").unwrap());

impl Configuration {
    pub fn from_pyproject(
        pyproject: &Option<PathBuf>,
        project_root: &Option<PathBuf>,
    ) -> Result<Self> {
        let options = load_options(pyproject)?;
        Ok(Configuration {
            dummy_variable_rgx: match options.dummy_variable_rgx {
                Some(pattern) => Regex::new(&pattern)
                    .map_err(|e| anyhow!("Invalid dummy-variable-rgx value: {e}"))?,
                None => DEFAULT_DUMMY_VARIABLE_RGX.clone(),
            },
            target_version: options.target_version.unwrap_or(PythonVersion::Py310),
            exclude: options
                .exclude
                .map(|paths| {
                    paths
                        .iter()
                        .map(|path| FilePattern::from_user(path, project_root))
                        .collect()
                })
                .unwrap_or_else(|| DEFAULT_EXCLUDE.clone()),
            extend_exclude: options
                .extend_exclude
                .unwrap_or_default()
                .iter()
                .map(|path| FilePattern::from_user(path, project_root))
                .collect(),
            extend_ignore: options.extend_ignore.unwrap_or_default(),
            select: options
                .select
                .unwrap_or_else(|| vec![CheckCodePrefix::E, CheckCodePrefix::F]),
            extend_select: options.extend_select.unwrap_or_default(),
            fix: options.fix.unwrap_or_default(),
            ignore: options.ignore.unwrap_or_default(),
            line_length: options.line_length.unwrap_or(88),
            per_file_ignores: options
                .per_file_ignores
                .map(|per_file_ignores| {
                    per_file_ignores
                        .iter()
                        .map(|(pattern, prefixes)| {
                            PerFileIgnore::new(pattern, prefixes, project_root)
                        })
                        .collect()
                })
                .unwrap_or_default(),
            // Plugins
            flake8_annotations: options
                .flake8_annotations
                .map(flake8_annotations::settings::Settings::from_options)
                .unwrap_or_default(),
            flake8_quotes: options
                .flake8_quotes
                .map(flake8_quotes::settings::Settings::from_options)
                .unwrap_or_default(),
            pep8_naming: options
                .pep8_naming
                .map(pep8_naming::settings::Settings::from_options)
                .unwrap_or_default(),
        })
    }
}
