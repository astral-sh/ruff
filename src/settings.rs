use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use glob::Pattern;
use once_cell::sync::Lazy;

use crate::checks::CheckCode;
use crate::pyproject::load_config;

#[derive(Debug)]
pub struct Settings {
    pub line_length: usize,
    pub exclude: Vec<Pattern>,
    pub extend_exclude: Vec<Pattern>,
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

static DEFAULT_EXCLUDE: Lazy<Vec<Pattern>> = Lazy::new(|| {
    vec![
        Pattern::new(".bzr").unwrap(),
        Pattern::new(".direnv").unwrap(),
        Pattern::new(".eggs").unwrap(),
        Pattern::new(".git").unwrap(),
        Pattern::new(".hg").unwrap(),
        Pattern::new(".mypy_cache").unwrap(),
        Pattern::new(".nox").unwrap(),
        Pattern::new(".pants.d").unwrap(),
        Pattern::new(".ruff_cache").unwrap(),
        Pattern::new(".svn").unwrap(),
        Pattern::new(".tox").unwrap(),
        Pattern::new(".venv").unwrap(),
        Pattern::new("__pypackages__").unwrap(),
        Pattern::new("_build").unwrap(),
        Pattern::new("buck-out").unwrap(),
        Pattern::new("build").unwrap(),
        Pattern::new("dist").unwrap(),
        Pattern::new("node_modules").unwrap(),
        Pattern::new("venv").unwrap(),
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
                        .map(|path| {
                            Pattern::new(&path.to_string_lossy()).expect("Invalid pattern.")
                        })
                        .collect()
                })
                .unwrap_or_else(|| DEFAULT_EXCLUDE.clone()),
            extend_exclude: config
                .extend_exclude
                .map(|paths| {
                    paths
                        .iter()
                        .map(|path| {
                            Pattern::new(&path.to_string_lossy()).expect("Invalid pattern.")
                        })
                        .collect()
                })
                .unwrap_or_default(),
            select: BTreeSet::from_iter(config.select.unwrap_or_else(|| {
                vec![
                    CheckCode::E402,
                    CheckCode::E501,
                    CheckCode::E711,
                    CheckCode::E712,
                    CheckCode::E713,
                    CheckCode::E714,
                    CheckCode::E721,
                    CheckCode::E722,
                    CheckCode::E731,
                    CheckCode::E741,
                    CheckCode::E742,
                    CheckCode::E743,
                    CheckCode::E902,
                    CheckCode::E999,
                    CheckCode::F401,
                    CheckCode::F403,
                    CheckCode::F406,
                    CheckCode::F407,
                    CheckCode::F541,
                    CheckCode::F601,
                    CheckCode::F602,
                    CheckCode::F621,
                    CheckCode::F622,
                    CheckCode::F631,
                    CheckCode::F632,
                    CheckCode::F633,
                    CheckCode::F634,
                    CheckCode::F701,
                    CheckCode::F702,
                    CheckCode::F704,
                    CheckCode::F706,
                    CheckCode::F707,
                    CheckCode::F722,
                    CheckCode::F821,
                    CheckCode::F822,
                    CheckCode::F823,
                    CheckCode::F831,
                    CheckCode::F841,
                    CheckCode::F901,
                    // Disable refactoring codes by default.
                    // CheckCode::R001,
                    // CheckCode::R002,
                ]
            })),
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
