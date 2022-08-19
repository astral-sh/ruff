use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::pyproject::load_config;

pub struct Settings {
    pub line_length: usize,
    pub exclude: Vec<PathBuf>,
}

static DEFAULT_MAX_LINE_LENGTH: usize = 88;

impl Settings {
    pub fn from_paths<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Result<Self> {
        let (project_root, config) = load_config(paths)?;

        Ok(Settings {
            line_length: config.line_length.unwrap_or(DEFAULT_MAX_LINE_LENGTH),
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
        })
    }
}
