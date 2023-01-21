//! Effective program settings, taking into account pyproject.toml and
//! command-line options. Structure is optimized for internal usage, as opposed
//! to external visibility or parsing.

use std::iter;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use colored::Colorize;
use globset::Glob;
use itertools::Either::{Left, Right};
use rustc_hash::FxHashSet;

use self::hashable::{HashableGlobMatcher, HashableGlobSet, HashableHashSet, HashableRegex};
use self::rule_table::RuleTable;
use crate::cache::cache_dir;
use crate::registry::{Rule, RuleSelector, SuffixLength, CATEGORIES, INCOMPATIBLE_CODES};
use crate::rules::{
    flake8_annotations, flake8_bandit, flake8_bugbear, flake8_builtins, flake8_errmsg,
    flake8_import_conventions, flake8_pytest_style, flake8_quotes, flake8_tidy_imports,
    flake8_unused_arguments, isort, mccabe, pep8_naming, pycodestyle, pydocstyle, pylint,
    pyupgrade,
};
use crate::settings::configuration::Configuration;
use crate::settings::types::{PerFileIgnore, PythonVersion, SerializationFormat, Version};
use crate::warn_user_once;

pub mod configuration;
pub mod defaults;
pub mod flags;
pub mod hashable;
pub mod options;
pub mod options_base;
pub mod pyproject;
mod rule_table;
pub mod types;

const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug)]
pub struct AllSettings {
    pub cli: CliSettings,
    pub lib: Settings,
}

impl AllSettings {
    pub fn from_configuration(config: Configuration, project_root: &Path) -> Result<Self> {
        Ok(Self {
            cli: CliSettings {
                cache_dir: config
                    .cache_dir
                    .clone()
                    .unwrap_or_else(|| cache_dir(project_root)),
                fix: config.fix.unwrap_or(false),
                fix_only: config.fix_only.unwrap_or(false),
                format: config.format.unwrap_or_default(),
                update_check: config.update_check.unwrap_or_default(),
            },
            lib: Settings::from_configuration(config, project_root)?,
        })
    }
}

#[derive(Debug, Default, Clone)]
/// Settings that are not used by this library and
/// only here so that `ruff_cli` can use them.
pub struct CliSettings {
    pub cache_dir: PathBuf,
    pub fix: bool,
    pub fix_only: bool,
    pub format: SerializationFormat,
    pub update_check: bool,
}

#[derive(Debug, Hash)]
#[allow(clippy::struct_excessive_bools)]
pub struct Settings {
    pub rules: RuleTable,
    pub per_file_ignores: Vec<(
        HashableGlobMatcher,
        HashableGlobMatcher,
        HashableHashSet<Rule>,
    )>,

    pub show_source: bool,
    pub target_version: PythonVersion,

    // Resolver settings
    pub exclude: HashableGlobSet,
    pub extend_exclude: HashableGlobSet,
    pub force_exclude: bool,
    pub respect_gitignore: bool,
    pub required_version: Option<Version>,

    // Rule-specific settings
    pub allowed_confusables: HashableHashSet<char>,
    pub builtins: Vec<String>,
    pub dummy_variable_rgx: HashableRegex,
    pub external: HashableHashSet<String>,
    pub ignore_init_module_imports: bool,
    pub line_length: usize,
    pub namespace_packages: Vec<PathBuf>,
    pub src: Vec<PathBuf>,
    pub task_tags: Vec<String>,
    pub typing_modules: Vec<String>,
    // Plugins
    pub flake8_annotations: flake8_annotations::settings::Settings,
    pub flake8_bandit: flake8_bandit::settings::Settings,
    pub flake8_bugbear: flake8_bugbear::settings::Settings,
    pub flake8_builtins: flake8_builtins::settings::Settings,
    pub flake8_errmsg: flake8_errmsg::settings::Settings,
    pub flake8_import_conventions: flake8_import_conventions::settings::Settings,
    pub flake8_pytest_style: flake8_pytest_style::settings::Settings,
    pub flake8_quotes: flake8_quotes::settings::Settings,
    pub flake8_tidy_imports: flake8_tidy_imports::Settings,
    pub flake8_unused_arguments: flake8_unused_arguments::settings::Settings,
    pub isort: isort::settings::Settings,
    pub mccabe: mccabe::settings::Settings,
    pub pep8_naming: pep8_naming::settings::Settings,
    pub pycodestyle: pycodestyle::settings::Settings,
    pub pydocstyle: pydocstyle::settings::Settings,
    pub pylint: pylint::settings::Settings,
    pub pyupgrade: pyupgrade::settings::Settings,
}

