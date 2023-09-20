//! Effective program settings, taking into account pyproject.toml and
//! command-line options. Structure is optimized for internal usage, as opposed
//! to external visibility or parsing.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use globset::{Glob, GlobMatcher};
use once_cell::sync::Lazy;
use path_absolutize::path_dedot;
use regex::Regex;
use rustc_hash::FxHashSet;

use crate::codes::RuleCodePrefix;
use ruff_macros::CacheKey;

use crate::registry::{Linter, Rule, RuleSet};
use crate::rules::{
    flake8_annotations, flake8_bandit, flake8_bugbear, flake8_builtins, flake8_comprehensions,
    flake8_copyright, flake8_errmsg, flake8_gettext, flake8_implicit_str_concat,
    flake8_import_conventions, flake8_pytest_style, flake8_quotes, flake8_self,
    flake8_tidy_imports, flake8_type_checking, flake8_unused_arguments, isort, mccabe, pep8_naming,
    pycodestyle, pydocstyle, pyflakes, pylint, pyupgrade,
};
use crate::settings::types::{PerFileIgnore, PythonVersion};
use crate::{codes, RuleSelector};

use super::line_width::{LineLength, TabSize};

use self::rule_table::RuleTable;
use self::types::PreviewMode;

pub mod flags;
pub mod rule_table;
pub mod types;

#[derive(Debug, CacheKey)]
pub struct LinterSettings {
    pub project_root: PathBuf,

    pub rules: RuleTable,
    pub per_file_ignores: Vec<(GlobMatcher, GlobMatcher, RuleSet)>,

    pub target_version: PythonVersion,
    pub preview: PreviewMode,

    // Rule-specific settings
    pub allowed_confusables: FxHashSet<char>,
    pub builtins: Vec<String>,
    pub dummy_variable_rgx: Regex,
    pub external: FxHashSet<String>,
    pub ignore_init_module_imports: bool,
    pub line_length: LineLength,
    pub logger_objects: Vec<String>,
    pub namespace_packages: Vec<PathBuf>,
    pub src: Vec<PathBuf>,
    pub tab_size: TabSize,
    pub task_tags: Vec<String>,
    pub typing_modules: Vec<String>,
    // Plugins
    pub flake8_annotations: flake8_annotations::settings::Settings,
    pub flake8_bandit: flake8_bandit::settings::Settings,
    pub flake8_bugbear: flake8_bugbear::settings::Settings,
    pub flake8_builtins: flake8_builtins::settings::Settings,
    pub flake8_comprehensions: flake8_comprehensions::settings::Settings,
    pub flake8_copyright: flake8_copyright::settings::Settings,
    pub flake8_errmsg: flake8_errmsg::settings::Settings,
    pub flake8_gettext: flake8_gettext::settings::Settings,
    pub flake8_implicit_str_concat: flake8_implicit_str_concat::settings::Settings,
    pub flake8_import_conventions: flake8_import_conventions::settings::Settings,
    pub flake8_pytest_style: flake8_pytest_style::settings::Settings,
    pub flake8_quotes: flake8_quotes::settings::Settings,
    pub flake8_self: flake8_self::settings::Settings,
    pub flake8_tidy_imports: flake8_tidy_imports::settings::Settings,
    pub flake8_type_checking: flake8_type_checking::settings::Settings,
    pub flake8_unused_arguments: flake8_unused_arguments::settings::Settings,
    pub isort: isort::settings::Settings,
    pub mccabe: mccabe::settings::Settings,
    pub pep8_naming: pep8_naming::settings::Settings,
    pub pycodestyle: pycodestyle::settings::Settings,
    pub pydocstyle: pydocstyle::settings::Settings,
    pub pyflakes: pyflakes::settings::Settings,
    pub pylint: pylint::settings::Settings,
    pub pyupgrade: pyupgrade::settings::Settings,
}

pub const PREFIXES: &[RuleSelector] = &[
    RuleSelector::Prefix {
        prefix: RuleCodePrefix::Pycodestyle(codes::Pycodestyle::E),
        redirected_from: None,
    },
    RuleSelector::Linter(Linter::Pyflakes),
];

pub const TASK_TAGS: &[&str] = &["TODO", "FIXME", "XXX"];

