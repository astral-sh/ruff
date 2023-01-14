//! Effective program settings, taking into account pyproject.toml and
//! command-line options. Structure is optimized for internal usage, as opposed
//! to external visibility or parsing.

use std::hash::{Hash, Hasher};
use std::iter;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use colored::Colorize;
use globset::{Glob, GlobMatcher, GlobSet};
use itertools::Either::{Left, Right};
use itertools::Itertools;
use once_cell::sync::Lazy;
use path_absolutize::path_dedot;
use regex::Regex;
use rustc_hash::FxHashSet;

use crate::cache::cache_dir;
use crate::registry::{RuleCode, RuleCodePrefix, SuffixLength, CATEGORIES, INCOMPATIBLE_CODES};
use crate::settings::configuration::Configuration;
use crate::settings::types::{
    FilePattern, PerFileIgnore, PythonVersion, SerializationFormat, Version,
};
use crate::{
    flake8_annotations, flake8_bandit, flake8_bugbear, flake8_errmsg, flake8_import_conventions,
    flake8_pytest_style, flake8_quotes, flake8_tidy_imports, flake8_unused_arguments, isort,
    mccabe, pep8_naming, pycodestyle, pydocstyle, pyupgrade, warn_user_once,
};

pub mod configuration;
pub mod flags;
pub mod options;
pub mod options_base;
pub mod pyproject;
pub mod types;