impl Settings {
    pub fn from_configuration(config: Configuration, project_root: &Path) -> Result<Self> {
        Ok(Self {
            rules: build_rule_table(
                config.fixable,
                config.unfixable,
                config.select,
                config.ignore,
                &config.extend_select,
                &config.extend_ignore,
                &config.pydocstyle,
            ),
            allowed_confusables: config
                .allowed_confusables
                .map(FxHashSet::from_iter)
                .unwrap_or_default()
                .into(),
            builtins: config.builtins.unwrap_or_default(),
            dummy_variable_rgx: config
                .dummy_variable_rgx
                .unwrap_or_else(|| defaults::DUMMY_VARIABLE_RGX.clone())
                .into(),
            exclude: HashableGlobSet::new(
                config.exclude.unwrap_or_else(|| defaults::EXCLUDE.clone()),
            )?,
            extend_exclude: HashableGlobSet::new(config.extend_exclude)?,
            external: FxHashSet::from_iter(config.external.unwrap_or_default()).into(),

            force_exclude: config.force_exclude.unwrap_or(false),

            ignore_init_module_imports: config.ignore_init_module_imports.unwrap_or_default(),
            line_length: config.line_length.unwrap_or(defaults::LINE_LENGTH),
            namespace_packages: config.namespace_packages.unwrap_or_default(),
            per_file_ignores: resolve_per_file_ignores(
                config.per_file_ignores.unwrap_or_default(),
            )?,
            respect_gitignore: config.respect_gitignore.unwrap_or(true),
            required_version: config.required_version,
            show_source: config.show_source.unwrap_or_default(),
            src: config
                .src
                .unwrap_or_else(|| vec![project_root.to_path_buf()]),
            target_version: config.target_version.unwrap_or(defaults::TARGET_VERSION),
            task_tags: config.task_tags.unwrap_or_else(|| {
                defaults::TASK_TAGS
                    .iter()
                    .map(ToString::to_string)
                    .collect()
            }),
            typing_modules: config.typing_modules.unwrap_or_default(),
            // Plugins
            flake8_annotations: config
                .flake8_annotations
                .map(Into::into)
                .unwrap_or_default(),
            flake8_bandit: config.flake8_bandit.map(Into::into).unwrap_or_default(),
            flake8_bugbear: config.flake8_bugbear.map(Into::into).unwrap_or_default(),
            flake8_builtins: config.flake8_builtins.map(Into::into).unwrap_or_default(),
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
            pylint: config.pylint.map(Into::into).unwrap_or_default(),
            pyupgrade: config.pyupgrade.map(Into::into).unwrap_or_default(),
        })
    }

    #[cfg(test)]
    pub fn for_rule(rule_code: Rule) -> Self {
        Self {
            rules: [rule_code].into(),
            ..Settings::default()
        }
    }

