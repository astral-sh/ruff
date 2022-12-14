//! Effective program settings, taking into account pyproject.toml and
//! command-line options. Structure is optimized for internal usage, as opposed
//! to external visibility or parsing.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use anyhow::Result;
use globset::{Glob, GlobMatcher, GlobSet};
use itertools::Itertools;
use once_cell::sync::Lazy;
use path_absolutize::path_dedot;
use regex::Regex;
use rustc_hash::FxHashSet;

use crate::checks::CheckCode;
use crate::checks_gen::{CheckCodePrefix, SuffixLength, CATEGORIES};
use crate::settings::configuration::Configuration;
use crate::settings::types::{FilePattern, PerFileIgnore, PythonVersion, SerializationFormat};
use crate::{
    flake8_annotations, flake8_bugbear, flake8_import_conventions, flake8_quotes,
    flake8_tidy_imports, isort, mccabe, pep8_naming, pyupgrade,
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
    pub external: FxHashSet<String>,
    pub fix: bool,
    pub fixable: FxHashSet<CheckCode>,
    pub format: SerializationFormat,
    pub ignore_init_module_imports: bool,
    pub line_length: usize,
    pub per_file_ignores: Vec<(GlobMatcher, GlobMatcher, FxHashSet<CheckCode>)>,
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

static DEFAULT_EXCLUDE: Lazy<Vec<FilePattern>> = Lazy::new(|| {
    vec![
        FilePattern::Builtin(".bzr"),
        FilePattern::Builtin(".direnv"),
        FilePattern::Builtin(".eggs"),
        FilePattern::Builtin(".git"),
        FilePattern::Builtin(".hg"),
        FilePattern::Builtin(".mypy_cache"),
        FilePattern::Builtin(".nox"),
        FilePattern::Builtin(".pants.d"),
        FilePattern::Builtin(".ruff_cache"),
        FilePattern::Builtin(".svn"),
        FilePattern::Builtin(".tox"),
        FilePattern::Builtin(".venv"),
        FilePattern::Builtin("__pypackages__"),
        FilePattern::Builtin("_build"),
        FilePattern::Builtin("buck-out"),
        FilePattern::Builtin("build"),
        FilePattern::Builtin("dist"),
        FilePattern::Builtin("node_modules"),
        FilePattern::Builtin("venv"),
    ]
});

static DEFAULT_DUMMY_VARIABLE_RGX: Lazy<Regex> =
    Lazy::new(|| Regex::new("^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$").unwrap());

impl Settings {
    pub fn from_configuration(config: Configuration, project_root: &Path) -> Result<Self> {
        Ok(Self {
            allowed_confusables: config
                .allowed_confusables
                .map(FxHashSet::from_iter)
                .unwrap_or_default(),
            dummy_variable_rgx: config
                .dummy_variable_rgx
                .unwrap_or_else(|| DEFAULT_DUMMY_VARIABLE_RGX.clone()),
            enabled: resolve_codes(
                &config
                    .select
                    .unwrap_or_else(|| vec![CheckCodePrefix::E, CheckCodePrefix::F])
                    .into_iter()
                    .chain(config.extend_select.unwrap_or_default().into_iter())
                    .collect::<Vec<_>>(),
                &config
                    .ignore
                    .unwrap_or_default()
                    .into_iter()
                    .chain(config.extend_ignore.unwrap_or_default().into_iter())
                    .collect::<Vec<_>>(),
            ),
            exclude: resolve_globset(config.exclude.unwrap_or_else(|| DEFAULT_EXCLUDE.clone()))?,
            extend_exclude: resolve_globset(config.extend_exclude.unwrap_or_default())?,
            external: FxHashSet::from_iter(config.external.unwrap_or_default()),
            fix: config.fix.unwrap_or(false),
            fixable: resolve_codes(
                &config.fixable.unwrap_or_else(|| CATEGORIES.to_vec()),
                &config.unfixable.unwrap_or_default(),
            ),
            format: config.format.unwrap_or(SerializationFormat::Text),
            ignore_init_module_imports: config.ignore_init_module_imports.unwrap_or_default(),
            line_length: config.line_length.unwrap_or(88),
            per_file_ignores: resolve_per_file_ignores(
                config.per_file_ignores.unwrap_or_default(),
            )?,
            src: config
                .src
                .unwrap_or_else(|| vec![project_root.to_path_buf()]),
            target_version: config.target_version.unwrap_or(PythonVersion::Py310),
            show_source: config.show_source.unwrap_or_default(),
            // Plugins
            flake8_annotations: config
                .flake8_annotations
                .map(flake8_annotations::settings::Settings::from_options)
                .unwrap_or_default(),
            flake8_bugbear: config
                .flake8_bugbear
                .map(flake8_bugbear::settings::Settings::from_options)
                .unwrap_or_default(),
            flake8_import_conventions: config
                .flake8_import_conventions
                .map(flake8_import_conventions::settings::Settings::from_options)
                .unwrap_or_default(),
            flake8_quotes: config
                .flake8_quotes
                .map(flake8_quotes::settings::Settings::from_options)
                .unwrap_or_default(),
            flake8_tidy_imports: config
                .flake8_tidy_imports
                .map(flake8_tidy_imports::settings::Settings::from_options)
                .unwrap_or_default(),
            isort: config
                .isort
                .map(isort::settings::Settings::from_options)
                .unwrap_or_default(),
            mccabe: config
                .mccabe
                .as_ref()
                .map(mccabe::settings::Settings::from_options)
                .unwrap_or_default(),
            pep8_naming: config
                .pep8_naming
                .map(pep8_naming::settings::Settings::from_options)
                .unwrap_or_default(),
            pyupgrade: config
                .pyupgrade
                .as_ref()
                .map(pyupgrade::settings::Settings::from_options)
                .unwrap_or_default(),
        })
    }

    pub fn for_rule(check_code: CheckCode) -> Self {
        Self {
            allowed_confusables: FxHashSet::from_iter([]),
            dummy_variable_rgx: Regex::new("^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$").unwrap(),
            enabled: FxHashSet::from_iter([check_code.clone()]),
            exclude: GlobSet::empty(),
            extend_exclude: GlobSet::empty(),
            external: FxHashSet::default(),
            fix: false,
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
            external: FxHashSet::default(),
            fix: false,
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
        for value in self.enabled.iter().sorted() {
            value.hash(state);
        }
        for value in self.external.iter().sorted() {
            value.hash(state);
        }
        for value in self.fixable.iter().sorted() {
            value.hash(state);
        }
        self.ignore_init_module_imports.hash(state);
        self.line_length.hash(state);
        for (absolute, basename, codes) in &self.per_file_ignores {
            absolute.glob().hash(state);
            basename.glob().hash(state);
            for value in codes.iter().sorted() {
                value.hash(state);
            }
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
pub fn resolve_globset(patterns: Vec<FilePattern>) -> Result<GlobSet> {
    let mut builder = globset::GlobSetBuilder::new();
    for pattern in patterns {
        pattern.add_to(&mut builder)?;
    }
    builder.build().map_err(std::convert::Into::into)
}

/// Given a list of patterns, create a `GlobSet`.
pub fn resolve_per_file_ignores(
    per_file_ignores: Vec<PerFileIgnore>,
) -> Result<Vec<(GlobMatcher, GlobMatcher, FxHashSet<CheckCode>)>> {
    per_file_ignores
        .into_iter()
        .map(|per_file_ignore| {
            // Construct absolute path matcher.
            let absolute =
                Glob::new(&per_file_ignore.absolute.to_string_lossy())?.compile_matcher();

            // Construct basename matcher.
            let basename = Glob::new(&per_file_ignore.basename)?.compile_matcher();

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
