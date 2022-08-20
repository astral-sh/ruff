use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::checks::CheckCode;
use anyhow::Result;

use crate::pyproject::load_config;

pub struct Settings {
    pub line_length: usize,
    pub exclude: Vec<PathBuf>,
    pub select: HashSet<CheckCode>,
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
                HashSet::from([
                    CheckCode::F831,
                    CheckCode::F541,
                    CheckCode::F634,
                    CheckCode::F403,
                    CheckCode::E501,
                ])
            }),
        })
    }
}