const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct Settings {
    pub allowed_confusables: FxHashSet<char>,
    pub builtins: Vec<String>,
    pub cache_dir: PathBuf,
    pub dummy_variable_rgx: Regex,
    pub enabled: FxHashSet<RuleCode>,
    pub exclude: GlobSet,
    pub extend_exclude: GlobSet,
    pub external: FxHashSet<String>,
    pub fix: bool,
    pub fix_only: bool,
    pub fixable: FxHashSet<RuleCode>,
    pub force_exclude: bool,
    pub format: SerializationFormat,
    pub ignore_init_module_imports: bool,
    pub line_length: usize,
    pub per_file_ignores: Vec<(GlobMatcher, GlobMatcher, FxHashSet<RuleCode>)>,
    pub required_version: Option<Version>,
    pub respect_gitignore: bool,
    pub show_source: bool,
    pub src: Vec<PathBuf>,
    pub target_version: PythonVersion,
    pub task_tags: Vec<String>,
    pub typing_modules: Vec<String>,
    pub update_check: bool,
    // Plugins
    pub flake8_annotations: flake8_annotations::settings::Settings,
    pub flake8_bandit: flake8_bandit::settings::Settings,
    pub flake8_bugbear: flake8_bugbear::settings::Settings,
    pub flake8_errmsg: flake8_errmsg::settings::Settings,
    pub flake8_import_conventions: flake8_import_conventions::settings::Settings,
    pub flake8_pytest_style: flake8_pytest_style::settings::Settings,
    pub flake8_quotes: flake8_quotes::settings::Settings,
    pub flake8_tidy_imports: flake8_tidy_imports::settings::Settings,
    pub flake8_unused_arguments: flake8_unused_arguments::settings::Settings,
    pub isort: isort::settings::Settings,
    pub mccabe: mccabe::settings::Settings,
    pub pep8_naming: pep8_naming::settings::Settings,
    pub pycodestyle: pycodestyle::settings::Settings,
    pub pydocstyle: pydocstyle::settings::Settings,
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
            builtins: config.builtins.unwrap_or_default(),
            cache_dir: config.cache_dir.unwrap_or_else(|| cache_dir(project_root)),
            dummy_variable_rgx: config
                .dummy_variable_rgx
                .unwrap_or_else(|| DEFAULT_DUMMY_VARIABLE_RGX.clone()),
            enabled: validate_enabled(resolve_codes(
                [RuleCodeSpec {
                    select: &config
                        .select
                        .unwrap_or_else(|| vec![RuleCodePrefix::E, RuleCodePrefix::F]),
                    ignore: &config.ignore.unwrap_or_default(),
                }]
                .into_iter()
                .chain(
                    config
                        .extend_select
                        .iter()
                        .zip(config.extend_ignore.iter())
                        .map(|(select, ignore)| RuleCodeSpec { select, ignore }),
                )
                .chain(
                    // If a docstring convention is specified, force-disable any incompatible error
                    // codes.
                    if let Some(convention) = config
                        .pydocstyle
                        .as_ref()
                        .and_then(|pydocstyle| pydocstyle.convention)
                    {
                        Left(iter::once(RuleCodeSpec {
                            select: &[],
                            ignore: convention.codes(),
                        }))
                    } else {
                        Right(iter::empty())
                    },
                ),
            )),
            exclude: resolve_globset(config.exclude.unwrap_or_else(|| DEFAULT_EXCLUDE.clone()))?,
            extend_exclude: resolve_globset(config.extend_exclude)?,
            external: FxHashSet::from_iter(config.external.unwrap_or_default()),
            fix: config.fix.unwrap_or(false),
            fix_only: config.fix_only.unwrap_or(false),
            fixable: resolve_codes(
                [RuleCodeSpec {
                    select: &config.fixable.unwrap_or_else(|| CATEGORIES.to_vec()),
                    ignore: &config.unfixable.unwrap_or_default(),
                }]
                .into_iter(),
            ),
            format: config.format.unwrap_or_default(),
            force_exclude: config.force_exclude.unwrap_or(false),
            ignore_init_module_imports: config.ignore_init_module_imports.unwrap_or_default(),
            line_length: config.line_length.unwrap_or(88),
            per_file_ignores: resolve_per_file_ignores(
                config.per_file_ignores.unwrap_or_default(),
            )?,
            respect_gitignore: config.respect_gitignore.unwrap_or(true),
            required_version: config.required_version,
            show_source: config.show_source.unwrap_or_default(),
            src: config
                .src
                .unwrap_or_else(|| vec![project_root.to_path_buf()]),
            target_version: config.target_version.unwrap_or_default(),
            task_tags: config.task_tags.unwrap_or_else(|| {
                vec!["TODO".to_string(), "FIXME".to_string(), "XXX".to_string()]
            }),
            typing_modules: config.typing_modules.unwrap_or_default(),
            update_check: config.update_check.unwrap_or_default(),
            // Plugins
            flake8_annotations: config
                .flake8_annotations
                .map(Into::into)
                .unwrap_or_default(),
            flake8_bandit: config.flake8_bandit.map(Into::into).unwrap_or_default(),
            flake8_bugbear: config.flake8_bugbear.map(Into::into).unwrap_or_default(),
            flake8_errmsg: config.flake8_errmsg.map(Into::into).unwrap_or_default(),
            flake8_import_conventions: config
                .flake8_import_conventions
                .map(Into::into)
                .unwrap_or_default(),
            flake8_pytest_style: config
                .flake8_pytest_style
                .map(Into::into)
                .unwrap_or_default(),
            flake8_quotes: config.flake8_quotes.map(Into::into).unwrap_or_default(),
            flake8_tidy_imports: config
                .flake8_tidy_imports
                .map(Into::into)
                .unwrap_or_default(),
            flake8_unused_arguments: config
                .flake8_unused_arguments
                .map(Into::into)
                .unwrap_or_default(),
            isort: config.isort.map(Into::into).unwrap_or_default(),
            mccabe: config.mccabe.map(Into::into).unwrap_or_default(),
            pep8_naming: config.pep8_naming.map(Into::into).unwrap_or_default(),
            pycodestyle: config.pycodestyle.map(Into::into).unwrap_or_default(),
            pydocstyle: config.pydocstyle.map(Into::into).unwrap_or_default(),
            pyupgrade: config.pyupgrade.map(Into::into).unwrap_or_default(),
        })
    }

    pub fn for_rule(rule_code: RuleCode) -> Self {
        Self {
            allowed_confusables: FxHashSet::from_iter([]),
            builtins: vec![],
            cache_dir: cache_dir(path_dedot::CWD.as_path()),
            dummy_variable_rgx: Regex::new("^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$").unwrap(),
            enabled: FxHashSet::from_iter([rule_code.clone()]),
            exclude: GlobSet::empty(),
            extend_exclude: GlobSet::empty(),
            external: FxHashSet::default(),
            fix: false,
            fix_only: false,
            fixable: FxHashSet::from_iter([rule_code]),
            force_exclude: false,
            format: SerializationFormat::Text,
            ignore_init_module_imports: false,
            line_length: 88,
            per_file_ignores: vec![],
            required_version: None,
            respect_gitignore: true,
            show_source: false,
            src: vec![path_dedot::CWD.clone()],
            target_version: PythonVersion::Py310,
            task_tags: vec!["TODO".to_string(), "FIXME".to_string(), "XXX".to_string()],
            typing_modules: vec![],
            update_check: false,
            flake8_annotations: flake8_annotations::settings::Settings::default(),
            flake8_bandit: flake8_bandit::settings::Settings::default(),
            flake8_bugbear: flake8_bugbear::settings::Settings::default(),
            flake8_errmsg: flake8_errmsg::settings::Settings::default(),
            flake8_import_conventions: flake8_import_conventions::settings::Settings::default(),
            flake8_pytest_style: flake8_pytest_style::settings::Settings::default(),
            flake8_quotes: flake8_quotes::settings::Settings::default(),
            flake8_tidy_imports: flake8_tidy_imports::settings::Settings::default(),
            flake8_unused_arguments: flake8_unused_arguments::settings::Settings::default(),
            isort: isort::settings::Settings::default(),
            mccabe: mccabe::settings::Settings::default(),
            pep8_naming: pep8_naming::settings::Settings::default(),
            pycodestyle: pycodestyle::settings::Settings::default(),
            pydocstyle: pydocstyle::settings::Settings::default(),
            pyupgrade: pyupgrade::settings::Settings::default(),
        }
    }

    pub fn for_rules(rule_codes: Vec<RuleCode>) -> Self {
        Self {
            allowed_confusables: FxHashSet::from_iter([]),
            builtins: vec![],
            cache_dir: cache_dir(path_dedot::CWD.as_path()),
            dummy_variable_rgx: Regex::new("^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$").unwrap(),
            enabled: FxHashSet::from_iter(rule_codes.clone()),
            exclude: GlobSet::empty(),
            extend_exclude: GlobSet::empty(),
            external: FxHashSet::default(),
            fix: false,
            fix_only: false,
            fixable: FxHashSet::from_iter(rule_codes),
            force_exclude: false,
            format: SerializationFormat::Text,
            ignore_init_module_imports: false,
            line_length: 88,
            per_file_ignores: vec![],
            required_version: None,
            respect_gitignore: true,
            show_source: false,
            src: vec![path_dedot::CWD.clone()],
            target_version: PythonVersion::Py310,
            task_tags: vec!["TODO".to_string(), "FIXME".to_string(), "XXX".to_string()],
            typing_modules: vec![],
            update_check: false,
            flake8_annotations: flake8_annotations::settings::Settings::default(),
            flake8_bandit: flake8_bandit::settings::Settings::default(),
            flake8_bugbear: flake8_bugbear::settings::Settings::default(),
            flake8_errmsg: flake8_errmsg::settings::Settings::default(),
            flake8_import_conventions: flake8_import_conventions::settings::Settings::default(),
            flake8_pytest_style: flake8_pytest_style::settings::Settings::default(),
            flake8_quotes: flake8_quotes::settings::Settings::default(),
            flake8_tidy_imports: flake8_tidy_imports::settings::Settings::default(),
            flake8_unused_arguments: flake8_unused_arguments::settings::Settings::default(),
            isort: isort::settings::Settings::default(),
            mccabe: mccabe::settings::Settings::default(),
            pep8_naming: pep8_naming::settings::Settings::default(),
            pycodestyle: pycodestyle::settings::Settings::default(),
            pydocstyle: pydocstyle::settings::Settings::default(),
            pyupgrade: pyupgrade::settings::Settings::default(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if let Some(required_version) = &self.required_version {
            if &**required_version != CARGO_PKG_VERSION {
                return Err(anyhow!(
                    "Required version `{}` does not match the running version `{}`",
                    &**required_version,
                    CARGO_PKG_VERSION
                ));
            }
        }
        Ok(())
    }
}

impl Hash for Settings {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Add base properties in alphabetical order.
        for confusable in &self.allowed_confusables {
            confusable.hash(state);
        }
        self.builtins.hash(state);
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
        self.src.hash(state);
        self.target_version.hash(state);
        self.task_tags.hash(state);
        self.typing_modules.hash(state);
        // Add plugin properties in alphabetical order.
        self.flake8_annotations.hash(state);
        self.flake8_bandit.hash(state);
        self.flake8_bugbear.hash(state);
        self.flake8_errmsg.hash(state);
        self.flake8_import_conventions.hash(state);
        self.flake8_pytest_style.hash(state);
        self.flake8_quotes.hash(state);
        self.flake8_tidy_imports.hash(state);
        self.flake8_unused_arguments.hash(state);
        self.isort.hash(state);
        self.mccabe.hash(state);
        self.pep8_naming.hash(state);
        self.pydocstyle.hash(state);
        self.pyupgrade.hash(state);
    }
}

