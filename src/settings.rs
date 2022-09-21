use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use glob::Pattern;
use once_cell::sync::Lazy;

use crate::checks::{CheckCode, ALL_CHECK_CODES};
use crate::fs;
use crate::pyproject::load_config;

#[derive(Debug, Clone)]
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

#[derive(Debug)]
pub struct Settings {
    pub pyproject: Option<PathBuf>,
    pub project_root: Option<PathBuf>,
    pub line_length: usize,
    pub exclude: Vec<FilePattern>,
    pub extend_exclude: Vec<FilePattern>,
    pub select: BTreeSet<CheckCode>,
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
        }
    }
}

impl Hash for Settings {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.line_length.hash(state);
        for value in self.select.iter() {
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

impl Settings {
    pub fn from_pyproject(pyproject: Option<PathBuf>, project_root: Option<PathBuf>) -> Self {
        let config = load_config(&pyproject);
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
                BTreeSet::from_iter(ALL_CHECK_CODES)
            },
            pyproject,
            project_root,
        };
        if let Some(ignore) = &config.ignore {
            settings.ignore(ignore);
        }
        settings
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
