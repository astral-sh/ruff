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
        let mut settings = Settings {
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
            select: BTreeSet::from_iter(config.select.unwrap_or_else(|| {
                vec![
                    CheckCode::E402,
                    CheckCode::E501,
                    CheckCode::E711,
                    CheckCode::E712,
                    CheckCode::E713,
                    CheckCode::E714,
                    CheckCode::E731,
                    CheckCode::E902,
                    CheckCode::F401,
                    CheckCode::F403,
                    CheckCode::F541,
                    CheckCode::F601,
                    CheckCode::F602,
                    CheckCode::F621,
                    CheckCode::F622,
                    CheckCode::F631,
                    CheckCode::F634,
                    CheckCode::F704,
                    CheckCode::F706,
                    CheckCode::F707,
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
        Ok(settings)
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

    pub fn exclude(&mut self, exclude: Vec<Pattern>) {
        self.exclude = exclude;
    }
}