/// Given a list of patterns, create a `GlobSet`.
pub fn resolve_globset(patterns: Vec<FilePattern>) -> Result<GlobSet> {
    let mut builder = globset::GlobSetBuilder::new();
    for pattern in patterns {
        pattern.add_to(&mut builder)?;
    }
    builder.build().map_err(Into::into)
}

/// Given a list of patterns, create a `GlobSet`.
pub fn resolve_per_file_ignores(
    per_file_ignores: Vec<PerFileIgnore>,
) -> Result<Vec<(GlobMatcher, GlobMatcher, FxHashSet<RuleCode>)>> {
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

#[derive(Debug)]
struct RuleCodeSpec<'a> {
    select: &'a [RuleCodePrefix],
    ignore: &'a [RuleCodePrefix],
}

/// Given a set of selected and ignored prefixes, resolve the set of enabled
/// rule codes.
fn resolve_codes<'a>(specs: impl Iterator<Item = RuleCodeSpec<'a>>) -> FxHashSet<RuleCode> {
    let mut codes: FxHashSet<RuleCode> = FxHashSet::default();
    for spec in specs {
        for specificity in [
            SuffixLength::None,
            SuffixLength::Zero,
            SuffixLength::One,
            SuffixLength::Two,
            SuffixLength::Three,
            SuffixLength::Four,
        ] {
            for prefix in spec.select {
                if prefix.specificity() == specificity {
                    codes.extend(prefix.codes());
                }
            }
            for prefix in spec.ignore {
                if prefix.specificity() == specificity {
                    for code in prefix.codes() {
                        codes.remove(&code);
                    }
                }
            }
        }
    }
    codes
}

