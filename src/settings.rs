use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use glob::Pattern;
use once_cell::sync::Lazy;

use crate::checks::{CheckCode, ALL_CHECK_CODES};
use crate::fs;
use crate::pyproject::load_config;

pub struct SimplePattern {}

#[derive(Debug, Clone)]
pub enum FilePattern {
    Simple(&'static str),
    Complex(
        Option<String>,
        Option<Pattern>,
        Option<String>,
        Option<Pattern>,
    ),
}

impl FilePattern {
    pub fn from_user(pattern: &str) -> Self {
        // STOPSHIP: Do this in one pass.
        let is_glob = pattern.contains('*')
            || pattern.contains('?')
            || pattern.contains('[')
            || pattern.contains(']');
        let has_segments = pattern.contains(std::path::MAIN_SEPARATOR);

        let basename = if !has_segments && !is_glob {
            Some(pattern.to_string())
        } else {
            None
        };
        let basename_glob = if !has_segments && is_glob {
            Some(Pattern::new(pattern).expect("Invalid pattern."))
        } else {
            None
        };
        let absolute = if !is_glob {
            Some(
                fs::normalize_path(Path::new(pattern))
                    .to_string_lossy()
                    .to_string(),
            )
        } else {
            None
        };
        let absolute_glob = if is_glob {
            Some(
                Pattern::new(&fs::normalize_path(Path::new(pattern)).to_string_lossy())
                    .expect("Invalid pattern."),
            )
        } else {
            None
        };

        FilePattern::Complex(basename, basename_glob, absolute, absolute_glob)
    }
}

#[derive(Debug)]
pub struct Settings {
    pub line_length: usize,
    pub exclude: Vec<FilePattern>,
    pub extend_exclude: Vec<FilePattern>,
    pub select: BTreeSet<CheckCode>,
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
    pub fn from_paths(paths: &[PathBuf]) -> Self {
        let config = load_config(paths);
        let mut settings = Settings {
            line_length: config.line_length.unwrap_or(88),
            exclude: config
                .exclude
                .map(|paths| {
                    paths
                        .iter()
                        .map(|path| FilePattern::from_user(path))
                        .collect()
                })
                .unwrap_or_else(|| DEFAULT_EXCLUDE.clone()),
            extend_exclude: config
                .extend_exclude
                .map(|paths| {
                    paths
                        .iter()
                        .map(|path| FilePattern::from_user(path))
                        .collect()
                })
                .unwrap_or_default(),
            select: if let Some(select) = config.select {
                BTreeSet::from_iter(select)
            } else {
                BTreeSet::from_iter(ALL_CHECK_CODES)
            },
        };
        if let Some(ignore) = &config.ignore {
            settings.ignore(ignore);
        }
        settings
    }

    pub fn select(&mut self, codes: Vec<CheckCode>) {
        self.select.clear();
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
