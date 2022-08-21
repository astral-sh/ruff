use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::checks::CheckCode;
use crate::pyproject::load_config;

pub struct Settings {
    pub line_length: usize,
    pub exclude: Vec<PathBuf>,
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

impl Settings {
    pub fn from_paths<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Result<Self> {
        let (project_root, config) = load_config(paths)?;
        Ok(Settings {
            line_length: config.line_length.unwrap_or(88),
            exclude: config
                .exclude
                .unwrap_or_default()
                .into_iter()
                .map(|path| {
                    if path.is_relative() {
                        project_root.join(path)
                    } else {
                        path
                    }
                })
                .collect(),
            select: config.select.unwrap_or_else(|| {
                BTreeSet::from([
                    CheckCode::F831,
                    CheckCode::F541,
                    CheckCode::F634,
                    CheckCode::F403,
                    CheckCode::F706,
                    CheckCode::F901,
                    CheckCode::E501,
                ])
            }),
        })
    }
}
