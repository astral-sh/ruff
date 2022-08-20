use std::path::Path;

use anyhow::Result;

use crate::pyproject::{load_config, Config};

pub struct Settings {
    pub line_length: usize,
}

static DEFAULT_MAX_LINE_LENGTH: usize = 88;

impl From<Config> for Settings {
    fn from(config: Config) -> Settings {
        Settings {
            line_length: config.line_length.unwrap_or(DEFAULT_MAX_LINE_LENGTH),
        }
    }
}

impl Settings {
    pub fn from_paths<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Result<Self> {
        load_config(paths).map(|config| config.into())
    }
}
