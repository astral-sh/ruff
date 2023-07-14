//! User-provided program settings, taking into account pyproject.toml and
//! command-line options. Structure mirrors the user-facing representation of
//! the various parameters.

use std::borrow::Cow;
use std::env::VarError;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use glob::{glob, GlobError, Paths, PatternError};
use regex::Regex;
use shellexpand;
use shellexpand::LookupError;

use crate::fs;
use crate::line_width::{LineLength, TabSize};
use crate::rule_selector::RuleSelector;
use crate::rules::{
    flake8_annotations, flake8_bandit, flake8_bugbear, flake8_builtins, flake8_comprehensions,
    flake8_copyright, flake8_errmsg, flake8_gettext, flake8_implicit_str_concat,
    flake8_import_conventions, flake8_pytest_style, flake8_quotes, flake8_self,
    flake8_tidy_imports, flake8_type_checking, flake8_unused_arguments, isort, mccabe, pep8_naming,
    pycodestyle, pydocstyle, pyflakes, pylint, pyupgrade,
};
use crate::settings::options::Options;
use crate::settings::types::{
    FilePattern, PerFileIgnore, PythonVersion, SerializationFormat, Version,
};

#[derive(Debug, Default)]
pub struct RuleSelection {
    pub select: Option<Vec<RuleSelector>>,
    pub ignore: Vec<RuleSelector>,
    pub extend_select: Vec<RuleSelector>,
    pub fixable: Option<Vec<RuleSelector>>,
    pub unfixable: Vec<RuleSelector>,
    pub extend_fixable: Vec<RuleSelector>,
}

#[derive(Debug, Default)]
pub struct Configuration {
    pub rule_selections: Vec<RuleSelection>,
    pub per_file_ignores: Option<Vec<PerFileIgnore>>,

    pub allowed_confusables: Option<Vec<char>>,
    pub builtins: Option<Vec<String>>,
    pub cache_dir: Option<PathBuf>,
    pub dummy_variable_rgx: Option<Regex>,
    pub exclude: Option<Vec<FilePattern>>,
    pub extend: Option<PathBuf>,
    pub extend_exclude: Vec<FilePattern>,
    pub extend_include: Vec<FilePattern>,
    pub extend_per_file_ignores: Vec<PerFileIgnore>,
    pub external: Option<Vec<String>>,
    pub fix: Option<bool>,
    pub fix_only: Option<bool>,
    pub force_exclude: Option<bool>,
    pub format: Option<SerializationFormat>,
    pub ignore_init_module_imports: Option<bool>,
    pub include: Option<Vec<FilePattern>>,
    pub line_length: Option<LineLength>,
    pub tab_size: Option<TabSize>,
    pub namespace_packages: Option<Vec<PathBuf>>,
    pub required_version: Option<Version>,
    pub respect_gitignore: Option<bool>,
    pub show_fixes: Option<bool>,
    pub show_source: Option<bool>,
    pub src: Option<Vec<PathBuf>>,
    pub target_version: Option<PythonVersion>,
    pub task_tags: Option<Vec<String>>,
    pub typing_modules: Option<Vec<String>>,
    // Plugins
    pub flake8_annotations: Option<flake8_annotations::settings::Options>,
    pub flake8_bandit: Option<flake8_bandit::settings::Options>,
    pub flake8_bugbear: Option<flake8_bugbear::settings::Options>,
    pub flake8_builtins: Option<flake8_builtins::settings::Options>,
    pub flake8_comprehensions: Option<flake8_comprehensions::settings::Options>,
    pub flake8_copyright: Option<flake8_copyright::settings::Options>,
    pub flake8_errmsg: Option<flake8_errmsg::settings::Options>,
    pub flake8_gettext: Option<flake8_gettext::settings::Options>,
    pub flake8_implicit_str_concat: Option<flake8_implicit_str_concat::settings::Options>,
    pub flake8_import_conventions: Option<flake8_import_conventions::settings::Options>,
    pub flake8_pytest_style: Option<flake8_pytest_style::settings::Options>,
    pub flake8_quotes: Option<flake8_quotes::settings::Options>,
    pub flake8_self: Option<flake8_self::settings::Options>,
    pub flake8_tidy_imports: Option<flake8_tidy_imports::options::Options>,
    pub flake8_type_checking: Option<flake8_type_checking::settings::Options>,
    pub flake8_unused_arguments: Option<flake8_unused_arguments::settings::Options>,
    pub isort: Option<isort::settings::Options>,
    pub mccabe: Option<mccabe::settings::Options>,
    pub pep8_naming: Option<pep8_naming::settings::Options>,
    pub pycodestyle: Option<pycodestyle::settings::Options>,
    pub pydocstyle: Option<pydocstyle::settings::Options>,
    pub pyflakes: Option<pyflakes::settings::Options>,
    pub pylint: Option<pylint::settings::Options>,
    pub pyupgrade: Option<pyupgrade::settings::Options>,
}