    #[cfg(test)]
    pub fn for_rules(rule_codes: Vec<Rule>) -> Self {
        Self {
            rules: rule_codes.into(),
            ..Settings::default()
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

fn build_rule_table(
    fixable: Option<Vec<RuleSelector>>,
    unfixable: Option<Vec<RuleSelector>>,
    select: Option<Vec<RuleSelector>>,
    ignore: Option<Vec<RuleSelector>>,
    extend_select: &[Vec<RuleSelector>],
    extend_ignore: &[Vec<RuleSelector>],
    pydocstyle: &Option<pydocstyle::settings::Options>,
) -> RuleTable {
    let mut rules = RuleTable::empty();

    let fixable = resolve_codes([RuleCodeSpec {
        select: &fixable.unwrap_or_else(|| CATEGORIES.to_vec()),
        ignore: &unfixable.unwrap_or_default(),
    }]);

    for code in validate_enabled(resolve_codes(
        [RuleCodeSpec {
            select: &select.unwrap_or_else(|| defaults::PREFIXES.to_vec()),
            ignore: &ignore.unwrap_or_default(),
        }]
        .into_iter()
        .chain(
            extend_select
                .iter()
                .zip(extend_ignore.iter())
                .map(|(select, ignore)| RuleCodeSpec { select, ignore }),
        )
        .chain(
            // If a docstring convention is specified, force-disable any incompatible error
            // codes.
            if let Some(convention) = pydocstyle
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
    )) {
        let fix = fixable.contains(&code);
        rules.enable(code, fix);
    }
    rules
}

/// Given a list of patterns, create a `GlobSet`.
pub fn resolve_per_file_ignores(
    per_file_ignores: Vec<PerFileIgnore>,
) -> Result<
    Vec<(
        HashableGlobMatcher,
        HashableGlobMatcher,
        HashableHashSet<Rule>,
    )>,
> {
    per_file_ignores
        .into_iter()
        .map(|per_file_ignore| {
            // Construct absolute path matcher.
            let absolute =
                Glob::new(&per_file_ignore.absolute.to_string_lossy())?.compile_matcher();

            // Construct basename matcher.
            let basename = Glob::new(&per_file_ignore.basename)?.compile_matcher();

            Ok((absolute.into(), basename.into(), per_file_ignore.codes))
        })
        .collect()
}

#[derive(Debug)]
struct RuleCodeSpec<'a> {
    select: &'a [RuleSelector],
    ignore: &'a [RuleSelector],
}

/// Given a set of selected and ignored prefixes, resolve the set of enabled
/// rule codes.
fn resolve_codes<'a>(specs: impl IntoIterator<Item = RuleCodeSpec<'a>>) -> FxHashSet<Rule> {
    let mut codes: FxHashSet<Rule> = FxHashSet::default();
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
fn validate_enabled(enabled: FxHashSet<Rule>) -> FxHashSet<Rule> {
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

    use crate::registry::{Rule, RuleSelector};
    use crate::settings::{resolve_codes, RuleCodeSpec};

    #[test]
    fn rule_codes() {
        let actual = resolve_codes([RuleCodeSpec {
            select: &[RuleSelector::W],
            ignore: &[],
        }]);
        let expected = FxHashSet::from_iter([
            Rule::NoNewLineAtEndOfFile,
            Rule::DocLineTooLong,
            Rule::InvalidEscapeSequence,
        ]);
        assert_eq!(actual, expected);

        let actual = resolve_codes([RuleCodeSpec {
            select: &[RuleSelector::W6],
            ignore: &[],
        }]);
        let expected = FxHashSet::from_iter([Rule::InvalidEscapeSequence]);
        assert_eq!(actual, expected);

        let actual = resolve_codes([RuleCodeSpec {
            select: &[RuleSelector::W],
            ignore: &[RuleSelector::W292],
        }]);
        let expected = FxHashSet::from_iter([Rule::DocLineTooLong, Rule::InvalidEscapeSequence]);
        assert_eq!(actual, expected);

        let actual = resolve_codes([RuleCodeSpec {
            select: &[RuleSelector::W605],
            ignore: &[RuleSelector::W605],
        }]);
        let expected = FxHashSet::from_iter([]);
        assert_eq!(actual, expected);

        let actual = resolve_codes([
            RuleCodeSpec {
                select: &[RuleSelector::W],
                ignore: &[RuleSelector::W292],
            },
            RuleCodeSpec {
                select: &[RuleSelector::W292],
                ignore: &[],
            },
        ]);
        let expected = FxHashSet::from_iter([
            Rule::NoNewLineAtEndOfFile,
            Rule::DocLineTooLong,
            Rule::InvalidEscapeSequence,
        ]);
        assert_eq!(actual, expected);

        let actual = resolve_codes([
            RuleCodeSpec {
                select: &[RuleSelector::W],
                ignore: &[RuleSelector::W292],
            },
            RuleCodeSpec {
                select: &[RuleSelector::W292],
                ignore: &[RuleSelector::W],
            },
        ]);
        let expected = FxHashSet::from_iter([Rule::NoNewLineAtEndOfFile]);
        assert_eq!(actual, expected);
    }
}
