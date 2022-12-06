//! Effective program settings, taking into account pyproject.toml and
//! command-line options. Structure is optimized for internal usage, as opposed
//! to external visibility or parsing.

use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use anyhow::Result;
use globset::{Glob, GlobMatcher, GlobSet};
use path_absolutize::path_dedot;
use regex::Regex;
use rustc_hash::FxHashSet;

use crate::checks::CheckCode;
use crate::checks_gen::{CheckCodePrefix, SuffixLength};
use crate::settings::configuration::Configuration;
use crate::settings::types::{FilePattern, PerFileIgnore, PythonVersion, SerializationFormat};
use crate::{
    flake8_annotations, flake8_bugbear, flake8_import_conventions, flake8_quotes,
    flake8_tidy_imports, fs, isort, mccabe, pep8_naming, pyupgrade,
};

pub mod configuration;
pub mod options;
pub mod options_base;
pub mod pyproject;
pub mod types;

#[derive(Debug)]
pub struct Settings {
    pub allowed_confusables: FxHashSet<char>,
    pub dummy_variable_rgx: Regex,
    pub enabled: FxHashSet<CheckCode>,
    pub exclude: GlobSet,
    pub extend_exclude: GlobSet,
    pub external: BTreeSet<String>,
    pub fixable: FxHashSet<CheckCode>,
    pub format: SerializationFormat,
    pub ignore_init_module_imports: bool,
    pub line_length: usize,
    pub per_file_ignores: Vec<(GlobMatcher, GlobMatcher, BTreeSet<CheckCode>)>,
    pub show_source: bool,
    pub src: Vec<PathBuf>,
    pub target_version: PythonVersion,
    // Plugins
    pub flake8_annotations: flake8_annotations::settings::Settings,
    pub flake8_bugbear: flake8_bugbear::settings::Settings,
    pub flake8_import_conventions: flake8_import_conventions::settings::Settings,
    pub flake8_quotes: flake8_quotes::settings::Settings,
    pub flake8_tidy_imports: flake8_tidy_imports::settings::Settings,
    pub isort: isort::settings::Settings,
    pub mccabe: mccabe::settings::Settings,
    pub pep8_naming: pep8_naming::settings::Settings,
    pub pyupgrade: pyupgrade::settings::Settings,
}

impl Settings {
    pub fn from_configuration(
        config: Configuration,
        project_root: Option<&PathBuf>,
    ) -> Result<Self> {
        Ok(Self {
            allowed_confusables: config.allowed_confusables,
            dummy_variable_rgx: config.dummy_variable_rgx,
            enabled: resolve_codes(
                &config
                    .select
                    .into_iter()
                    .chain(config.extend_select.into_iter())
                    .collect::<Vec<_>>(),
                &config
                    .ignore
                    .into_iter()
                    .chain(config.extend_ignore.into_iter())
                    .collect::<Vec<_>>(),
            ),
            exclude: resolve_globset(config.exclude, project_root)?,
            extend_exclude: resolve_globset(config.extend_exclude, project_root)?,
            external: BTreeSet::from_iter(config.external),
            fixable: resolve_codes(&config.fixable, &config.unfixable),
            format: config.format,
            flake8_annotations: config.flake8_annotations,
            flake8_bugbear: config.flake8_bugbear,
            flake8_import_conventions: config.flake8_import_conventions,
            flake8_quotes: config.flake8_quotes,
            flake8_tidy_imports: config.flake8_tidy_imports,
            ignore_init_module_imports: config.ignore_init_module_imports,
            isort: config.isort,
            mccabe: config.mccabe,
            line_length: config.line_length,
            pep8_naming: config.pep8_naming,
            pyupgrade: config.pyupgrade,
            per_file_ignores: resolve_per_file_ignores(config.per_file_ignores, project_root)?,
            src: config.src,
            target_version: config.target_version,
            show_source: config.show_source,
        })
    }

    pub fn for_rule(check_code: CheckCode) -> Self {
        Self {
            allowed_confusables: FxHashSet::from_iter([]),
            dummy_variable_rgx: Regex::new("^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$").unwrap(),
            enabled: FxHashSet::from_iter([check_code.clone()]),
            exclude: GlobSet::empty(),
            extend_exclude: GlobSet::empty(),
            external: BTreeSet::default(),
            fixable: FxHashSet::from_iter([check_code]),
            format: SerializationFormat::Text,
            ignore_init_module_imports: false,
            line_length: 88,
            per_file_ignores: vec![],
            show_source: false,
            src: vec![path_dedot::CWD.clone()],
            target_version: PythonVersion::Py310,
            flake8_annotations: flake8_annotations::settings::Settings::default(),
            flake8_bugbear: flake8_bugbear::settings::Settings::default(),
            flake8_import_conventions: flake8_import_conventions::settings::Settings::default(),
            flake8_quotes: flake8_quotes::settings::Settings::default(),
            flake8_tidy_imports: flake8_tidy_imports::settings::Settings::default(),
            isort: isort::settings::Settings::default(),
            mccabe: mccabe::settings::Settings::default(),
            pep8_naming: pep8_naming::settings::Settings::default(),
            pyupgrade: pyupgrade::settings::Settings::default(),
        }
    }

