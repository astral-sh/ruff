//! User-provided program settings, taking into account pyproject.toml and
//! command-line options. Structure mirrors the user-facing representation of
//! the various parameters.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use path_absolutize::path_dedot;
use regex::Regex;

use crate::checks_gen::CheckCodePrefix;
use crate::settings::pyproject::load_options;
use crate::settings::types::{create_glob, PerFileIgnore, PythonVersion};
use crate::{
    flake8_annotations, flake8_bugbear, flake8_quotes, flake8_tidy_imports, fs, isort, mccabe,
    pep8_naming,
};

#[derive(Debug)]
pub struct Configuration {
    pub dummy_variable_rgx: Regex,
    pub exclude: globset::GlobSet,
    pub extend_exclude: globset::GlobSet,
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
    pub flake8_bugbear: flake8_bugbear::settings::Settings,
    pub flake8_quotes: flake8_quotes::settings::Settings,
    pub flake8_tidy_imports: flake8_tidy_imports::settings::Settings,
    pub isort: isort::settings::Settings,
    pub mccabe: mccabe::settings::Settings,
    pub pep8_naming: pep8_naming::settings::Settings,
    // Display Info
    pub exclude_globs: Vec<(globset::Glob, Option<globset::Glob>)>,
    pub extend_exclude_globs: Vec<(globset::Glob, Option<globset::Glob>)>,
}

static DEFAULT_EXCLUDE: Lazy<Vec<(globset::Glob, Option<globset::Glob>)>> = Lazy::new(|| {
    vec![
        (
            globset::Glob::new(".bzr").expect("Failed to build Glob"),
            None,
        ),
        (
            globset::Glob::new(".direnv").expect("Failed to build Glob"),
            None,
        ),
        (
            globset::Glob::new(".eggs").expect("Failed to build Glob"),
            None,
        ),
        (
            globset::Glob::new(".git").expect("Failed to build Glob"),
            None,
        ),
        (
            globset::Glob::new(".hg").expect("Failed to build Glob"),
            None,
        ),
        (
            globset::Glob::new(".mypy_cache").expect("Failed to build Glob"),
            None,
        ),
        (
            globset::Glob::new(".nox").expect("Failed to build Glob"),
            None,
        ),
        (
            globset::Glob::new(".pants.d").expect("Failed to build Glob"),
            None,
        ),
        (
            globset::Glob::new(".ruff_cache").expect("Failed to build Glob"),
            None,
        ),
        (
            globset::Glob::new(".svn").expect("Failed to build Glob"),
            None,
        ),
        (
            globset::Glob::new(".tox").expect("Failed to build Glob"),
            None,
        ),
        (
            globset::Glob::new(".venv").expect("Failed to build Glob"),
            None,
        ),
        (
            globset::Glob::new("__pypackages__").expect("Failed to build Glob"),
            None,
        ),
        (
            globset::Glob::new("_build").expect("Failed to build Glob"),
            None,
        ),
        (
            globset::Glob::new("buck-out").expect("Failed to build Glob"),
            None,
        ),
        (
            globset::Glob::new("build").expect("Failed to build Glob"),
            None,
        ),
        (
            globset::Glob::new("dist").expect("Failed to build Glob"),
            None,
        ),
        (
            globset::Glob::new("node_modules").expect("Failed to build Glob"),
            None,
        ),
        (
            globset::Glob::new("venv").expect("Failed to build Glob"),
            None,
        ),
    ]
});

static DEFAULT_DUMMY_VARIABLE_RGX: Lazy<Regex> =
    Lazy::new(|| Regex::new("^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$").unwrap());