pub static DUMMY_VARIABLE_RGX: Lazy<Regex> =
    Lazy::new(|| Regex::new("^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$").unwrap());

impl LinterSettings {
    pub fn for_rule(rule_code: Rule) -> Self {
        Self {
            rules: RuleTable::from_iter([rule_code]),
            target_version: PythonVersion::latest(),
            ..Self::default()
        }
    }

    pub fn for_rules(rules: impl IntoIterator<Item = Rule>) -> Self {
        Self {
            rules: RuleTable::from_iter(rules),
            target_version: PythonVersion::latest(),
            ..Self::default()
        }
    }

    pub fn new(project_root: &Path) -> Self {
        Self {
            target_version: PythonVersion::default(),
            project_root: project_root.to_path_buf(),
            rules: PREFIXES
                .iter()
                .flat_map(|selector| selector.rules(PreviewMode::default()))
                .collect(),
            allowed_confusables: FxHashSet::from_iter([]),

            // Needs duplicating
            builtins: vec![],
            dummy_variable_rgx: DUMMY_VARIABLE_RGX.clone(),

            external: HashSet::default(),
            ignore_init_module_imports: false,
            line_length: LineLength::default(),
            logger_objects: vec![],
            namespace_packages: vec![],

            per_file_ignores: vec![],

            src: vec![path_dedot::CWD.clone()],
            // Needs duplicating
            tab_size: TabSize::default(),

            task_tags: TASK_TAGS.iter().map(ToString::to_string).collect(),
            typing_modules: vec![],
            flake8_annotations: flake8_annotations::settings::Settings::default(),
            flake8_bandit: flake8_bandit::settings::Settings::default(),
            flake8_bugbear: flake8_bugbear::settings::Settings::default(),
            flake8_builtins: flake8_builtins::settings::Settings::default(),
            flake8_comprehensions: flake8_comprehensions::settings::Settings::default(),
            flake8_copyright: flake8_copyright::settings::Settings::default(),
            flake8_errmsg: flake8_errmsg::settings::Settings::default(),
            flake8_implicit_str_concat: flake8_implicit_str_concat::settings::Settings::default(),
            flake8_import_conventions: flake8_import_conventions::settings::Settings::default(),
            flake8_pytest_style: flake8_pytest_style::settings::Settings::default(),
            flake8_quotes: flake8_quotes::settings::Settings::default(),
            flake8_gettext: flake8_gettext::settings::Settings::default(),
            flake8_self: flake8_self::settings::Settings::default(),
            flake8_tidy_imports: flake8_tidy_imports::settings::Settings::default(),
            flake8_type_checking: flake8_type_checking::settings::Settings::default(),
            flake8_unused_arguments: flake8_unused_arguments::settings::Settings::default(),
            isort: isort::settings::Settings::default(),
            mccabe: mccabe::settings::Settings::default(),
            pep8_naming: pep8_naming::settings::Settings::default(),
            pycodestyle: pycodestyle::settings::Settings::default(),
            pydocstyle: pydocstyle::settings::Settings::default(),
            pyflakes: pyflakes::settings::Settings::default(),
            pylint: pylint::settings::Settings::default(),
            pyupgrade: pyupgrade::settings::Settings::default(),
            preview: PreviewMode::default(),
        }
    }

    #[must_use]
    pub fn with_target_version(mut self, target_version: PythonVersion) -> Self {
        self.target_version = target_version;
        self
    }
}

impl Default for LinterSettings {
    fn default() -> Self {
        Self::new(path_dedot::CWD.as_path())
    }
}

/// Given a list of patterns, create a `GlobSet`.
pub fn resolve_per_file_ignores(
    per_file_ignores: Vec<PerFileIgnore>,
) -> Result<Vec<(GlobMatcher, GlobMatcher, RuleSet)>> {
    per_file_ignores
        .into_iter()
        .map(|per_file_ignore| {
            // Construct absolute path matcher.
            let absolute =
                Glob::new(&per_file_ignore.absolute.to_string_lossy())?.compile_matcher();

            // Construct basename matcher.
            let basename = Glob::new(&per_file_ignore.basename)?.compile_matcher();

            Ok((absolute, basename, per_file_ignore.rules))
        })
        .collect()
}
