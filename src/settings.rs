use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use glob::Pattern;
use once_cell::sync::Lazy;

use crate::checks::{CheckCode, ALL_CHECK_CODES};
use crate::fs;
use crate::pyproject::load_config;

#[derive(Debug, Clone)]
pub struct FilePattern {
    pub basename: Pattern,
    pub absolute: Option<Pattern>,
    pub directory_only: bool,
}

impl FilePattern {
    pub fn single(pattern: &str) -> Self {
        FilePattern {
            basename: Pattern::new(pattern).unwrap(),
            absolute: None,
            directory_only: true,
        }
    }

    pub fn user_provided(pattern: &str) -> Self {
        FilePattern {
            basename: Pattern::new(pattern).expect("Invalid pattern."),
            absolute: Some(
                Pattern::new(&fs::normalize_path(Path::new(pattern)).to_string_lossy())
                    .expect("Invalid pattern."),
            ),
            directory_only: false,
        }
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
        FilePattern::single(".bzr"),
        FilePattern::single(".direnv"),
        FilePattern::single(".eggs"),
        FilePattern::single(".git"),
        FilePattern::single(".hg"),
        FilePattern::single(".mypy_cache"),
        FilePattern::single(".nox"),
        FilePattern::single(".pants.d"),
        FilePattern::single(".ruff_cache"),
        FilePattern::single(".svn"),
        FilePattern::single(".tox"),
        FilePattern::single(".venv"),
        FilePattern::single("__pypackages__"),
        FilePattern::single("_build"),
        FilePattern::single("buck-out"),
        FilePattern::single("build"),
        FilePattern::single("dist"),
        FilePattern::single("node_modules"),
        FilePattern::single("venv"),
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
                        .map(|path| FilePattern::user_provided(path))
                        .collect()
                })
                .unwrap_or_else(|| DEFAULT_EXCLUDE.clone()),
            extend_exclude: config
                .extend_exclude
                .map(|paths| {
                    paths
                        .iter()
                        .map(|path| FilePattern::user_provided(path))
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