/// Warn if the set of enabled codes contains any incompatibilities.
fn validate_enabled(enabled: FxHashSet<RuleCode>) -> FxHashSet<RuleCode> {
    for (a, b, message) in INCOMPATIBLE_CODES {
        if enabled.contains(a) && enabled.contains(b) {
            warn_user_once!("{}", message);
        }
    }
    enabled
}

#[cfg(test)]
mod tests {
    use rustc_hash::FxHashSet;

    use crate::registry::{RuleCode, RuleCodePrefix};
    use crate::settings::{resolve_codes, RuleCodeSpec};

    #[test]
    fn rule_codes() {
        let actual = resolve_codes(
            [RuleCodeSpec {
                select: &[RuleCodePrefix::W],
                ignore: &[],
            }]
            .into_iter(),
        );
        let expected = FxHashSet::from_iter([RuleCode::W292, RuleCode::W505, RuleCode::W605]);
        assert_eq!(actual, expected);

        let actual = resolve_codes(
            [RuleCodeSpec {
                select: &[RuleCodePrefix::W6],
                ignore: &[],
            }]
            .into_iter(),
        );
        let expected = FxHashSet::from_iter([RuleCode::W605]);
        assert_eq!(actual, expected);

        let actual = resolve_codes(
            [RuleCodeSpec {
                select: &[RuleCodePrefix::W],
                ignore: &[RuleCodePrefix::W292],
            }]
            .into_iter(),
        );
        let expected = FxHashSet::from_iter([RuleCode::W505, RuleCode::W605]);
        assert_eq!(actual, expected);

        let actual = resolve_codes(
            [RuleCodeSpec {
                select: &[RuleCodePrefix::W605],
                ignore: &[RuleCodePrefix::W605],
            }]
            .into_iter(),
        );
        let expected = FxHashSet::from_iter([]);
        assert_eq!(actual, expected);

        let actual = resolve_codes(
            [
                RuleCodeSpec {
                    select: &[RuleCodePrefix::W],
                    ignore: &[RuleCodePrefix::W292],
                },
                RuleCodeSpec {
                    select: &[RuleCodePrefix::W292],
                    ignore: &[],
                },
            ]
            .into_iter(),
        );
        let expected = FxHashSet::from_iter([RuleCode::W292, RuleCode::W505, RuleCode::W605]);
        assert_eq!(actual, expected);

        let actual = resolve_codes(
            [
                RuleCodeSpec {
                    select: &[RuleCodePrefix::W],
                    ignore: &[RuleCodePrefix::W292],
                },
                RuleCodeSpec {
                    select: &[RuleCodePrefix::W292],
                    ignore: &[RuleCodePrefix::W],
                },
            ]
            .into_iter(),
        );
        let expected = FxHashSet::from_iter([RuleCode::W292]);
        assert_eq!(actual, expected);
    }
}