impl Configuration {
    pub fn from_pyproject(
        pyproject: Option<&PathBuf>,
        project_root: Option<&PathBuf>,
    ) -> Result<Self> {
        let options = load_options(pyproject)?;

        let local_to_glob =
            |path: String| create_glob(&path, project_root).expect("Failed to build Glob");

        let unfurl = |mut vec: Vec<globset::Glob>,
                      item: &(globset::Glob, Option<globset::Glob>)| {
            vec.push(item.0.clone());
            if let Some(glob) = item.1.clone() {
                vec.push(glob);
            };
            vec
        };

        let exclude = {
            let mut builder = globset::GlobSetBuilder::new();

            let paths = options.exclude.map_or_else(
                || DEFAULT_EXCLUDE.clone(),
                |paths| paths.into_iter().map(local_to_glob).collect(),
            );

            for path in paths.iter().fold(vec![], unfurl) {
                builder.add(path.clone());
            }
            (builder.build()?, paths)
        };

        let extend_exclude = {
            let mut builder = globset::GlobSetBuilder::new();
            let paths: Vec<(globset::Glob, Option<globset::Glob>)> = options
                .extend_exclude
                .unwrap_or_default()
                .into_iter()
                .map(local_to_glob)
                .collect();

            for path in &paths.iter().fold(vec![], unfurl) {
                builder.add(path.clone());
            }
            (builder.build()?, paths)
        };

        Ok(Configuration {
            dummy_variable_rgx: match options.dummy_variable_rgx {
                Some(pattern) => Regex::new(&pattern)
                    .map_err(|e| anyhow!("Invalid dummy-variable-rgx value: {e}"))?,
                None => DEFAULT_DUMMY_VARIABLE_RGX.clone(),
            },
            src: options.src.map_or_else(
                || {
                    vec![match project_root {
                        Some(project_root) => project_root.clone(),
                        None => path_dedot::CWD.clone(),
                    }]
                },
                |src| {
                    src.iter()
                        .map(|path| {
                            let path = Path::new(path);
                            match project_root {
                                Some(project_root) => fs::normalize_path_to(path, project_root),
                                None => fs::normalize_path(path),
                            }
                        })
                        .collect()
                },
            ),
            target_version: options.target_version.unwrap_or(PythonVersion::Py310),
            exclude: exclude.0,
            extend_exclude: extend_exclude.0,
            exclude_globs: exclude.1,
            extend_exclude_globs: extend_exclude.1,
            extend_ignore: options.extend_ignore.unwrap_or_default(),
            select: options
                .select
                .unwrap_or_else(|| vec![CheckCodePrefix::E, CheckCodePrefix::F]),
            extend_select: options.extend_select.unwrap_or_default(),
            fix: options.fix.unwrap_or_default(),
            fixable: options.fixable.unwrap_or_else(|| {
                // TODO(charlie): Autogenerate this list.
                vec![
                    CheckCodePrefix::A,
                    CheckCodePrefix::B,
                    CheckCodePrefix::BLE,
                    CheckCodePrefix::C,
                    CheckCodePrefix::D,
                    CheckCodePrefix::E,
                    CheckCodePrefix::F,
                    CheckCodePrefix::I,
                    CheckCodePrefix::M,
                    CheckCodePrefix::N,
                    CheckCodePrefix::Q,
                    CheckCodePrefix::RUF,
                    CheckCodePrefix::S,
                    CheckCodePrefix::T,
                    CheckCodePrefix::U,
                    CheckCodePrefix::W,
                    CheckCodePrefix::YTT,
                ]
            }),
            unfixable: options.unfixable.unwrap_or_default(),
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
                .transpose()?
                .unwrap_or_default(),
            show_source: options.show_source.unwrap_or_default(),
            // Plugins
            flake8_annotations: options
                .flake8_annotations
                .map(flake8_annotations::settings::Settings::from_options)
                .unwrap_or_default(),
            flake8_bugbear: options
                .flake8_bugbear
                .map(flake8_bugbear::settings::Settings::from_options)
                .unwrap_or_default(),
            flake8_quotes: options
                .flake8_quotes
                .map(flake8_quotes::settings::Settings::from_options)
                .unwrap_or_default(),
            flake8_tidy_imports: options
                .flake8_tidy_imports
                .map(flake8_tidy_imports::settings::Settings::from_options)
                .unwrap_or_default(),
            isort: options
                .isort
                .map(isort::settings::Settings::from_options)
                .unwrap_or_default(),
            mccabe: options
                .mccabe
                .map(mccabe::settings::Settings::from_options)
                .unwrap_or_default(),
            pep8_naming: options
                .pep8_naming
                .map(pep8_naming::settings::Settings::from_options)
                .unwrap_or_default(),
        })
    }
}