impl Configuration {
    pub fn from_options(options: Options, project_root: &Path) -> Result<Self> {
        Ok(Self {
            rule_selections: vec![RuleSelection {
                select: options.select,
                ignore: options
                    .ignore
                    .into_iter()
                    .flatten()
                    .chain(options.extend_ignore.into_iter().flatten())
                    .collect(),
                extend_select: options.extend_select.unwrap_or_default(),
                fixable: options.fixable,
                unfixable: options
                    .unfixable
                    .into_iter()
                    .flatten()
                    .chain(options.extend_unfixable.into_iter().flatten())
                    .collect(),
                extend_fixable: options.extend_fixable.unwrap_or_default(),
            }],
            allowed_confusables: options.allowed_confusables,
            builtins: options.builtins,
            cache_dir: options
                .cache_dir
                .map(|dir| {
                    let dir = shellexpand::full(&dir);
                    dir.map(|dir| PathBuf::from(dir.as_ref()))
                })
                .transpose()
                .map_err(|e| anyhow!("Invalid `cache-dir` value: {e}"))?,
            dummy_variable_rgx: options
                .dummy_variable_rgx
                .map(|pattern| Regex::new(&pattern))
                .transpose()
                .map_err(|e| anyhow!("Invalid `dummy-variable-rgx` value: {e}"))?,
            exclude: options.exclude.map(|paths| {
                paths
                    .into_iter()
                    .map(|pattern| {
                        let absolute = fs::normalize_path_to(&pattern, project_root);
                        FilePattern::User(pattern, absolute)
                    })
                    .collect()
            }),
            extend: options
                .extend
                .map(|extend| {
                    let extend = shellexpand::full(&extend);
                    extend.map(|extend| PathBuf::from(extend.as_ref()))
                })
                .transpose()
                .map_err(|e| anyhow!("Invalid `extend` value: {e}"))?,
            extend_exclude: options
                .extend_exclude
                .map(|paths| {
                    paths
                        .into_iter()
                        .map(|pattern| {
                            let absolute = fs::normalize_path_to(&pattern, project_root);
                            FilePattern::User(pattern, absolute)
                        })
                        .collect()
                })
                .unwrap_or_default(),
            extend_include: options
                .extend_include
                .map(|paths| {
                    paths
                        .into_iter()
                        .map(|pattern| {
                            let absolute = fs::normalize_path_to(&pattern, project_root);
                            FilePattern::User(pattern, absolute)
                        })
                        .collect()
                })
                .unwrap_or_default(),
            extend_per_file_ignores: options
                .extend_per_file_ignores
                .map(|per_file_ignores| {
                    per_file_ignores
                        .into_iter()
                        .map(|(pattern, prefixes)| {
                            PerFileIgnore::new(pattern, &prefixes, Some(project_root))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            external: options.external,
            fix: options.fix,
            fix_only: options.fix_only,
            format: options.format,
            force_exclude: options.force_exclude,
            ignore_init_module_imports: options.ignore_init_module_imports,
            include: options.include.map(|paths| {
                paths
                    .into_iter()
                    .map(|pattern| {
                        let absolute = fs::normalize_path_to(&pattern, project_root);
                        FilePattern::User(pattern, absolute)
                    })
                    .collect()
            }),
            line_length: options.line_length,
            tab_size: options.tab_size,
            namespace_packages: options
                .namespace_packages
                .map(|namespace_package| resolve_src(&namespace_package, project_root))
                .transpose()?,
            per_file_ignores: options.per_file_ignores.map(|per_file_ignores| {
                per_file_ignores
                    .into_iter()
                    .map(|(pattern, prefixes)| {
                        PerFileIgnore::new(pattern, &prefixes, Some(project_root))
                    })
                    .collect()
            }),
            required_version: options.required_version,
            respect_gitignore: options.respect_gitignore,
            show_source: options.show_source,
            show_fixes: options.show_fixes,
            src: options
                .src
                .map(|src| resolve_src(&src, project_root))
                .transpose()?,
            target_version: options.target_version,
            task_tags: options.task_tags,
            typing_modules: options.typing_modules,
            // Plugins
            flake8_annotations: options.flake8_annotations,
            flake8_bandit: options.flake8_bandit,
            flake8_bugbear: options.flake8_bugbear,
            flake8_builtins: options.flake8_builtins,
            flake8_comprehensions: options.flake8_comprehensions,
            flake8_copyright: options.flake8_copyright,
            flake8_errmsg: options.flake8_errmsg,
            flake8_gettext: options.flake8_gettext,
            flake8_implicit_str_concat: options.flake8_implicit_str_concat,
            flake8_import_conventions: options.flake8_import_conventions,
            flake8_pytest_style: options.flake8_pytest_style,
            flake8_quotes: options.flake8_quotes,
            flake8_self: options.flake8_self,
            flake8_tidy_imports: options.flake8_tidy_imports,
            flake8_type_checking: options.flake8_type_checking,
            flake8_unused_arguments: options.flake8_unused_arguments,
            isort: options.isort,
            mccabe: options.mccabe,
            pep8_naming: options.pep8_naming,
            pycodestyle: options.pycodestyle,
            pydocstyle: options.pydocstyle,
            pyflakes: options.pyflakes,
            pylint: options.pylint,
            pyupgrade: options.pyupgrade,
        })
    }

    #[must_use]
    pub fn combine(self, config: Self) -> Self {
        Self {
            rule_selections: config
                .rule_selections
                .into_iter()
                .chain(self.rule_selections)
                .collect(),
            allowed_confusables: self.allowed_confusables.or(config.allowed_confusables),
            builtins: self.builtins.or(config.builtins),
            cache_dir: self.cache_dir.or(config.cache_dir),
            dummy_variable_rgx: self.dummy_variable_rgx.or(config.dummy_variable_rgx),
            exclude: self.exclude.or(config.exclude),
            extend: self.extend.or(config.extend),
            extend_exclude: config
                .extend_exclude
                .into_iter()
                .chain(self.extend_exclude)
                .collect(),
            extend_include: config
                .extend_include
                .into_iter()
                .chain(self.extend_include)
                .collect(),
            extend_per_file_ignores: config
                .extend_per_file_ignores
                .into_iter()
                .chain(self.extend_per_file_ignores)
                .collect(),
            external: self.external.or(config.external),
            fix: self.fix.or(config.fix),
            fix_only: self.fix_only.or(config.fix_only),
            format: self.format.or(config.format),
            force_exclude: self.force_exclude.or(config.force_exclude),
            include: self.include.or(config.include),
            ignore_init_module_imports: self
                .ignore_init_module_imports
                .or(config.ignore_init_module_imports),
            line_length: self.line_length.or(config.line_length),
            tab_size: self.tab_size.or(config.tab_size),
            namespace_packages: self.namespace_packages.or(config.namespace_packages),
            per_file_ignores: self.per_file_ignores.or(config.per_file_ignores),
            required_version: self.required_version.or(config.required_version),
            respect_gitignore: self.respect_gitignore.or(config.respect_gitignore),
            show_source: self.show_source.or(config.show_source),
            show_fixes: self.show_fixes.or(config.show_fixes),
            src: self.src.or(config.src),
            target_version: self.target_version.or(config.target_version),
            task_tags: self.task_tags.or(config.task_tags),
            typing_modules: self.typing_modules.or(config.typing_modules),
            // Plugins
            flake8_annotations: self.flake8_annotations.combine(config.flake8_annotations),
            flake8_bandit: self.flake8_bandit.combine(config.flake8_bandit),
            flake8_bugbear: self.flake8_bugbear.combine(config.flake8_bugbear),
            flake8_builtins: self.flake8_builtins.combine(config.flake8_builtins),
            flake8_comprehensions: self
                .flake8_comprehensions
                .combine(config.flake8_comprehensions),
            flake8_copyright: self.flake8_copyright.combine(config.flake8_copyright),
            flake8_errmsg: self.flake8_errmsg.combine(config.flake8_errmsg),
            flake8_gettext: self.flake8_gettext.combine(config.flake8_gettext),
            flake8_implicit_str_concat: self
                .flake8_implicit_str_concat
                .combine(config.flake8_implicit_str_concat),
            flake8_import_conventions: self
                .flake8_import_conventions
                .combine(config.flake8_import_conventions),
            flake8_pytest_style: self.flake8_pytest_style.combine(config.flake8_pytest_style),
            flake8_quotes: self.flake8_quotes.combine(config.flake8_quotes),
            flake8_self: self.flake8_self.combine(config.flake8_self),
            flake8_tidy_imports: self.flake8_tidy_imports.combine(config.flake8_tidy_imports),
            flake8_type_checking: self
                .flake8_type_checking
                .combine(config.flake8_type_checking),
            flake8_unused_arguments: self
                .flake8_unused_arguments
                .combine(config.flake8_unused_arguments),
            isort: self.isort.combine(config.isort),
            mccabe: self.mccabe.combine(config.mccabe),
            pep8_naming: self.pep8_naming.combine(config.pep8_naming),
            pycodestyle: self.pycodestyle.combine(config.pycodestyle),
            pydocstyle: self.pydocstyle.combine(config.pydocstyle),
            pyflakes: self.pyflakes.combine(config.pyflakes),
            pylint: self.pylint.combine(config.pylint),
            pyupgrade: self.pyupgrade.combine(config.pyupgrade),
        }
    }
}

pub trait CombinePluginOptions {
    #[must_use]
    fn combine(self, other: Self) -> Self;
}

impl<T: CombinePluginOptions> CombinePluginOptions for Option<T> {
    fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Some(base), Some(other)) => Some(base.combine(other)),
            (Some(base), None) => Some(base),
            (None, Some(other)) => Some(other),
            (None, None) => None,
        }
    }
}

/// Given a list of source paths, which could include glob patterns, resolve the
/// matching paths.
pub fn resolve_src(src: &[String], project_root: &Path) -> Result<Vec<PathBuf>> {
    let expansions = src
        .iter()
        .map(shellexpand::full)
        .collect::<Result<Vec<Cow<'_, str>>, LookupError<VarError>>>()?;
    let globs = expansions
        .iter()
        .map(|path| Path::new(path.as_ref()))
        .map(|path| fs::normalize_path_to(path, project_root))
        .map(|path| glob(&path.to_string_lossy()))
        .collect::<Result<Vec<Paths>, PatternError>>()?;
    let paths: Vec<PathBuf> = globs
        .into_iter()
        .flatten()
        .collect::<Result<Vec<PathBuf>, GlobError>>()?;
    Ok(paths)
}
