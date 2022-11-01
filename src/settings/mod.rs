//! Effective program settings, taking into account pyproject.toml and command-line options.
//! Structure is optimized for internal usage, as opposed to external visibility or parsing.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

use regex::Regex;

use crate::checks::CheckCode;
use crate::checks_gen::{CheckCodePrefix, PrefixSpecificity};
use crate::settings::configuration::Configuration;
use crate::settings::types::{FilePattern, PerFileIgnore, PythonVersion};
use crate::{flake8_quotes, pep8_naming};

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
    pub pep8_naming: pep8_naming::settings::Settings,
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
            pep8_naming: config.pep8_naming,
            per_file_ignores: resolve_per_file_ignores(&config.per_file_ignores),
            target_version: config.target_version,
        }
    }

    pub fn for_rule(check_code: CheckCode) -> Self {
        Self {
            dummy_variable_rgx: Regex::new("^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$").unwrap(),
            enabled: BTreeSet::from([check_code]),
            exclude: Default::default(),
            extend_exclude: Default::default(),
            line_length: 88,
            per_file_ignores: Default::default(),
            target_version: PythonVersion::Py310,
            flake8_quotes: Default::default(),
            pep8_naming: Default::default(),
        }
    }

    pub fn for_rules(check_codes: Vec<CheckCode>) -> Self {
        Self {
            dummy_variable_rgx: Regex::new("^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$").unwrap(),
            enabled: BTreeSet::from_iter(check_codes),
            exclude: Default::default(),
            extend_exclude: Default::default(),
            line_length: 88,
            per_file_ignores: Default::default(),
            target_version: PythonVersion::Py310,
            flake8_quotes: Default::default(),
            pep8_naming: Default::default(),
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

fn resolve_per_file_ignores(
    per_file_ignores: &BTreeMap<String, Vec<CheckCodePrefix>>,
) -> Vec<PerFileIgnore> {
    per_file_ignores
        .iter()
        .map(|(pattern, prefixes)| PerFileIgnore::new(pattern, prefixes, &None))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::checks::CheckCode;
    use crate::checks_gen::CheckCodePrefix;
    use crate::settings::resolve_codes;

    #[test]
    fn resolver() {
        let actual = resolve_codes(&[CheckCodePrefix::W], &[], &[], &[]);
        let expected = BTreeSet::from_iter([CheckCode::W292, CheckCode::W605]);
        assert_eq!(actual, expected);

        let actual = resolve_codes(&[CheckCodePrefix::W6], &[], &[], &[]);
        let expected = BTreeSet::from_iter([CheckCode::W605]);
        assert_eq!(actual, expected);

        let actual = resolve_codes(&[CheckCodePrefix::W], &[], &[CheckCodePrefix::W292], &[]);
        let expected = BTreeSet::from_iter([CheckCode::W605]);
        assert_eq!(actual, expected);

        let actual = resolve_codes(&[CheckCodePrefix::W605], &[], &[CheckCodePrefix::W605], &[]);
        let expected = BTreeSet::from_iter([]);
        assert_eq!(actual, expected);
    }
}
