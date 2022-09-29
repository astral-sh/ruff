use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use glob::Pattern;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::checks::{CheckCode, DEFAULT_CHECK_CODES};
use crate::fs;
use crate::pyproject::{load_config, StrCheckCodePair};

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
    pub code: CheckCode,
}

impl PerFileIgnore {
    pub fn new(user_in: StrCheckCodePair, project_root: &Option<PathBuf>) -> Self {
        let pattern = FilePattern::from_user(user_in.pattern.as_str(), project_root);
        let code = user_in.code;
        Self { pattern, code }
    }
}

#[derive(Debug)]
pub struct Settings {
    pub pyproject: Option<PathBuf>,
    pub project_root: Option<PathBuf>,
    pub line_length: usize,
    pub exclude: Vec<FilePattern>,
    pub extend_exclude: Vec<FilePattern>,
    pub select: BTreeSet<CheckCode>,
    pub per_file_ignores: Vec<PerFileIgnore>,
    pub dummy_variable_rgx: Regex,
}

impl Settings {
    pub fn for_rule(check_code: CheckCode) -> Self {
        Self {
            pyproject: None,
            project_root: None,
            line_length: 88,
            exclude: vec![],
            extend_exclude: vec![],
            select: BTreeSet::from([check_code]),
            per_file_ignores: vec![],
            dummy_variable_rgx: DEFAULT_DUMMY_VARIABLE_RGX.clone(),
        }
    }

    pub fn for_rules(check_codes: Vec<CheckCode>) -> Self {
        Self {
            pyproject: None,
            project_root: None,
            line_length: 88,
            exclude: vec![],
            extend_exclude: vec![],
            select: BTreeSet::from_iter(check_codes),
            per_file_ignores: vec![],
            dummy_variable_rgx: DEFAULT_DUMMY_VARIABLE_RGX.clone(),
        }
    }
}

impl Hash for Settings {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.line_length.hash(state);
        self.dummy_variable_rgx.as_str().hash(state);
        for value in self.select.iter() {
            value.hash(state);
        }
        for value in self.per_file_ignores.iter() {
            value.hash(state);
        }
    }
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

impl Settings {
    pub fn from_pyproject(
        pyproject: Option<PathBuf>,
        project_root: Option<PathBuf>,
    ) -> Result<Self> {
        let config = load_config(&pyproject)?;
        let mut settings = Settings {
            line_length: config.line_length.unwrap_or(88),
            exclude: config
                .exclude
                .map(|paths| {
                    paths
                        .iter()
                        .map(|path| FilePattern::from_user(path, &project_root))
                        .collect()
                })
                .unwrap_or_else(|| DEFAULT_EXCLUDE.clone()),
            extend_exclude: config
                .extend_exclude
                .map(|paths| {
                    paths
                        .iter()
                        .map(|path| FilePattern::from_user(path, &project_root))
                        .collect()
                })
                .unwrap_or_default(),
            select: if let Some(select) = config.select {
                BTreeSet::from_iter(select)
            } else {
                BTreeSet::from_iter(DEFAULT_CHECK_CODES)
            },
            per_file_ignores: config
                .per_file_ignores
                .map(|ignore_strings| {
                    ignore_strings
                        .into_iter()
                        .map(|pair| PerFileIgnore::new(pair, &project_root))
                        .collect()
                })
                .unwrap_or_default(),
            dummy_variable_rgx: match config.dummy_variable_rgx {
                Some(pattern) => Regex::new(&pattern)
                    .map_err(|e| anyhow!("Invalid dummy-variable-rgx value: {e}"))?,
                None => DEFAULT_DUMMY_VARIABLE_RGX.clone(),
            },
            pyproject,
            project_root,
        };
        if let Some(ignore) = &config.ignore {
            settings.ignore(ignore);
        }
        Ok(settings)
    }

    pub fn clear(&mut self) {
        self.select.clear();
    }

    pub fn select(&mut self, codes: Vec<CheckCode>) {
        for code in codes {
            self.select.insert(code);
        }
    }

    pub fn ignore(&mut self, codes: &[CheckCode]) {
        for code in codes {
            self.select.remove(code);
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
    pub pyproject: Option<PathBuf>,
    pub project_root: Option<PathBuf>,
    pub line_length: usize,
    pub exclude: Vec<Exclusion>,
    pub extend_exclude: Vec<Exclusion>,
    pub select: BTreeSet<CheckCode>,
    pub per_file_ignores: Vec<PerFileIgnore>,
    pub dummy_variable_rgx: Regex,
}

impl CurrentSettings {
    pub fn from_settings(settings: Settings) -> Self {
        Self {
            pyproject: settings.pyproject,
            project_root: settings.project_root,
            line_length: settings.line_length,
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
            select: settings.select,
            per_file_ignores: settings.per_file_ignores,
            dummy_variable_rgx: settings.dummy_variable_rgx,
        }
    }
}
