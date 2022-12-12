//! User-provided program settings, taking into account pyproject.toml and
//! command-line options. Structure mirrors the user-facing representation of
//! the various parameters.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use regex::Regex;

use crate::checks_gen::CheckCodePrefix;
use crate::cli::{collect_per_file_ignores, Overrides};
use crate::settings::options::Options;
use crate::settings::pyproject::load_options;
use crate::settings::types::{FilePattern, PerFileIgnore, PythonVersion, SerializationFormat};
use crate::{
    flake8_annotations, flake8_bugbear, flake8_import_conventions, flake8_quotes,
    flake8_tidy_imports, fs, isort, mccabe, pep8_naming, pyupgrade,
};

#[derive(Debug, Default)]
pub struct Configuration {
    pub allowed_confusables: Option<Vec<char>>,
    pub dummy_variable_rgx: Option<Regex>,
    pub exclude: Option<Vec<FilePattern>>,
    pub extend: Option<PathBuf>,
    pub extend_exclude: Option<Vec<FilePattern>>,
    pub extend_ignore: Option<Vec<CheckCodePrefix>>,
    pub extend_select: Option<Vec<CheckCodePrefix>>,
    pub external: Option<Vec<String>>,
    pub fix: Option<bool>,
    pub fixable: Option<Vec<CheckCodePrefix>>,
    pub format: Option<SerializationFormat>,
    pub ignore: Option<Vec<CheckCodePrefix>>,
    pub ignore_init_module_imports: Option<bool>,
    pub line_length: Option<usize>,
    pub per_file_ignores: Option<Vec<PerFileIgnore>>,
    pub select: Option<Vec<CheckCodePrefix>>,
    pub show_source: Option<bool>,
    pub src: Option<Vec<PathBuf>>,
    pub target_version: Option<PythonVersion>,
    pub unfixable: Option<Vec<CheckCodePrefix>>,
    // Plugins
    pub flake8_annotations: Option<flake8_annotations::settings::Options>,
    pub flake8_bugbear: Option<flake8_bugbear::settings::Options>,
    pub flake8_import_conventions: Option<flake8_import_conventions::settings::Options>,
    pub flake8_quotes: Option<flake8_quotes::settings::Options>,
    pub flake8_tidy_imports: Option<flake8_tidy_imports::settings::Options>,
    pub isort: Option<isort::settings::Options>,
    pub mccabe: Option<mccabe::settings::Options>,
    pub pep8_naming: Option<pep8_naming::settings::Options>,
    pub pyupgrade: Option<pyupgrade::settings::Options>,
}

impl Configuration {
    pub fn from_pyproject(pyproject: &Path, project_root: &Path) -> Result<Self> {
        Self::from_options(load_options(pyproject)?, project_root)
    }

    pub fn from_options(options: Options, project_root: &Path) -> Result<Self> {
        Ok(Configuration {
            extend: options.extend.map(PathBuf::from),
            allowed_confusables: options.allowed_confusables,
            dummy_variable_rgx: options
                .dummy_variable_rgx
                .map(|pattern| Regex::new(&pattern))
                .transpose()
                .map_err(|e| anyhow!("Invalid `dummy-variable-rgx` value: {e}"))?,
            src: options.src.map(|src| {
                src.iter()
                    .map(|path| fs::normalize_path_to(Path::new(path), project_root))
                    .collect()
            }),
            target_version: options.target_version,
            exclude: options.exclude.map(|paths| {
                paths
                    .into_iter()
                    .map(|pattern| {
                        let absolute = fs::normalize_path_to(Path::new(&pattern), project_root);
                        FilePattern::User(pattern, absolute)
                    })
                    .collect()
            }),
            extend_exclude: options.extend_exclude.map(|paths| {
                paths
                    .into_iter()
                    .map(|pattern| {
                        let absolute = fs::normalize_path_to(Path::new(&pattern), project_root);
                        FilePattern::User(pattern, absolute)
                    })
                    .collect()
            }),
            extend_ignore: options.extend_ignore,
            select: options.select,
            extend_select: options.extend_select,
            external: options.external,
            fix: options.fix,
            fixable: options.fixable,
            unfixable: options.unfixable,
            format: options.format,
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
            show_source: options.show_source,
            // Plugins
            flake8_annotations: options.flake8_annotations,
            flake8_bugbear: options.flake8_bugbear,
            flake8_import_conventions: options.flake8_import_conventions,
            flake8_quotes: options.flake8_quotes,
            flake8_tidy_imports: options.flake8_tidy_imports,
            isort: options.isort,
            mccabe: options.mccabe,
            pep8_naming: options.pep8_naming,
            pyupgrade: options.pyupgrade,
        })
    }

    #[must_use]
    pub fn combine(self, config: Configuration) -> Self {
        Self {
            allowed_confusables: config.allowed_confusables.or(self.allowed_confusables),
            dummy_variable_rgx: config.dummy_variable_rgx.or(self.dummy_variable_rgx),
            exclude: config.exclude.or(self.exclude),
            extend: config.extend.or(self.extend),
            extend_exclude: config.extend_exclude.or(self.extend_exclude),
            extend_ignore: config.extend_ignore.or(self.extend_ignore),
            extend_select: config.extend_select.or(self.extend_select),
            external: config.external.or(self.external),
            fix: config.fix.or(self.fix),
            fixable: config.fixable.or(self.fixable),
            format: config.format.or(self.format),
            ignore: config.ignore.or(self.ignore),
            ignore_init_module_imports: config
                .ignore_init_module_imports
                .or(self.ignore_init_module_imports),
            line_length: config.line_length.or(self.line_length),
            per_file_ignores: config.per_file_ignores.or(self.per_file_ignores),
            select: config.select.or(self.select),
            show_source: config.show_source.or(self.show_source),
            src: config.src.or(self.src),
            target_version: config.target_version.or(self.target_version),
            unfixable: config.unfixable.or(self.unfixable),
            // Plugins
            flake8_annotations: config.flake8_annotations.or(self.flake8_annotations),
            flake8_bugbear: config.flake8_bugbear.or(self.flake8_bugbear),
            flake8_import_conventions: config
                .flake8_import_conventions
                .or(self.flake8_import_conventions),
            flake8_quotes: config.flake8_quotes.or(self.flake8_quotes),
            flake8_tidy_imports: config.flake8_tidy_imports.or(self.flake8_tidy_imports),
            isort: config.isort.or(self.isort),
            mccabe: config.mccabe.or(self.mccabe),
            pep8_naming: config.pep8_naming.or(self.pep8_naming),
            pyupgrade: config.pyupgrade.or(self.pyupgrade),
        }
    }

    pub fn apply(&mut self, overrides: Overrides) {
        if let Some(dummy_variable_rgx) = overrides.dummy_variable_rgx {
            self.dummy_variable_rgx = Some(dummy_variable_rgx);
        }
        if let Some(exclude) = overrides.exclude {
            self.exclude = Some(exclude);
        }
        if let Some(extend_exclude) = overrides.extend_exclude {
            self.extend_exclude = Some(extend_exclude);
        }
        if let Some(extend_ignore) = overrides.extend_ignore {
            self.extend_ignore = Some(extend_ignore);
        }
        if let Some(extend_select) = overrides.extend_select {
            self.extend_select = Some(extend_select);
        }
        if let Some(fix) = overrides.fix {
            self.fix = Some(fix);
        }
        if let Some(fixable) = overrides.fixable {
            self.fixable = Some(fixable);
        }
        if let Some(format) = overrides.format {
            self.format = Some(format);
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
    }
}
