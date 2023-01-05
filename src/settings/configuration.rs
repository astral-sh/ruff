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

use crate::cli::{collect_per_file_ignores, Overrides};
use crate::registry_gen::CheckCodePrefix;
use crate::settings::options::Options;
use crate::settings::pyproject::load_options;
use crate::settings::types::{
    FilePattern, PerFileIgnore, PythonVersion, SerializationFormat, Version,
};
use crate::{
    flake8_annotations, flake8_bandit, flake8_bugbear, flake8_errmsg, flake8_import_conventions,
    flake8_pytest_style, flake8_quotes, flake8_tidy_imports, flake8_unused_arguments, fs, isort,
    mccabe, pep8_naming, pydocstyle, pyupgrade,
};

#[derive(Debug, Default)]
pub struct Configuration {
    pub allowed_confusables: Option<Vec<char>>,
    pub cache_dir: Option<PathBuf>,
    pub dummy_variable_rgx: Option<Regex>,
    pub exclude: Option<Vec<FilePattern>>,
    pub extend: Option<PathBuf>,
    pub extend_exclude: Vec<FilePattern>,
    pub extend_ignore: Vec<Vec<CheckCodePrefix>>,
    pub extend_select: Vec<Vec<CheckCodePrefix>>,
    pub external: Option<Vec<String>>,
    pub fix: Option<bool>,
    pub fix_only: Option<bool>,
    pub fixable: Option<Vec<CheckCodePrefix>>,
    pub force_exclude: Option<bool>,
    pub format: Option<SerializationFormat>,
    pub ignore: Option<Vec<CheckCodePrefix>>,
    pub ignore_init_module_imports: Option<bool>,
    pub line_length: Option<usize>,
    pub per_file_ignores: Option<Vec<PerFileIgnore>>,
    pub required_version: Option<Version>,
    pub respect_gitignore: Option<bool>,
    pub select: Option<Vec<CheckCodePrefix>>,
    pub show_source: Option<bool>,
    pub src: Option<Vec<PathBuf>>,
    pub target_version: Option<PythonVersion>,
    pub unfixable: Option<Vec<CheckCodePrefix>>,
    pub update_check: Option<bool>,
    // Plugins
    pub flake8_annotations: Option<flake8_annotations::settings::Options>,
    pub flake8_bandit: Option<flake8_bandit::settings::Options>,
    pub flake8_bugbear: Option<flake8_bugbear::settings::Options>,
    pub flake8_errmsg: Option<flake8_errmsg::settings::Options>,
    pub flake8_import_conventions: Option<flake8_import_conventions::settings::Options>,
    pub flake8_pytest_style: Option<flake8_pytest_style::settings::Options>,
    pub flake8_quotes: Option<flake8_quotes::settings::Options>,
    pub flake8_tidy_imports: Option<flake8_tidy_imports::settings::Options>,
    pub flake8_unused_arguments: Option<flake8_unused_arguments::settings::Options>,
    pub isort: Option<isort::settings::Options>,
    pub mccabe: Option<mccabe::settings::Options>,
    pub pep8_naming: Option<pep8_naming::settings::Options>,
    pub pydocstyle: Option<pydocstyle::settings::Options>,
    pub pyupgrade: Option<pyupgrade::settings::Options>,
}

impl Configuration {
    pub fn from_toml(path: &Path, project_root: &Path) -> Result<Self> {
        Self::from_options(load_options(path)?, project_root)
    }