    pub fn for_rules(check_codes: Vec<CheckCode>) -> Self {
        Self {
            allowed_confusables: FxHashSet::from_iter([]),
            dummy_variable_rgx: Regex::new("^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$").unwrap(),
            enabled: FxHashSet::from_iter(check_codes.clone()),
            exclude: GlobSet::empty(),
            extend_exclude: GlobSet::empty(),
            external: BTreeSet::default(),
            fixable: FxHashSet::from_iter(check_codes),
            format: SerializationFormat::Text,
            ignore_init_module_imports: false,
            line_length: 88,
            per_file_ignores: vec![],
            show_source: false,
            src: vec![path_dedot::CWD.clone()],
            target_version: PythonVersion::Py310,
            flake8_annotations: flake8_annotations::settings::Settings::default(),
            flake8_bugbear: flake8_bugbear::settings::Settings::default(),
            flake8_import_conventions: flake8_import_conventions::settings::Settings::default(),
            flake8_quotes: flake8_quotes::settings::Settings::default(),
            flake8_tidy_imports: flake8_tidy_imports::settings::Settings::default(),
            isort: isort::settings::Settings::default(),
            mccabe: mccabe::settings::Settings::default(),
            pep8_naming: pep8_naming::settings::Settings::default(),
            pyupgrade: pyupgrade::settings::Settings::default(),
        }
    }
}

impl Hash for Settings {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Add base properties in alphabetical order.
        for confusable in &self.allowed_confusables {
            confusable.hash(state);
        }
        self.dummy_variable_rgx.as_str().hash(state);
        for value in &self.enabled {
            value.hash(state);
        }
        self.external.hash(state);
        for value in &self.fixable {
            value.hash(state);
        }
        self.line_length.hash(state);
        for (absolute, basename, codes) in &self.per_file_ignores {
            absolute.glob().hash(state);
            basename.glob().hash(state);
            codes.hash(state);
        }
        self.show_source.hash(state);
        self.target_version.hash(state);
        // Add plugin properties in alphabetical order.
        self.flake8_annotations.hash(state);
        self.flake8_bugbear.hash(state);
        self.flake8_import_conventions.hash(state);
        self.flake8_quotes.hash(state);
        self.flake8_tidy_imports.hash(state);
        self.isort.hash(state);
        self.mccabe.hash(state);
        self.pep8_naming.hash(state);
        self.pyupgrade.hash(state);
    }
}

/// Given a list of patterns, create a `GlobSet`.
pub fn resolve_globset(
    patterns: Vec<FilePattern>,
    project_root: Option<&PathBuf>,
) -> Result<GlobSet> {
    let mut builder = globset::GlobSetBuilder::new();
    for pattern in patterns {
        pattern.add_to(&mut builder, project_root)?;
    }
    builder.build().map_err(std::convert::Into::into)
}

/// Given a list of patterns, create a `GlobSet`.
pub fn resolve_per_file_ignores(
    per_file_ignores: Vec<PerFileIgnore>,
    project_root: Option<&PathBuf>,
) -> Result<Vec<(GlobMatcher, GlobMatcher, BTreeSet<CheckCode>)>> {
    per_file_ignores
        .into_iter()
        .map(|per_file_ignore| {
            // Construct absolute path matcher.
            let path = Path::new(&per_file_ignore.pattern);
            let absolute_path = match project_root {
                Some(project_root) => fs::normalize_path_to(path, project_root),
                None => fs::normalize_path(path),
            };
            let absolute = Glob::new(&absolute_path.to_string_lossy())?.compile_matcher();

            // Construct basename matcher.
            let basename = Glob::new(&per_file_ignore.pattern)?.compile_matcher();

            Ok((absolute, basename, per_file_ignore.codes))
        })
        .collect()
}

/// Given a set of selected and ignored prefixes, resolve the set of enabled
/// error codes.
fn resolve_codes(select: &[CheckCodePrefix], ignore: &[CheckCodePrefix]) -> FxHashSet<CheckCode> {
    let mut codes: FxHashSet<CheckCode> = FxHashSet::default();
    for specificity in [
        SuffixLength::Zero,
        SuffixLength::One,
        SuffixLength::Two,
        SuffixLength::Three,
        SuffixLength::Four,
    ] {
        for prefix in select {
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
    }
    codes
}

#[cfg(test)]
mod tests {
    use rustc_hash::FxHashSet;

    use crate::checks::CheckCode;
    use crate::checks_gen::CheckCodePrefix;
    use crate::settings::resolve_codes;

    #[test]
    fn resolver() {
        let actual = resolve_codes(&[CheckCodePrefix::W], &[]);
        let expected = FxHashSet::from_iter([CheckCode::W292, CheckCode::W605]);
        assert_eq!(actual, expected);

        let actual = resolve_codes(&[CheckCodePrefix::W6], &[]);
        let expected = FxHashSet::from_iter([CheckCode::W605]);
        assert_eq!(actual, expected);

        let actual = resolve_codes(&[CheckCodePrefix::W], &[CheckCodePrefix::W292]);
        let expected = FxHashSet::from_iter([CheckCode::W605]);
        assert_eq!(actual, expected);

        let actual = resolve_codes(&[CheckCodePrefix::W605], &[CheckCodePrefix::W605]);
        let expected = FxHashSet::from_iter([]);
        assert_eq!(actual, expected);
    }
}
