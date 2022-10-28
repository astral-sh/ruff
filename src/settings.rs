use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, Result};
use glob::Pattern;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::checks::CheckCode;
use crate::checks_gen::{CheckCodePrefix, PrefixSpecificity};
use crate::pyproject::{load_config, StrCheckCodePair};
use crate::{flake8_quotes, fs};

#[derive(Clone, Debug, PartialOrd, PartialEq, Eq, Serialize, Deserialize)]
pub enum PythonVersion {
    Py33,
    Py34,
    Py35,
    Py36,
    Py37,
    Py38,
    Py39,
    Py310,
    Py311,
}

impl FromStr for PythonVersion {
    type Err = anyhow::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string {
            "py33" => Ok(PythonVersion::Py33),
            "py34" => Ok(PythonVersion::Py34),
            "py35" => Ok(PythonVersion::Py35),
            "py36" => Ok(PythonVersion::Py36),
            "py37" => Ok(PythonVersion::Py37),
            "py38" => Ok(PythonVersion::Py38),
            "py39" => Ok(PythonVersion::Py39),
            "py310" => Ok(PythonVersion::Py310),
            "py311" => Ok(PythonVersion::Py311),
            _ => Err(anyhow!("Unknown version: {}", string)),
        }
    }
}

#[derive(Debug, Clone, Hash)]
pub enum FilePattern {
    Simple(&'static str),
    Complex(Pattern, Option<Pattern>),
}

impl FilePattern {
    pub fn from_user(pattern: &str, project_root: &Option<PathBuf>) -> Self {
        let path = Path::new(pattern);
        let absolute_path = match project_root {
            Some(project_root) => fs::normalize_path_to(path, project_root),
            None => fs::normalize_path(path),
        };

        let absolute = Pattern::new(&absolute_path.to_string_lossy()).expect("Invalid pattern.");
        let basename = if !pattern.contains(std::path::MAIN_SEPARATOR) {
            Some(Pattern::new(pattern).expect("Invalid pattern."))
        } else {
            None
        };

        FilePattern::Complex(absolute, basename)
    }
}

#[derive(Debug, Clone, Hash)]
pub struct PerFileIgnore {
    pub pattern: FilePattern,
    pub codes: BTreeSet<CheckCode>,
}

impl PerFileIgnore {
    pub fn new(user_in: StrCheckCodePair, project_root: &Option<PathBuf>) -> Self {
        let pattern = FilePattern::from_user(user_in.pattern.as_str(), project_root);
        let codes = BTreeSet::from_iter(user_in.code.codes());
        Self { pattern, codes }
    }
}

#[derive(Debug)]
pub struct RawSettings {
    pub dummy_variable_rgx: Regex,
    pub exclude: Vec<FilePattern>,
    pub extend_exclude: Vec<FilePattern>,
    pub extend_ignore: Vec<CheckCodePrefix>,
    pub extend_select: Vec<CheckCodePrefix>,
    pub ignore: Vec<CheckCodePrefix>,
    pub line_length: usize,
    pub per_file_ignores: Vec<PerFileIgnore>,
    pub select: Vec<CheckCodePrefix>,
    pub target_version: PythonVersion,
    // Plugins
    pub flake8_quotes: flake8_quotes::settings::Settings,
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

impl RawSettings {
    pub fn from_pyproject(
        pyproject: &Option<PathBuf>,
        project_root: &Option<PathBuf>,
        quiet: bool,
    ) -> Result<Self> {
        let config = load_config(pyproject, quiet)?;
        Ok(RawSettings {
            dummy_variable_rgx: match config.dummy_variable_rgx {
                Some(pattern) => Regex::new(&pattern)
                    .map_err(|e| anyhow!("Invalid dummy-variable-rgx value: {e}"))?,
                None => DEFAULT_DUMMY_VARIABLE_RGX.clone(),
            },
            target_version: config.target_version.unwrap_or(PythonVersion::Py310),
            exclude: config
                .exclude
                .map(|paths| {
                    paths
                        .iter()
                        .map(|path| FilePattern::from_user(path, project_root))
                        .collect()
                })
                .unwrap_or_else(|| DEFAULT_EXCLUDE.clone()),
            extend_exclude: config
                .extend_exclude
                .iter()
                .map(|path| FilePattern::from_user(path, project_root))
                .collect(),
            extend_ignore: config.extend_ignore,
            select: config
                .select
                .unwrap_or_else(|| vec![CheckCodePrefix::E, CheckCodePrefix::F]),
            extend_select: config.extend_select,
            ignore: config.ignore,
            line_length: config.line_length.unwrap_or(88),
            per_file_ignores: config
                .per_file_ignores
                .into_iter()
                .map(|pair| PerFileIgnore::new(pair, project_root))
                .collect(),
            // Plugins
            flake8_quotes: config
                .flake8_quotes
                .map(flake8_quotes::settings::Settings::from_config)
                .unwrap_or_default(),
        })
    }
}

#[derive(Debug)]
pub struct Settings {
    pub dummy_variable_rgx: Regex,
    pub enabled: BTreeSet<CheckCode>,
    pub exclude: Vec<FilePattern>,
    pub extend_exclude: Vec<FilePattern>,
    pub line_length: usize,
    pub per_file_ignores: Vec<PerFileIgnore>,
    pub target_version: PythonVersion,
    // Plugins
    pub flake8_quotes: flake8_quotes::settings::Settings,
}

/// Given a set of selected and ignored prefixes, resolve the set of enabled error codes.
fn resolve_codes(
    select: &[CheckCodePrefix],
    extend_select: &[CheckCodePrefix],
    ignore: &[CheckCodePrefix],
    extend_ignore: &[CheckCodePrefix],
) -> BTreeSet<CheckCode> {
    let mut codes: BTreeSet<CheckCode> = BTreeSet::new();
    for specificity in [
        PrefixSpecificity::Category,
        PrefixSpecificity::Hundreds,
        PrefixSpecificity::Tens,
        PrefixSpecificity::Explicit,
    ] {
        for prefix in select {
            if prefix.specificity() == specificity {
                codes.extend(prefix.codes());
            }
        }
        for prefix in extend_select {
            if prefix.specificity() == specificity {
                codes.extend(prefix.codes());
            }
        }
        for prefix in ignore {
            if prefix.specificity() == specificity {
                for code in prefix.codes() {
                    codes.remove(&code);
                }
            }
        }
        for prefix in extend_ignore {
            if prefix.specificity() == specificity {
                for code in prefix.codes() {
                    codes.remove(&code);
                }
            }
        }
    }
    codes
}

impl Settings {
    pub fn from_raw(settings: RawSettings) -> Self {
        Self {
            dummy_variable_rgx: settings.dummy_variable_rgx,
            enabled: resolve_codes(
                &settings.select,
                &settings.extend_select,
                &settings.ignore,
                &settings.extend_ignore,
            ),
            exclude: settings.exclude,
            extend_exclude: settings.extend_exclude,
            flake8_quotes: settings.flake8_quotes,
            line_length: settings.line_length,
            per_file_ignores: settings.per_file_ignores,
            target_version: settings.target_version,
        }
    }