    pub fn from_options(options: Options, project_root: &Path) -> Result<Self> {
        Ok(Configuration {
            allowed_confusables: options.allowed_confusables,
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
                        let absolute = fs::normalize_path_to(Path::new(&pattern), project_root);
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
                            let absolute = fs::normalize_path_to(Path::new(&pattern), project_root);
                            FilePattern::User(pattern, absolute)
                        })
                        .collect()
                })
                .unwrap_or_default(),
            extend_ignore: vec![options.extend_ignore.unwrap_or_default()],
            extend_select: vec![options.extend_select.unwrap_or_default()],
            external: options.external,
            fix: options.fix,
            fix_only: options.fix_only,
            fixable: options.fixable,
            format: options.format,
            force_exclude: options.force_exclude,
            ignore: options.ignore,
            ignore_init_module_imports: options.ignore_init_module_imports,
            line_length: options.line_length,
            per_file_ignores: options.per_file_ignores.map(|per_file_ignores| {
                per_file_ignores
                    .into_iter()
                    .map(|(pattern, prefixes)| {
                        let absolute = fs::normalize_path_to(Path::new(&pattern), project_root);
                        PerFileIgnore::new(pattern, absolute, &prefixes)
                    })
                    .collect()
            }),
            required_version: options.required_version,
            respect_gitignore: options.respect_gitignore,
            select: options.select,
            show_source: options.show_source,
            src: options
                .src
                .map(|src| resolve_src(&src, project_root))
                .transpose()?,
            target_version: options.target_version,
            unfixable: options.unfixable,
            update_check: options.update_check,
            // Plugins
            flake8_annotations: options.flake8_annotations,
            flake8_bandit: options.flake8_bandit,
            flake8_bugbear: options.flake8_bugbear,
            flake8_errmsg: options.flake8_errmsg,
            flake8_import_conventions: options.flake8_import_conventions,
            flake8_pytest_style: options.flake8_pytest_style,
            flake8_quotes: options.flake8_quotes,
            flake8_tidy_imports: options.flake8_tidy_imports,
            flake8_unused_arguments: options.flake8_unused_arguments,
            isort: options.isort,
            mccabe: options.mccabe,
            pep8_naming: options.pep8_naming,
            pydocstyle: options.pydocstyle,
            pyupgrade: options.pyupgrade,
        })
    }

    #[must_use]
    pub fn combine(self, config: Configuration) -> Self {
        Self {
            allowed_confusables: self.allowed_confusables.or(config.allowed_confusables),
            cache_dir: self.cache_dir.or(config.cache_dir),
            dummy_variable_rgx: self.dummy_variable_rgx.or(config.dummy_variable_rgx),
            exclude: self.exclude.or(config.exclude),
            extend: self.extend.or(config.extend),
            extend_exclude: config
                .extend_exclude
                .into_iter()
                .chain(self.extend_exclude.into_iter())
                .collect(),
            extend_ignore: config
                .extend_ignore
                .into_iter()
                .chain(self.extend_ignore.into_iter())
                .collect(),
            extend_select: config
                .extend_select
                .into_iter()
                .chain(self.extend_select.into_iter())
                .collect(),
            external: self.external.or(config.external),
            fix: self.fix.or(config.fix),
            fix_only: self.fix_only.or(config.fix_only),
            fixable: self.fixable.or(config.fixable),
            format: self.format.or(config.format),
            force_exclude: self.force_exclude.or(config.force_exclude),
            ignore: self.ignore.or(config.ignore),
            ignore_init_module_imports: self
                .ignore_init_module_imports
                .or(config.ignore_init_module_imports),
            line_length: self.line_length.or(config.line_length),
            per_file_ignores: self.per_file_ignores.or(config.per_file_ignores),
            required_version: self.required_version.or(config.required_version),
            respect_gitignore: self.respect_gitignore.or(config.respect_gitignore),
            select: self.select.or(config.select),
            show_source: self.show_source.or(config.show_source),
            src: self.src.or(config.src),
            target_version: self.target_version.or(config.target_version),
            unfixable: self.unfixable.or(config.unfixable),
            update_check: self.update_check.or(config.update_check),
            // Plugins
            flake8_annotations: self.flake8_annotations.or(config.flake8_annotations),
            flake8_bandit: self.flake8_bandit.or(config.flake8_bandit),
            flake8_bugbear: self.flake8_bugbear.or(config.flake8_bugbear),
            flake8_errmsg: self.flake8_errmsg.or(config.flake8_errmsg),
            flake8_import_conventions: self
                .flake8_import_conventions
                .or(config.flake8_import_conventions),
            flake8_pytest_style: self.flake8_pytest_style.or(config.flake8_pytest_style),
            flake8_quotes: self.flake8_quotes.or(config.flake8_quotes),
            flake8_tidy_imports: self.flake8_tidy_imports.or(config.flake8_tidy_imports),
            flake8_unused_arguments: self
                .flake8_unused_arguments
                .or(config.flake8_unused_arguments),
            isort: self.isort.or(config.isort),
            mccabe: self.mccabe.or(config.mccabe),
            pep8_naming: self.pep8_naming.or(config.pep8_naming),
            pydocstyle: self.pydocstyle.or(config.pydocstyle),
            pyupgrade: self.pyupgrade.or(config.pyupgrade),
        }
    }

    pub fn apply(&mut self, overrides: Overrides) {
        if let Some(cache_dir) = overrides.cache_dir {
            self.cache_dir = Some(cache_dir);
        }
        if let Some(dummy_variable_rgx) = overrides.dummy_variable_rgx {
            self.dummy_variable_rgx = Some(dummy_variable_rgx);
        }
        if let Some(exclude) = overrides.exclude {
            self.exclude = Some(exclude);
        }
        if let Some(extend_exclude) = overrides.extend_exclude {
            self.extend_exclude.extend(extend_exclude);
        }
        if let Some(fix) = overrides.fix {
            self.fix = Some(fix);
        }
        if let Some(fix_only) = overrides.fix_only {
            self.fix_only = Some(fix_only);
        }
        if let Some(fixable) = overrides.fixable {
            self.fixable = Some(fixable);
        }
        if let Some(format) = overrides.format {
            self.format = Some(format);
        }
        if let Some(force_exclude) = overrides.force_exclude {
            self.force_exclude = Some(force_exclude);
        }
        if let Some(ignore) = overrides.ignore {
            self.ignore = Some(ignore);
        }
        if let Some(line_length) = overrides.line_length {
            self.line_length = Some(line_length);
        }
        if let Some(max_complexity) = overrides.max_complexity {
            self.mccabe = Some(mccabe::settings::Options {
                max_complexity: Some(max_complexity),
            });
        }
        if let Some(per_file_ignores) = overrides.per_file_ignores {
            self.per_file_ignores = Some(collect_per_file_ignores(per_file_ignores));
        }
        if let Some(respect_gitignore) = overrides.respect_gitignore {
            self.respect_gitignore = Some(respect_gitignore);
        }
        if let Some(select) = overrides.select {
            self.select = Some(select);
        }
        if let Some(show_source) = overrides.show_source {
            self.show_source = Some(show_source);
        }
        if let Some(target_version) = overrides.target_version {
            self.target_version = Some(target_version);
        }
        if let Some(unfixable) = overrides.unfixable {
            self.unfixable = Some(unfixable);
        }
        if let Some(update_check) = overrides.update_check {
            self.update_check = Some(update_check);
        }
        // Special-case: `extend_ignore` and `extend_select` are parallel arrays, so
        // push an empty array if only one of the two is provided.
        match (overrides.extend_ignore, overrides.extend_select) {
            (Some(extend_ignore), Some(extend_select)) => {
                self.extend_ignore.push(extend_ignore);
                self.extend_select.push(extend_select);
            }
            (Some(extend_ignore), None) => {
                self.extend_ignore.push(extend_ignore);
                self.extend_select.push(Vec::new());
            }
            (None, Some(extend_select)) => {
                self.extend_ignore.push(Vec::new());
                self.extend_select.push(extend_select);
            }
            (None, None) => {}
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
