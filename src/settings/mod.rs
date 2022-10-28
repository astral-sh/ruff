//! Effective program settings, taking into account pyproject.toml and command-line options.
//! Structure is optimized for internal usage, as opposed to external visibility or parsing.

use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};

use regex::Regex;

use crate::checks::CheckCode;
use crate::checks_gen::{CheckCodePrefix, PrefixSpecificity};
use crate::flake8_quotes;
use crate::settings::configuration::Configuration;
use crate::settings::types::{FilePattern, PerFileIgnore, PythonVersion};

pub mod configuration;
pub mod options;
pub mod pyproject;
pub mod types;
pub mod user;

#[derive(Debug)]
pub struct Settings {
    pub dummy_variable_rgx: Regex,
    pub enabled: BTreeSet<CheckCode>,
    pub exclude: Vec<FilePattern>,
    pub extend_exclude: Vec<FilePattern>,
    pub line_length: usize,
    pub per_file_ignores: Vec<PerFileIgnore>,
    pub target_version: PythonVersion,
    // Plugins
    pub flake8_quotes: flake8_quotes::settings::Settings,
}

impl Settings {
    pub fn from_configuration(config: Configuration) -> Self {
        Self {
            dummy_variable_rgx: config.dummy_variable_rgx,
            enabled: resolve_codes(
                &config.select,
                &config.extend_select,
                &config.ignore,
                &config.extend_ignore,
            ),
            exclude: config.exclude,
            extend_exclude: config.extend_exclude,
            flake8_quotes: config.flake8_quotes,
            line_length: config.line_length,
            per_file_ignores: config.per_file_ignores,
            target_version: config.target_version,
        }
    }

    pub fn for_rule(check_code: CheckCode) -> Self {
        Self {
            dummy_variable_rgx: Regex::new("^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$").unwrap(),
            enabled: BTreeSet::from([check_code]),
            exclude: vec![],
            extend_exclude: vec![],
            line_length: 88,
            per_file_ignores: vec![],
            target_version: PythonVersion::Py310,
            flake8_quotes: Default::default(),
        }
    }

    pub fn for_rules(check_codes: Vec<CheckCode>) -> Self {
        Self {
            dummy_variable_rgx: Regex::new("^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$").unwrap(),
            enabled: BTreeSet::from_iter(check_codes),
            exclude: vec![],
            extend_exclude: vec![],
            line_length: 88,
            per_file_ignores: vec![],
            target_version: PythonVersion::Py310,
            flake8_quotes: Default::default(),
        }
    }
}

impl Hash for Settings {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.line_length.hash(state);
        self.dummy_variable_rgx.as_str().hash(state);
        for value in self.enabled.iter() {
            value.hash(state);
        }
        for value in self.per_file_ignores.iter() {
            value.hash(state);
        }
    }
}

/// Given a set of selected and ignored prefixes, resolve the set of enabled error codes.
fn resolve_codes(
    select: &[CheckCodePrefix],
    extend_select: &[CheckCodePrefix],
    ignore: &[CheckCodePrefix],
    extend_ignore: &[CheckCodePrefix],
) -> BTreeSet<CheckCode> {
    let mut codes: BTreeSet<CheckCode> = BTreeSet::new();
    for specificity in [
        PrefixSpecificity::Category,
        PrefixSpecificity::Hundreds,
        PrefixSpecificity::Tens,
        PrefixSpecificity::Explicit,
    ] {
        for prefix in select {
            if prefix.specificity() == specificity {
                codes.extend(prefix.codes());
            }
        }
        for prefix in extend_select {
            if prefix.specificity() == specificity {
                codes.extend(prefix.codes());
            }
        }
        for prefix in ignore {
            if prefix.specificity() == specificity {
                for code in prefix.codes() {
                    codes.remove(&code);
                }
            }
        }
        for prefix in extend_ignore {
            if prefix.specificity() == specificity {
                for code in prefix.codes() {
                    codes.remove(&code);
                }
            }
        }
    }
    codes
}
