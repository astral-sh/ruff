//! Effective program settings, taking into account pyproject.toml and
//! command-line options. Structure is optimized for internal usage, as opposed
//! to external visibility or parsing.

use std::path::PathBuf;

use anyhow::Result;
use globset::{Glob, GlobMatcher};
use once_cell::sync::Lazy;
use regex::Regex;
use rustc_hash::FxHashSet;

use ruff_macros::CacheKey;

use crate::registry::{Rule, RuleSet};
use crate::rules::{
    flake8_annotations, flake8_bandit, flake8_bugbear, flake8_builtins, flake8_comprehensions,
    flake8_copyright, flake8_errmsg, flake8_gettext, flake8_implicit_str_concat,
    flake8_import_conventions, flake8_pytest_style, flake8_quotes, flake8_self,
    flake8_tidy_imports, flake8_type_checking, flake8_unused_arguments, isort, mccabe, pep8_naming,
    pycodestyle, pydocstyle, pyflakes, pylint, pyupgrade,
};
use crate::settings::types::{
    FilePattern, FilePatternSet, PerFileIgnore, PythonVersion, SerializationFormat,
};

use super::line_width::{LineLength, TabSize};

use self::rule_table::RuleTable;
use self::types::PreviewMode;

pub mod defaults;
pub mod flags;
pub mod rule_table;
pub mod types;

pub static EXCLUDE: Lazy<Vec<FilePattern>> = Lazy::new(|| {
    vec![
        FilePattern::Builtin(".bzr"),
        FilePattern::Builtin(".direnv"),
        FilePattern::Builtin(".eggs"),
        FilePattern::Builtin(".git"),
        FilePattern::Builtin(".git-rewrite"),
        FilePattern::Builtin(".hg"),
        FilePattern::Builtin(".ipynb_checkpoints"),
        FilePattern::Builtin(".mypy_cache"),
        FilePattern::Builtin(".nox"),
        FilePattern::Builtin(".pants.d"),
        FilePattern::Builtin(".pyenv"),
        FilePattern::Builtin(".pytest_cache"),
        FilePattern::Builtin(".pytype"),
        FilePattern::Builtin(".ruff_cache"),
        FilePattern::Builtin(".svn"),
        FilePattern::Builtin(".tox"),
        FilePattern::Builtin(".venv"),
        FilePattern::Builtin(".vscode"),
        FilePattern::Builtin("__pypackages__"),
        FilePattern::Builtin("_build"),
        FilePattern::Builtin("buck-out"),
        FilePattern::Builtin("build"),
        FilePattern::Builtin("dist"),
        FilePattern::Builtin("node_modules"),
        FilePattern::Builtin("venv"),
    ]
});

pub static INCLUDE: Lazy<Vec<FilePattern>> = Lazy::new(|| {
    vec![
        FilePattern::Builtin("*.py"),
        FilePattern::Builtin("*.pyi"),
        FilePattern::Builtin("**/pyproject.toml"),
    ]
});

#[derive(Debug, CacheKey)]
pub struct FileResolverSettings {
    pub exclude: FilePatternSet,
    pub extend_exclude: FilePatternSet,
    pub force_exclude: bool,
    pub include: FilePatternSet,
    pub extend_include: FilePatternSet,
    pub respect_gitignore: bool,
    pub project_root: PathBuf,
}

impl FileResolverSettings {
    fn with_project_root(project_root: PathBuf) -> Self {
        Self {
            project_root,
            exclude: FilePatternSet::try_from_vec(EXCLUDE.clone()).unwrap(),
            extend_exclude: FilePatternSet::default(),
            extend_include: FilePatternSet::default(),
            force_exclude: false,
            respect_gitignore: true,
            include: FilePatternSet::try_from_vec(INCLUDE.clone()).unwrap(),
        }
    }
}

#[derive(Debug, CacheKey)]
#[allow(clippy::struct_excessive_bools)]
pub struct Settings {
    #[cache_key(ignore)]
    pub cache_dir: PathBuf,
    #[cache_key(ignore)]
    pub fix: bool,
    #[cache_key(ignore)]
    pub fix_only: bool,
    #[cache_key(ignore)]
    pub output_format: SerializationFormat,
    #[cache_key(ignore)]
    pub show_fixes: bool,
    #[cache_key(ignore)]
    pub show_source: bool,

    pub file_resolver: FileResolverSettings,

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

impl Settings {
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

    /// Return the [`Settings`] after updating the target [`PythonVersion`].
    #[must_use]
    pub fn with_target_version(mut self, target_version: PythonVersion) -> Self {
        self.target_version = target_version;
        self
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