    pub fn for_rule(check_code: CheckCode) -> Self {
        Self {
            dummy_variable_rgx: DEFAULT_DUMMY_VARIABLE_RGX.clone(),
            enabled: BTreeSet::from([check_code]),
            exclude: vec![],
            extend_exclude: vec![],
            line_length: 88,
            per_file_ignores: vec![],
            target_version: PythonVersion::Py310,
            flake8_quotes: Default::default(),
        }
    }

    pub fn for_rules(check_codes: Vec<CheckCode>) -> Self {
        Self {
            dummy_variable_rgx: DEFAULT_DUMMY_VARIABLE_RGX.clone(),
            enabled: BTreeSet::from_iter(check_codes),
            exclude: vec![],
            extend_exclude: vec![],
            line_length: 88,
            per_file_ignores: vec![],
            target_version: PythonVersion::Py310,
            flake8_quotes: Default::default(),
        }
    }
}

impl Hash for Settings {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.line_length.hash(state);
        self.dummy_variable_rgx.as_str().hash(state);
        for value in self.enabled.iter() {
            value.hash(state);
        }
        for value in self.per_file_ignores.iter() {
            value.hash(state);
        }
    }
}

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

/// Struct to render user-facing Settings.
#[derive(Debug)]
pub struct CurrentSettings {
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

impl CurrentSettings {
    pub fn from_settings(
        settings: RawSettings,
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
