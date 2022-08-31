use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::path::Path;

use anyhow::Result;
use glob::Pattern;

use crate::checks::CheckCode;
use crate::pyproject::load_config;

#[derive(Debug)]
pub struct Settings {
    pub line_length: usize,
    pub exclude: Vec<Pattern>,
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
                .map(|path| Pattern::new(&path.to_string_lossy()).expect("Invalid pattern."))
                .collect(),
            select: config.select.unwrap_or_else(|| {
                BTreeSet::from([
                    CheckCode::E501,
                    CheckCode::F401,
                    CheckCode::F403,
                    CheckCode::F541,
                    CheckCode::F634,
                    CheckCode::F706,
                    CheckCode::F831,
                    CheckCode::F832,
                    CheckCode::F901,
                ])
            }),
        })
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
