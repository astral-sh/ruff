//! User-provided program settings, taking into account pyproject.toml and
//! command-line options. Structure mirrors the user-facing representation of
//! the various parameters.

use std::borrow::Cow;
use std::env::VarError;
use std::num::{NonZeroU16, NonZeroU8};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use glob::{glob, GlobError, Paths, PatternError};
use regex::Regex;
use ruff_linter::settings::fix_safety_table::FixSafetyTable;
use rustc_hash::{FxHashMap, FxHashSet};
use shellexpand;
use shellexpand::LookupError;
use strum::IntoEnumIterator;

use ruff_cache::cache_dir;
use ruff_formatter::IndentStyle;
use ruff_linter::line_width::{IndentWidth, LineLength};
use ruff_linter::registry::RuleNamespace;
use ruff_linter::registry::{Rule, RuleSet, INCOMPATIBLE_CODES};
use ruff_linter::rule_selector::{PreviewOptions, Specificity};
use ruff_linter::rules::pycodestyle;
use ruff_linter::settings::rule_table::RuleTable;
use ruff_linter::settings::types::{
    ExtensionMapping, FilePattern, FilePatternSet, PerFileIgnore, PreviewMode, PythonVersion,
    SerializationFormat, UnsafeFixes, Version,
};
use ruff_linter::settings::{
    resolve_per_file_ignores, LinterSettings, DEFAULT_SELECTORS, DUMMY_VARIABLE_RGX, TASK_TAGS,
};
use ruff_linter::{
    fs, warn_user, warn_user_once, warn_user_once_by_id, RuleSelector, RUFF_PKG_VERSION,
};
use ruff_python_formatter::{
    DocstringCode, DocstringCodeLineWidth, MagicTrailingComma, QuoteStyle,
};

use crate::options::{
    Flake8AnnotationsOptions, Flake8BanditOptions, Flake8BugbearOptions, Flake8BuiltinsOptions,
    Flake8ComprehensionsOptions, Flake8CopyrightOptions, Flake8ErrMsgOptions, Flake8GetTextOptions,
    Flake8ImplicitStrConcatOptions, Flake8ImportConventionsOptions, Flake8PytestStyleOptions,
    Flake8QuotesOptions, Flake8SelfOptions, Flake8TidyImportsOptions, Flake8TypeCheckingOptions,
    Flake8UnusedArgumentsOptions, FormatOptions, IsortOptions, LintOptions, McCabeOptions, Options,
    Pep8NamingOptions, PyUpgradeOptions, PycodestyleOptions, PydocstyleOptions, PyflakesOptions,
    PylintOptions,
};
use crate::settings::{
    FileResolverSettings, FormatterSettings, LineEnding, Settings, EXCLUDE, INCLUDE,
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

#[derive(Debug, Eq, PartialEq, is_macro::Is)]
pub enum RuleSelectorKind {
    /// Enables the selected rules
    Enable,
    /// Disables the selected rules
    Disable,
    /// Modifies the behavior of selected rules
    Modify,
}

impl RuleSelection {
    pub fn selectors_by_kind(&self) -> impl Iterator<Item = (RuleSelectorKind, &RuleSelector)> {
        self.select
            .iter()
            .flatten()
            .map(|selector| (RuleSelectorKind::Enable, selector))
            .chain(
                self.fixable
                    .iter()
                    .flatten()
                    .map(|selector| (RuleSelectorKind::Modify, selector)),
            )
            .chain(
                self.ignore
                    .iter()
                    .map(|selector| (RuleSelectorKind::Disable, selector)),
            )
            .chain(
                self.extend_select
                    .iter()
                    .map(|selector| (RuleSelectorKind::Enable, selector)),
            )
            .chain(
                self.unfixable
                    .iter()
                    .map(|selector| (RuleSelectorKind::Modify, selector)),
            )
            .chain(
                self.extend_fixable
                    .iter()
                    .map(|selector| (RuleSelectorKind::Modify, selector)),
            )
    }
}

#[derive(Debug, Default)]
pub struct Configuration {
    // Global options
    pub cache_dir: Option<PathBuf>,
    pub extend: Option<PathBuf>,
    pub fix: Option<bool>,
    pub fix_only: Option<bool>,
    pub unsafe_fixes: Option<UnsafeFixes>,
    pub output_format: Option<SerializationFormat>,
    pub preview: Option<PreviewMode>,
    pub required_version: Option<Version>,
    pub show_fixes: Option<bool>,
    pub show_source: Option<bool>,

    // File resolver options
    pub exclude: Option<Vec<FilePattern>>,
    pub extend_exclude: Vec<FilePattern>,
    pub extend_include: Vec<FilePattern>,
    pub force_exclude: Option<bool>,
    pub include: Option<Vec<FilePattern>>,
    pub respect_gitignore: Option<bool>,

    // Generic python options settings
    pub builtins: Option<Vec<String>>,
    pub namespace_packages: Option<Vec<PathBuf>>,
    pub src: Option<Vec<PathBuf>>,
    pub target_version: Option<PythonVersion>,

    // Global formatting options
    pub line_length: Option<LineLength>,
    pub indent_width: Option<IndentWidth>,

    pub lint: LintConfiguration,
    pub format: FormatConfiguration,
}

impl Configuration {
    pub fn into_settings(self, project_root: &Path) -> Result<Settings> {
        if let Some(required_version) = &self.required_version {
            if &**required_version != RUFF_PKG_VERSION {
                return Err(anyhow!(
                    "Required version `{}` does not match the running version `{}`",
                    &**required_version,
                    RUFF_PKG_VERSION
                ));
            }
        }

        let target_version = self.target_version.unwrap_or_default();
        let global_preview = self.preview.unwrap_or_default();

        let format = self.format;
        let format_defaults = FormatterSettings::default();

        let quote_style = format.quote_style.unwrap_or(format_defaults.quote_style);
        let format_preview = match format.preview.unwrap_or(global_preview) {
            PreviewMode::Disabled => ruff_python_formatter::PreviewMode::Disabled,
            PreviewMode::Enabled => ruff_python_formatter::PreviewMode::Enabled,
        };

        if quote_style == QuoteStyle::Preserve && !format_preview.is_enabled() {
            return Err(anyhow!(
                "'quote-style = preserve' is a preview only feature. Run with '--preview' to enable it."
            ));
        }

        let formatter = FormatterSettings {
            exclude: FilePatternSet::try_from_iter(format.exclude.unwrap_or_default())?,
            preview: format_preview,
            target_version: match target_version {
                PythonVersion::Py37 => ruff_python_formatter::PythonVersion::Py37,
                PythonVersion::Py38 => ruff_python_formatter::PythonVersion::Py38,
                PythonVersion::Py39 => ruff_python_formatter::PythonVersion::Py39,
                PythonVersion::Py310 => ruff_python_formatter::PythonVersion::Py310,
                PythonVersion::Py311 => ruff_python_formatter::PythonVersion::Py311,
                PythonVersion::Py312 => ruff_python_formatter::PythonVersion::Py312,
            },
            line_width: self
                .line_length
                .map_or(format_defaults.line_width, |length| {
                    ruff_formatter::LineWidth::from(NonZeroU16::from(length))
                }),
            line_ending: format.line_ending.unwrap_or(format_defaults.line_ending),
            indent_style: format.indent_style.unwrap_or(format_defaults.indent_style),
            indent_width: self
                .indent_width
                .map_or(format_defaults.indent_width, |tab_size| {
                    ruff_formatter::IndentWidth::from(NonZeroU8::from(tab_size))
                }),
            quote_style,
            magic_trailing_comma: format
                .magic_trailing_comma
                .unwrap_or(format_defaults.magic_trailing_comma),
            docstring_code_format: format
                .docstring_code_format
                .unwrap_or(format_defaults.docstring_code_format),
            docstring_code_line_width: format
                .docstring_code_line_width
                .unwrap_or(format_defaults.docstring_code_line_width),
        };

        let lint = self.lint;
        let lint_preview = lint.preview.unwrap_or(global_preview);

        let line_length = self.line_length.unwrap_or_default();

        Ok(Settings {
            cache_dir: self
                .cache_dir
                .clone()
                .unwrap_or_else(|| cache_dir(project_root)),
            fix: self.fix.unwrap_or(false),
            fix_only: self.fix_only.unwrap_or(false),
            unsafe_fixes: self.unsafe_fixes.unwrap_or_default(),
            output_format: self.output_format.unwrap_or_default(),
            show_fixes: self.show_fixes.unwrap_or(false),
            show_source: self.show_source.unwrap_or(false),

            file_resolver: FileResolverSettings {
                exclude: FilePatternSet::try_from_iter(
                    self.exclude.unwrap_or_else(|| EXCLUDE.to_vec()),
                )?,
                extend_exclude: FilePatternSet::try_from_iter(self.extend_exclude)?,
                extend_include: FilePatternSet::try_from_iter(self.extend_include)?,
                force_exclude: self.force_exclude.unwrap_or(false),
                include: FilePatternSet::try_from_iter(
                    self.include.unwrap_or_else(|| INCLUDE.to_vec()),
                )?,
                respect_gitignore: self.respect_gitignore.unwrap_or(true),
                project_root: project_root.to_path_buf(),
            },

            linter: LinterSettings {
                rules: lint.as_rule_table(lint_preview),
                exclude: FilePatternSet::try_from_iter(lint.exclude.unwrap_or_default())?,
                extension: lint.extension.unwrap_or_default(),
                preview: lint_preview,
                target_version,
                project_root: project_root.to_path_buf(),
                allowed_confusables: lint
                    .allowed_confusables
                    .map(FxHashSet::from_iter)
                    .unwrap_or_default(),
                builtins: self.builtins.unwrap_or_default(),
                dummy_variable_rgx: lint
                    .dummy_variable_rgx
                    .unwrap_or_else(|| DUMMY_VARIABLE_RGX.clone()),
                external: lint.external.unwrap_or_default(),
                ignore_init_module_imports: lint.ignore_init_module_imports.unwrap_or_default(),
                line_length,
                tab_size: self.indent_width.unwrap_or_default(),
                namespace_packages: self.namespace_packages.unwrap_or_default(),
                per_file_ignores: resolve_per_file_ignores(
                    lint.per_file_ignores
                        .unwrap_or_default()
                        .into_iter()
                        .chain(lint.extend_per_file_ignores)
                        .collect(),
                )?,

                fix_safety: FixSafetyTable::from_rule_selectors(
                    &lint.extend_safe_fixes,
                    &lint.extend_unsafe_fixes,
                    &PreviewOptions {
                        mode: lint_preview,
                        require_explicit: false,
                    },
                ),

                src: self.src.unwrap_or_else(|| vec![project_root.to_path_buf()]),
                explicit_preview_rules: lint.explicit_preview_rules.unwrap_or_default(),

                task_tags: lint
                    .task_tags
                    .unwrap_or_else(|| TASK_TAGS.iter().map(ToString::to_string).collect()),
                logger_objects: lint.logger_objects.unwrap_or_default(),
                typing_modules: lint.typing_modules.unwrap_or_default(),
                // Plugins
                flake8_annotations: lint
                    .flake8_annotations
                    .map(Flake8AnnotationsOptions::into_settings)
                    .unwrap_or_default(),
                flake8_bandit: lint
                    .flake8_bandit
                    .map(Flake8BanditOptions::into_settings)
                    .unwrap_or_default(),
                flake8_bugbear: lint
                    .flake8_bugbear
                    .map(Flake8BugbearOptions::into_settings)
                    .unwrap_or_default(),
                flake8_builtins: lint
                    .flake8_builtins
                    .map(Flake8BuiltinsOptions::into_settings)
                    .unwrap_or_default(),
                flake8_comprehensions: lint
                    .flake8_comprehensions
                    .map(Flake8ComprehensionsOptions::into_settings)
                    .unwrap_or_default(),
                flake8_copyright: lint
                    .flake8_copyright
                    .map(Flake8CopyrightOptions::try_into_settings)
                    .transpose()?
                    .unwrap_or_default(),
                flake8_errmsg: lint
                    .flake8_errmsg
                    .map(Flake8ErrMsgOptions::into_settings)
                    .unwrap_or_default(),
                flake8_implicit_str_concat: lint
                    .flake8_implicit_str_concat
                    .map(Flake8ImplicitStrConcatOptions::into_settings)
                    .unwrap_or_default(),
                flake8_import_conventions: lint
                    .flake8_import_conventions
                    .map(Flake8ImportConventionsOptions::into_settings)
                    .unwrap_or_default(),
                flake8_pytest_style: lint
                    .flake8_pytest_style
                    .map(Flake8PytestStyleOptions::try_into_settings)
                    .transpose()?
                    .unwrap_or_default(),
                flake8_quotes: lint
                    .flake8_quotes
                    .map(Flake8QuotesOptions::into_settings)
                    .unwrap_or_default(),
                flake8_self: lint
                    .flake8_self
                    .map(Flake8SelfOptions::into_settings)
                    .unwrap_or_default(),
                flake8_tidy_imports: lint
                    .flake8_tidy_imports
                    .map(Flake8TidyImportsOptions::into_settings)
                    .unwrap_or_default(),
                flake8_type_checking: lint
                    .flake8_type_checking
                    .map(Flake8TypeCheckingOptions::into_settings)
                    .unwrap_or_default(),
                flake8_unused_arguments: lint
                    .flake8_unused_arguments
                    .map(Flake8UnusedArgumentsOptions::into_settings)
                    .unwrap_or_default(),
                flake8_gettext: lint
                    .flake8_gettext
                    .map(Flake8GetTextOptions::into_settings)
                    .unwrap_or_default(),
                isort: lint
                    .isort
                    .map(IsortOptions::try_into_settings)
                    .transpose()?
                    .unwrap_or_default(),
                mccabe: lint
                    .mccabe
                    .map(McCabeOptions::into_settings)
                    .unwrap_or_default(),
                pep8_naming: lint
                    .pep8_naming
                    .map(Pep8NamingOptions::try_into_settings)
                    .transpose()?
                    .unwrap_or_default(),
                pycodestyle: if let Some(pycodestyle) = lint.pycodestyle {
                    pycodestyle.into_settings(line_length)
                } else {
                    pycodestyle::settings::Settings {
                        max_line_length: line_length,
                        ..pycodestyle::settings::Settings::default()
                    }
                },
                pydocstyle: lint
                    .pydocstyle
                    .map(PydocstyleOptions::into_settings)
                    .unwrap_or_default(),
                pyflakes: lint
                    .pyflakes
                    .map(PyflakesOptions::into_settings)
                    .unwrap_or_default(),
                pylint: lint
                    .pylint
                    .map(PylintOptions::into_settings)
                    .unwrap_or_default(),
                pyupgrade: lint
                    .pyupgrade
                    .map(PyUpgradeOptions::into_settings)
                    .unwrap_or_default(),
            },

            formatter,
        })
    }

    pub fn from_options(options: Options, project_root: &Path) -> Result<Self> {
        let lint = if let Some(mut lint) = options.lint {
            lint.common = lint.common.combine(options.lint_top_level);
            lint
        } else {
            LintOptions {
                common: options.lint_top_level,
                ..LintOptions::default()
            }
        };

        #[allow(deprecated)]
        let indent_width = {
            if options.tab_size.is_some() {
                warn_user_once!("The `tab-size` option has been renamed to `indent-width` to emphasize that it configures the indentation used by the formatter as well as the tab width. Please update your configuration to use `indent-width = <value>` instead.");
            }

            options.indent_width.or(options.tab_size)
        };

        Ok(Self {
            builtins: options.builtins,
            cache_dir: options
                .cache_dir
                .map(|dir| {
                    let dir = shellexpand::full(&dir);
                    dir.map(|dir| fs::normalize_path_to(dir.as_ref(), project_root))
                })
                .transpose()
                .map_err(|e| anyhow!("Invalid `cache-dir` value: {e}"))?,

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
            include: options.include.map(|paths| {
                paths
                    .into_iter()
                    .map(|pattern| {
                        let absolute = fs::normalize_path_to(&pattern, project_root);
                        FilePattern::User(pattern, absolute)
                    })
                    .collect()
            }),
            fix: options.fix,
            fix_only: options.fix_only,
            unsafe_fixes: options.unsafe_fixes.map(UnsafeFixes::from),
            output_format: options.output_format,
            force_exclude: options.force_exclude,
            line_length: options.line_length,
            indent_width,
            namespace_packages: options
                .namespace_packages
                .map(|namespace_package| resolve_src(&namespace_package, project_root))
                .transpose()?,
            preview: options.preview.map(PreviewMode::from),
            required_version: options.required_version,
            respect_gitignore: options.respect_gitignore,
            show_source: options.show_source,
            show_fixes: options.show_fixes,
            src: options
                .src
                .map(|src| resolve_src(&src, project_root))
                .transpose()?,
            target_version: options.target_version,

            lint: LintConfiguration::from_options(lint, project_root)?,
            format: FormatConfiguration::from_options(
                options.format.unwrap_or_default(),
                project_root,
            )?,
        })
    }

    #[must_use]
    pub fn combine(self, config: Self) -> Self {
        Self {
            builtins: self.builtins.or(config.builtins),
            cache_dir: self.cache_dir.or(config.cache_dir),
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
            include: self.include.or(config.include),
            fix: self.fix.or(config.fix),
            fix_only: self.fix_only.or(config.fix_only),
            unsafe_fixes: self.unsafe_fixes.or(config.unsafe_fixes),
            output_format: self.output_format.or(config.output_format),
            force_exclude: self.force_exclude.or(config.force_exclude),
            line_length: self.line_length.or(config.line_length),
            indent_width: self.indent_width.or(config.indent_width),
            namespace_packages: self.namespace_packages.or(config.namespace_packages),
            required_version: self.required_version.or(config.required_version),
            respect_gitignore: self.respect_gitignore.or(config.respect_gitignore),
            show_source: self.show_source.or(config.show_source),
            show_fixes: self.show_fixes.or(config.show_fixes),
            src: self.src.or(config.src),
            target_version: self.target_version.or(config.target_version),
            preview: self.preview.or(config.preview),

            lint: self.lint.combine(config.lint),
            format: self.format.combine(config.format),
        }
    }
}

#[derive(Debug, Default)]
pub struct LintConfiguration {
    pub exclude: Option<Vec<FilePattern>>,
    pub preview: Option<PreviewMode>,
    pub extension: Option<ExtensionMapping>,

    // Rule selection
    pub extend_per_file_ignores: Vec<PerFileIgnore>,
    pub per_file_ignores: Option<Vec<PerFileIgnore>>,
    pub rule_selections: Vec<RuleSelection>,
    pub explicit_preview_rules: Option<bool>,

    // Fix configuration
    pub extend_unsafe_fixes: Vec<RuleSelector>,
    pub extend_safe_fixes: Vec<RuleSelector>,

    // Global lint settings
    pub allowed_confusables: Option<Vec<char>>,
    pub dummy_variable_rgx: Option<Regex>,
    pub external: Option<Vec<String>>,
    pub ignore_init_module_imports: Option<bool>,
    pub logger_objects: Option<Vec<String>>,
    pub task_tags: Option<Vec<String>>,
    pub typing_modules: Option<Vec<String>>,

    // Plugins
    pub flake8_annotations: Option<Flake8AnnotationsOptions>,
    pub flake8_bandit: Option<Flake8BanditOptions>,
    pub flake8_bugbear: Option<Flake8BugbearOptions>,
    pub flake8_builtins: Option<Flake8BuiltinsOptions>,
    pub flake8_comprehensions: Option<Flake8ComprehensionsOptions>,
    pub flake8_copyright: Option<Flake8CopyrightOptions>,
    pub flake8_errmsg: Option<Flake8ErrMsgOptions>,
    pub flake8_gettext: Option<Flake8GetTextOptions>,
    pub flake8_implicit_str_concat: Option<Flake8ImplicitStrConcatOptions>,
    pub flake8_import_conventions: Option<Flake8ImportConventionsOptions>,
    pub flake8_pytest_style: Option<Flake8PytestStyleOptions>,
    pub flake8_quotes: Option<Flake8QuotesOptions>,
    pub flake8_self: Option<Flake8SelfOptions>,
    pub flake8_tidy_imports: Option<Flake8TidyImportsOptions>,
    pub flake8_type_checking: Option<Flake8TypeCheckingOptions>,
    pub flake8_unused_arguments: Option<Flake8UnusedArgumentsOptions>,
    pub isort: Option<IsortOptions>,
    pub mccabe: Option<McCabeOptions>,
    pub pep8_naming: Option<Pep8NamingOptions>,
    pub pycodestyle: Option<PycodestyleOptions>,
    pub pydocstyle: Option<PydocstyleOptions>,
    pub pyflakes: Option<PyflakesOptions>,
    pub pylint: Option<PylintOptions>,
    pub pyupgrade: Option<PyUpgradeOptions>,
}

impl LintConfiguration {
    fn from_options(options: LintOptions, project_root: &Path) -> Result<Self> {
        #[allow(deprecated)]
        let ignore = options
            .common
            .ignore
            .into_iter()
            .flatten()
            .chain(options.common.extend_ignore.into_iter().flatten())
            .collect();
        #[allow(deprecated)]
        let unfixable = options
            .common
            .unfixable
            .into_iter()
            .flatten()
            .chain(options.common.extend_unfixable.into_iter().flatten())
            .collect();
        Ok(LintConfiguration {
            // `--extension` is a hidden command-line argument that isn't supported in configuration
            // files at present.
            extension: None,
            exclude: options.exclude.map(|paths| {
                paths
                    .into_iter()
                    .map(|pattern| {
                        let absolute = fs::normalize_path_to(&pattern, project_root);
                        FilePattern::User(pattern, absolute)
                    })
                    .collect()
            }),
            preview: options.preview.map(PreviewMode::from),

            rule_selections: vec![RuleSelection {
                select: options.common.select,
                ignore,
                extend_select: options.common.extend_select.unwrap_or_default(),
                fixable: options.common.fixable,
                unfixable,
                extend_fixable: options.common.extend_fixable.unwrap_or_default(),
            }],
            extend_safe_fixes: options.common.extend_safe_fixes.unwrap_or_default(),
            extend_unsafe_fixes: options.common.extend_unsafe_fixes.unwrap_or_default(),
            allowed_confusables: options.common.allowed_confusables,
            dummy_variable_rgx: options
                .common
                .dummy_variable_rgx
                .map(|pattern| Regex::new(&pattern))
                .transpose()
                .map_err(|e| anyhow!("Invalid `dummy-variable-rgx` value: {e}"))?,
            extend_per_file_ignores: options
                .common
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
            external: options.common.external,
            ignore_init_module_imports: options.common.ignore_init_module_imports,
            explicit_preview_rules: options.common.explicit_preview_rules,
            per_file_ignores: options.common.per_file_ignores.map(|per_file_ignores| {
                per_file_ignores
                    .into_iter()
                    .map(|(pattern, prefixes)| {
                        PerFileIgnore::new(pattern, &prefixes, Some(project_root))
                    })
                    .collect()
            }),
            task_tags: options.common.task_tags,
            logger_objects: options.common.logger_objects,
            typing_modules: options.common.typing_modules,
            // Plugins
            flake8_annotations: options.common.flake8_annotations,
            flake8_bandit: options.common.flake8_bandit,
            flake8_bugbear: options.common.flake8_bugbear,
            flake8_builtins: options.common.flake8_builtins,
            flake8_comprehensions: options.common.flake8_comprehensions,
            flake8_copyright: options.common.flake8_copyright,
            flake8_errmsg: options.common.flake8_errmsg,
            flake8_gettext: options.common.flake8_gettext,
            flake8_implicit_str_concat: options.common.flake8_implicit_str_concat,
            flake8_import_conventions: options.common.flake8_import_conventions,
            flake8_pytest_style: options.common.flake8_pytest_style,
            flake8_quotes: options.common.flake8_quotes,
            flake8_self: options.common.flake8_self,
            flake8_tidy_imports: options.common.flake8_tidy_imports,
            flake8_type_checking: options.common.flake8_type_checking,
            flake8_unused_arguments: options.common.flake8_unused_arguments,
            isort: options.common.isort,
            mccabe: options.common.mccabe,
            pep8_naming: options.common.pep8_naming,
            pycodestyle: options.common.pycodestyle,
            pydocstyle: options.common.pydocstyle,
            pyflakes: options.common.pyflakes,
            pylint: options.common.pylint,
            pyupgrade: options.common.pyupgrade,
        })
    }

    fn as_rule_table(&self, preview: PreviewMode) -> RuleTable {
        let preview = PreviewOptions {
            mode: preview,
            require_explicit: self.explicit_preview_rules.unwrap_or_default(),
        };

        // The select_set keeps track of which rules have been selected.
        let mut select_set: RuleSet = DEFAULT_SELECTORS
            .iter()
            .flat_map(|selector| selector.rules(&preview))
            .collect();

        // The fixable set keeps track of which rules are fixable.
        let mut fixable_set: RuleSet = RuleSelector::All.rules(&preview).collect();

        // Ignores normally only subtract from the current set of selected
        // rules.  By that logic the ignore in `select = [], ignore = ["E501"]`
        // would be effectless. Instead we carry over the ignores to the next
        // selection in that case, creating a way for ignores to be reused
        // across config files (which otherwise wouldn't be possible since ruff
        // only has `extended` but no `extended-by`).
        let mut carryover_ignores: Option<&[RuleSelector]> = None;
        let mut carryover_unfixables: Option<&[RuleSelector]> = None;

        // Store selectors for displaying warnings
        let mut redirects = FxHashMap::default();
        let mut deprecated_nursery_selectors = FxHashSet::default();
        let mut ignored_preview_selectors = FxHashSet::default();

        // Track which docstring rules are specifically enabled
        // which lets us override the docstring convention ignore-list
        let mut docstring_overrides: FxHashSet<Rule> = FxHashSet::default();

        for selection in &self.rule_selections {
            // If a selection only specifies extend-select we cannot directly
            // apply its rule selectors to the select_set because we firstly have
            // to resolve the effectively selected rules within the current rule selection
            // (taking specificity into account since more specific selectors take
            // precedence over less specific selectors within a rule selection).
            // We do this via the following HashMap where the bool indicates
            // whether to enable or disable the given rule.
            let mut select_map_updates: FxHashMap<Rule, bool> = FxHashMap::default();
            let mut fixable_map_updates: FxHashMap<Rule, bool> = FxHashMap::default();

            let mut docstring_override_updates: FxHashSet<Rule> = FxHashSet::default();

            let carriedover_ignores = carryover_ignores.take();
            let carriedover_unfixables = carryover_unfixables.take();

            for spec in Specificity::iter() {
                // Iterate over rule selectors in order of specificity.
                for selector in selection
                    .select
                    .iter()
                    .flatten()
                    .chain(selection.extend_select.iter())
                    .filter(|s| s.specificity() == spec)
                {
                    for rule in selector.rules(&preview) {
                        select_map_updates.insert(rule, true);

                        if spec == Specificity::Rule {
                            docstring_override_updates.insert(rule);
                        }
                    }
                }
                for selector in selection
                    .ignore
                    .iter()
                    .chain(carriedover_ignores.into_iter().flatten())
                    .filter(|s| s.specificity() == spec)
                {
                    for rule in selector.rules(&preview) {
                        select_map_updates.insert(rule, false);
                    }
                }

                // Apply the same logic to `fixable` and `unfixable`.
                for selector in selection
                    .fixable
                    .iter()
                    .flatten()
                    .chain(selection.extend_fixable.iter())
                    .filter(|s| s.specificity() == spec)
                {
                    for rule in selector.rules(&preview) {
                        fixable_map_updates.insert(rule, true);
                    }
                }
                for selector in selection
                    .unfixable
                    .iter()
                    .chain(carriedover_unfixables.into_iter().flatten())
                    .filter(|s| s.specificity() == spec)
                {
                    for rule in selector.rules(&preview) {
                        fixable_map_updates.insert(rule, false);
                    }
                }
            }

            if let Some(select) = &selection.select {
                // If the `select` option is given we reassign the whole select_set
                // (overriding everything that has been defined previously).
                select_set = select_map_updates
                    .into_iter()
                    .filter_map(|(rule, enabled)| enabled.then_some(rule))
                    .collect();

                if select.is_empty()
                    && selection.extend_select.is_empty()
                    && !selection.ignore.is_empty()
                {
                    carryover_ignores = Some(&selection.ignore);
                }

                docstring_overrides = docstring_override_updates;
            } else {
                // Otherwise we apply the updates on top of the existing select_set.
                for (rule, enabled) in select_map_updates {
                    if enabled {
                        select_set.insert(rule);
                    } else {
                        select_set.remove(rule);
                    }
                }

                for rule in docstring_override_updates {
                    docstring_overrides.insert(rule);
                }
            }

            // Apply the same logic to `fixable` and `unfixable`.
            if let Some(fixable) = &selection.fixable {
                fixable_set = fixable_map_updates
                    .into_iter()
                    .filter_map(|(rule, enabled)| enabled.then_some(rule))
                    .collect();

                if fixable.is_empty()
                    && selection.extend_fixable.is_empty()
                    && !selection.unfixable.is_empty()
                {
                    carryover_unfixables = Some(&selection.unfixable);
                }
            } else {
                for (rule, enabled) in fixable_map_updates {
                    if enabled {
                        fixable_set.insert(rule);
                    } else {
                        fixable_set.remove(rule);
                    }
                }
            }

            // Check for selections that require a warning
            for (kind, selector) in selection.selectors_by_kind() {
                #[allow(deprecated)]
                if matches!(selector, RuleSelector::Nursery) {
                    let suggestion = if preview.mode.is_disabled() {
                        " Use the `--preview` flag instead."
                    } else {
                        // We have no suggested alternative since there is intentionally no "PREVIEW" selector
                        ""
                    };
                    warn_user_once!("The `NURSERY` selector has been deprecated.{suggestion}");
                };

                // Only warn for the following selectors if used to enable rules
                // e.g. use with `--ignore` or `--fixable` is okay
                if preview.mode.is_disabled() && kind.is_enable() {
                    if let RuleSelector::Rule { prefix, .. } = selector {
                        if prefix.rules().any(|rule| rule.is_nursery()) {
                            deprecated_nursery_selectors.insert(selector);
                        }
                    }

                    // Check if the selector is empty because preview mode is disabled
                    if selector.rules(&PreviewOptions::default()).next().is_none() {
                        ignored_preview_selectors.insert(selector);
                    }
                }

                if let RuleSelector::Prefix {
                    prefix,
                    redirected_from: Some(redirect_from),
                } = selector
                {
                    redirects.insert(redirect_from, prefix);
                }
            }
        }

        for (from, target) in redirects {
            // TODO(martin): This belongs into the ruff_cli crate.
            warn_user_once_by_id!(
                from,
                "`{from}` has been remapped to `{}{}`.",
                target.linter().common_prefix(),
                target.short_code()
            );
        }

        for selection in deprecated_nursery_selectors {
            let (prefix, code) = selection.prefix_and_code();
            warn_user!("Selection of nursery rule `{prefix}{code}` without the `--preview` flag is deprecated.",);
        }

        for selection in ignored_preview_selectors {
            let (prefix, code) = selection.prefix_and_code();
            warn_user!(
                "Selection `{prefix}{code}` has no effect because the `--preview` flag was not included.",
            );
        }

        let mut rules = RuleTable::empty();

        for rule in select_set {
            let fix = fixable_set.contains(rule);
            rules.enable(rule, fix);
        }

        // If a docstring convention is specified, disable any incompatible error
        // codes unless we are specifically overridden.
        if let Some(convention) = self
            .pydocstyle
            .as_ref()
            .and_then(|pydocstyle| pydocstyle.convention)
        {
            for rule in convention.rules_to_be_ignored() {
                if !docstring_overrides.contains(rule) {
                    rules.disable(*rule);
                }
            }
        }

        // Validate that we didn't enable any incompatible rules. Use this awkward
        // approach to give each pair it's own `warn_user_once`.
        for (preferred, expendable, message) in INCOMPATIBLE_CODES {
            if rules.enabled(*preferred) && rules.enabled(*expendable) {
                warn_user_once_by_id!(expendable.as_ref(), "{}", message);
                rules.disable(*expendable);
            }
        }

        rules
    }

    #[must_use]
    pub fn combine(self, config: Self) -> Self {
        Self {
            exclude: self.exclude.or(config.exclude),
            preview: self.preview.or(config.preview),
            extension: self.extension.or(config.extension),
            rule_selections: config
                .rule_selections
                .into_iter()
                .chain(self.rule_selections)
                .collect(),
            extend_safe_fixes: config
                .extend_safe_fixes
                .into_iter()
                .chain(self.extend_safe_fixes)
                .collect(),
            extend_unsafe_fixes: config
                .extend_unsafe_fixes
                .into_iter()
                .chain(self.extend_unsafe_fixes)
                .collect(),
            allowed_confusables: self.allowed_confusables.or(config.allowed_confusables),
            dummy_variable_rgx: self.dummy_variable_rgx.or(config.dummy_variable_rgx),
            extend_per_file_ignores: config
                .extend_per_file_ignores
                .into_iter()
                .chain(self.extend_per_file_ignores)
                .collect(),
            external: self.external.or(config.external),
            ignore_init_module_imports: self
                .ignore_init_module_imports
                .or(config.ignore_init_module_imports),
            logger_objects: self.logger_objects.or(config.logger_objects),
            per_file_ignores: self.per_file_ignores.or(config.per_file_ignores),
            explicit_preview_rules: self
                .explicit_preview_rules
                .or(config.explicit_preview_rules),
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

#[derive(Debug, Default)]
pub struct FormatConfiguration {
    pub exclude: Option<Vec<FilePattern>>,
    pub preview: Option<PreviewMode>,

    pub indent_style: Option<IndentStyle>,
    pub quote_style: Option<QuoteStyle>,
    pub magic_trailing_comma: Option<MagicTrailingComma>,
    pub line_ending: Option<LineEnding>,
    pub docstring_code_format: Option<DocstringCode>,
    pub docstring_code_line_width: Option<DocstringCodeLineWidth>,
}

impl FormatConfiguration {
    #[allow(clippy::needless_pass_by_value)]
    pub fn from_options(options: FormatOptions, project_root: &Path) -> Result<Self> {
        Ok(Self {
            exclude: options.exclude.map(|paths| {
                paths
                    .into_iter()
                    .map(|pattern| {
                        let absolute = fs::normalize_path_to(&pattern, project_root);
                        FilePattern::User(pattern, absolute)
                    })
                    .collect()
            }),
            preview: options.preview.map(PreviewMode::from),
            indent_style: options.indent_style,
            quote_style: options.quote_style,
            magic_trailing_comma: options.skip_magic_trailing_comma.map(|skip| {
                if skip {
                    MagicTrailingComma::Ignore
                } else {
                    MagicTrailingComma::Respect
                }
            }),
            line_ending: options.line_ending,
            docstring_code_format: options.docstring_code_format.map(|yes| {
                if yes {
                    DocstringCode::Enabled
                } else {
                    DocstringCode::Disabled
                }
            }),
            docstring_code_line_width: options.docstring_code_line_length,
        })
    }

    #[must_use]
    #[allow(clippy::needless_pass_by_value)]
    pub fn combine(self, other: Self) -> Self {
        Self {
            exclude: self.exclude.or(other.exclude),
            preview: self.preview.or(other.preview),
            indent_style: self.indent_style.or(other.indent_style),
            quote_style: self.quote_style.or(other.quote_style),
            magic_trailing_comma: self.magic_trailing_comma.or(other.magic_trailing_comma),
            line_ending: self.line_ending.or(other.line_ending),
            docstring_code_format: self.docstring_code_format.or(other.docstring_code_format),
            docstring_code_line_width: self
                .docstring_code_line_width
                .or(other.docstring_code_line_width),
        }
    }
}
pub(crate) trait CombinePluginOptions {
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

#[cfg(test)]
mod tests {
    use crate::configuration::{LintConfiguration, RuleSelection};
    use crate::options::PydocstyleOptions;
    use ruff_linter::codes::{Flake8Copyright, Pycodestyle, Refurb};
    use ruff_linter::registry::{Linter, Rule, RuleSet};
    use ruff_linter::rule_selector::PreviewOptions;
    use ruff_linter::settings::types::PreviewMode;
    use ruff_linter::RuleSelector;
    use std::str::FromStr;

    const NURSERY_RULES: &[Rule] = &[
        Rule::MissingCopyrightNotice,
        Rule::IndentationWithInvalidMultiple,
        Rule::NoIndentedBlock,
        Rule::UnexpectedIndentation,
        Rule::IndentationWithInvalidMultipleComment,
        Rule::NoIndentedBlockComment,
        Rule::UnexpectedIndentationComment,
        Rule::OverIndented,
        Rule::WhitespaceAfterOpenBracket,
        Rule::WhitespaceBeforeCloseBracket,
        Rule::WhitespaceBeforePunctuation,
        Rule::WhitespaceBeforeParameters,
        Rule::MultipleSpacesBeforeOperator,
        Rule::MultipleSpacesAfterOperator,
        Rule::TabBeforeOperator,
        Rule::TabAfterOperator,
        Rule::MissingWhitespaceAroundOperator,
        Rule::MissingWhitespaceAroundArithmeticOperator,
        Rule::MissingWhitespaceAroundBitwiseOrShiftOperator,
        Rule::MissingWhitespaceAroundModuloOperator,
        Rule::MissingWhitespace,
        Rule::MultipleSpacesAfterComma,
        Rule::TabAfterComma,
        Rule::UnexpectedSpacesAroundKeywordParameterEquals,
        Rule::MissingWhitespaceAroundParameterEquals,
        Rule::TooFewSpacesBeforeInlineComment,
        Rule::NoSpaceAfterInlineComment,
        Rule::NoSpaceAfterBlockComment,
        Rule::MultipleLeadingHashesForBlockComment,
        Rule::MultipleSpacesAfterKeyword,
        Rule::MultipleSpacesBeforeKeyword,
        Rule::TabAfterKeyword,
        Rule::TabBeforeKeyword,
        Rule::MissingWhitespaceAfterKeyword,
        Rule::CompareToEmptyString,
        Rule::NoSelfUse,
        Rule::EqWithoutHash,
        Rule::BadDunderMethodName,
        Rule::RepeatedAppend,
        Rule::DeleteFullSlice,
        Rule::CheckAndRemoveFromSet,
        Rule::QuadraticListSummation,
    ];

    const PREVIEW_RULES: &[Rule] = &[
        Rule::AndOrTernary,
        Rule::AssignmentInAssert,
        Rule::DirectLoggerInstantiation,
        Rule::InvalidGetLoggerArgument,
        Rule::IsinstanceTypeNone,
        Rule::IfExprMinMax,
        Rule::ManualDictComprehension,
        Rule::ReimplementedStarmap,
        Rule::SliceCopy,
        Rule::TooManyPublicMethods,
        Rule::TooManyPublicMethods,
        Rule::UndocumentedWarn,
        Rule::UnnecessaryEnumerate,
        Rule::MathConstant,
    ];

    #[allow(clippy::needless_pass_by_value)]
    fn resolve_rules(
        selections: impl IntoIterator<Item = RuleSelection>,
        preview: Option<PreviewOptions>,
    ) -> RuleSet {
        LintConfiguration {
            rule_selections: selections.into_iter().collect(),
            explicit_preview_rules: preview.as_ref().map(|preview| preview.require_explicit),
            ..LintConfiguration::default()
        }
        .as_rule_table(preview.map(|preview| preview.mode).unwrap_or_default())
        .iter_enabled()
        // Filter out rule gated behind `#[cfg(feature = "unreachable-code")]`, which is off-by-default
        .filter(|rule| rule.noqa_code() != "RUF014")
        .collect()
    }

    #[test]
    fn select_linter() {
        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Linter::Pycodestyle.into()]),
                ..RuleSelection::default()
            }],
            None,
        );

        let expected = RuleSet::from_rules(&[
            Rule::MixedSpacesAndTabs,
            Rule::MultipleImportsOnOneLine,
            Rule::ModuleImportNotAtTopOfFile,
            Rule::LineTooLong,
            Rule::MultipleStatementsOnOneLineColon,
            Rule::MultipleStatementsOnOneLineSemicolon,
            Rule::UselessSemicolon,
            Rule::NoneComparison,
            Rule::TrueFalseComparison,
            Rule::NotInTest,
            Rule::NotIsTest,
            Rule::TypeComparison,
            Rule::BareExcept,
            Rule::LambdaAssignment,
            Rule::AmbiguousVariableName,
            Rule::AmbiguousClassName,
            Rule::AmbiguousFunctionName,
            Rule::IOError,
            Rule::SyntaxError,
            Rule::TabIndentation,
            Rule::TrailingWhitespace,
            Rule::MissingNewlineAtEndOfFile,
            Rule::BlankLineWithWhitespace,
            Rule::DocLineTooLong,
            Rule::InvalidEscapeSequence,
        ]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn select_one_char_prefix() {
        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Pycodestyle::W.into()]),
                ..RuleSelection::default()
            }],
            None,
        );

        let expected = RuleSet::from_rules(&[
            Rule::TrailingWhitespace,
            Rule::MissingNewlineAtEndOfFile,
            Rule::BlankLineWithWhitespace,
            Rule::DocLineTooLong,
            Rule::InvalidEscapeSequence,
            Rule::TabIndentation,
        ]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn select_two_char_prefix() {
        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Pycodestyle::W6.into()]),
                ..RuleSelection::default()
            }],
            None,
        );
        let expected = RuleSet::from_rule(Rule::InvalidEscapeSequence);
        assert_eq!(actual, expected);
    }

    #[test]
    fn select_prefix_ignore_code() {
        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Pycodestyle::W.into()]),
                ignore: vec![Pycodestyle::W292.into()],
                ..RuleSelection::default()
            }],
            None,
        );
        let expected = RuleSet::from_rules(&[
            Rule::TrailingWhitespace,
            Rule::BlankLineWithWhitespace,
            Rule::DocLineTooLong,
            Rule::InvalidEscapeSequence,
            Rule::TabIndentation,
        ]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn select_code_ignore_prefix() {
        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Pycodestyle::W292.into()]),
                ignore: vec![Pycodestyle::W.into()],
                ..RuleSelection::default()
            }],
            None,
        );
        let expected = RuleSet::from_rule(Rule::MissingNewlineAtEndOfFile);
        assert_eq!(actual, expected);
    }

    #[test]
    fn select_code_ignore_code() {
        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Pycodestyle::W605.into()]),
                ignore: vec![Pycodestyle::W605.into()],
                ..RuleSelection::default()
            }],
            None,
        );
        let expected = RuleSet::empty();
        assert_eq!(actual, expected);
    }

    #[test]
    fn select_prefix_ignore_code_then_extend_select_code() {
        let actual = resolve_rules(
            [
                RuleSelection {
                    select: Some(vec![Pycodestyle::W.into()]),
                    ignore: vec![Pycodestyle::W292.into()],
                    ..RuleSelection::default()
                },
                RuleSelection {
                    extend_select: vec![Pycodestyle::W292.into()],
                    ..RuleSelection::default()
                },
            ],
            None,
        );
        let expected = RuleSet::from_rules(&[
            Rule::TrailingWhitespace,
            Rule::MissingNewlineAtEndOfFile,
            Rule::BlankLineWithWhitespace,
            Rule::DocLineTooLong,
            Rule::InvalidEscapeSequence,
            Rule::TabIndentation,
        ]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn select_prefix_ignore_code_then_extend_select_code_ignore_prefix() {
        let actual = resolve_rules(
            [
                RuleSelection {
                    select: Some(vec![Pycodestyle::W.into()]),
                    ignore: vec![Pycodestyle::W292.into()],
                    ..RuleSelection::default()
                },
                RuleSelection {
                    extend_select: vec![Pycodestyle::W292.into()],
                    ignore: vec![Pycodestyle::W.into()],
                    ..RuleSelection::default()
                },
            ],
            None,
        );
        let expected = RuleSet::from_rule(Rule::MissingNewlineAtEndOfFile);
        assert_eq!(actual, expected);
    }

    #[test]
    fn ignore_code_then_select_prefix() {
        let actual = resolve_rules(
            [
                RuleSelection {
                    select: Some(vec![]),
                    ignore: vec![Pycodestyle::W292.into()],
                    ..RuleSelection::default()
                },
                RuleSelection {
                    select: Some(vec![Pycodestyle::W.into()]),
                    ..RuleSelection::default()
                },
            ],
            None,
        );
        let expected = RuleSet::from_rules(&[
            Rule::TrailingWhitespace,
            Rule::BlankLineWithWhitespace,
            Rule::DocLineTooLong,
            Rule::InvalidEscapeSequence,
            Rule::TabIndentation,
        ]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn ignore_code_then_select_prefix_ignore_code() {
        let actual = resolve_rules(
            [
                RuleSelection {
                    select: Some(vec![]),
                    ignore: vec![Pycodestyle::W292.into()],
                    ..RuleSelection::default()
                },
                RuleSelection {
                    select: Some(vec![Pycodestyle::W.into()]),
                    ignore: vec![Pycodestyle::W505.into()],
                    ..RuleSelection::default()
                },
            ],
            None,
        );
        let expected = RuleSet::from_rules(&[
            Rule::TrailingWhitespace,
            Rule::BlankLineWithWhitespace,
            Rule::InvalidEscapeSequence,
            Rule::TabIndentation,
        ]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn select_all_preview() {
        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![RuleSelector::All]),
                ..RuleSelection::default()
            }],
            Some(PreviewOptions {
                mode: PreviewMode::Disabled,
                ..PreviewOptions::default()
            }),
        );
        assert!(!actual.intersects(&RuleSet::from_rules(PREVIEW_RULES)));

        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![RuleSelector::All]),
                ..RuleSelection::default()
            }],
            Some(PreviewOptions {
                mode: PreviewMode::Enabled,
                ..PreviewOptions::default()
            }),
        );
        assert!(actual.intersects(&RuleSet::from_rules(PREVIEW_RULES)));
    }

    #[test]
    fn select_linter_preview() {
        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Linter::Flake8Copyright.into()]),
                ..RuleSelection::default()
            }],
            Some(PreviewOptions {
                mode: PreviewMode::Disabled,
                ..PreviewOptions::default()
            }),
        );
        let expected = RuleSet::empty();
        assert_eq!(actual, expected);

        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Linter::Flake8Copyright.into()]),
                ..RuleSelection::default()
            }],
            Some(PreviewOptions {
                mode: PreviewMode::Enabled,
                ..PreviewOptions::default()
            }),
        );
        let expected = RuleSet::from_rule(Rule::MissingCopyrightNotice);
        assert_eq!(actual, expected);
    }

    #[test]
    fn select_prefix_preview() {
        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Flake8Copyright::_0.into()]),
                ..RuleSelection::default()
            }],
            Some(PreviewOptions {
                mode: PreviewMode::Disabled,
                ..PreviewOptions::default()
            }),
        );
        let expected = RuleSet::empty();
        assert_eq!(actual, expected);

        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Flake8Copyright::_0.into()]),
                ..RuleSelection::default()
            }],
            Some(PreviewOptions {
                mode: PreviewMode::Enabled,
                ..PreviewOptions::default()
            }),
        );
        let expected = RuleSet::from_rule(Rule::MissingCopyrightNotice);
        assert_eq!(actual, expected);
    }

    #[test]
    fn select_rule_preview() {
        // Test inclusion when toggling preview on and off
        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Refurb::_145.into()]),
                ..RuleSelection::default()
            }],
            Some(PreviewOptions {
                mode: PreviewMode::Disabled,
                ..PreviewOptions::default()
            }),
        );
        let expected = RuleSet::empty();
        assert_eq!(actual, expected);

        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Refurb::_145.into()]),
                ..RuleSelection::default()
            }],
            Some(PreviewOptions {
                mode: PreviewMode::Enabled,
                ..PreviewOptions::default()
            }),
        );
        let expected = RuleSet::from_rule(Rule::SliceCopy);
        assert_eq!(actual, expected);

        // Test inclusion when preview is on but explicit codes are required
        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Refurb::_145.into()]),
                ..RuleSelection::default()
            }],
            Some(PreviewOptions {
                mode: PreviewMode::Enabled,
                require_explicit: true,
            }),
        );
        let expected = RuleSet::from_rule(Rule::SliceCopy);
        assert_eq!(actual, expected);
    }

    #[test]
    fn nursery_select_code() {
        // Backwards compatible behavior allows selection of nursery rules with their exact code
        // when preview is disabled
        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Flake8Copyright::_001.into()]),
                ..RuleSelection::default()
            }],
            Some(PreviewOptions {
                mode: PreviewMode::Disabled,
                ..PreviewOptions::default()
            }),
        );
        let expected = RuleSet::from_rule(Rule::MissingCopyrightNotice);
        assert_eq!(actual, expected);

        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Flake8Copyright::_001.into()]),
                ..RuleSelection::default()
            }],
            Some(PreviewOptions {
                mode: PreviewMode::Enabled,
                ..PreviewOptions::default()
            }),
        );
        let expected = RuleSet::from_rule(Rule::MissingCopyrightNotice);
        assert_eq!(actual, expected);
    }

    #[test]
    #[allow(deprecated)]
    fn select_nursery() {
        // Backwards compatible behavior allows selection of nursery rules with the nursery selector
        // when preview is disabled
        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![RuleSelector::Nursery]),
                ..RuleSelection::default()
            }],
            Some(PreviewOptions {
                mode: PreviewMode::Disabled,
                ..PreviewOptions::default()
            }),
        );
        let expected = RuleSet::from_rules(NURSERY_RULES);
        assert_eq!(actual, expected);

        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![RuleSelector::Nursery]),
                ..RuleSelection::default()
            }],
            Some(PreviewOptions {
                mode: PreviewMode::Enabled,
                ..PreviewOptions::default()
            }),
        );
        let expected = RuleSet::from_rules(NURSERY_RULES);
        assert_eq!(actual, expected);
    }

    #[test]
    fn select_docstring_convention_override() {
        fn assert_override(rule_selections: Vec<RuleSelection>, should_be_overridden: bool) {
            use ruff_linter::rules::pydocstyle::settings::Convention;

            let config = LintConfiguration {
                rule_selections,
                pydocstyle: Some(PydocstyleOptions {
                    convention: Some(Convention::Numpy),
                    ..PydocstyleOptions::default()
                }),
                ..LintConfiguration::default()
            };

            let mut expected = RuleSet::from_rules(&[
                Rule::from_code("D410").unwrap(),
                Rule::from_code("D411").unwrap(),
                Rule::from_code("D412").unwrap(),
                Rule::from_code("D414").unwrap(),
                Rule::from_code("D418").unwrap(),
                Rule::from_code("D419").unwrap(),
            ]);
            if should_be_overridden {
                expected.insert(Rule::from_code("D417").unwrap());
            }
            assert_eq!(
                config
                    .as_rule_table(PreviewMode::Disabled)
                    .iter_enabled()
                    .collect::<RuleSet>(),
                expected,
            );
        }

        let d41 = RuleSelector::from_str("D41").unwrap();
        let d417 = RuleSelector::from_str("D417").unwrap();

        // D417 does not appear when D41 is provided...
        assert_override(
            vec![RuleSelection {
                select: Some(vec![d41.clone()]),
                ..RuleSelection::default()
            }],
            false,
        );

        // ...but does appear when specified directly.
        assert_override(
            vec![RuleSelection {
                select: Some(vec![d41.clone(), d417.clone()]),
                ..RuleSelection::default()
            }],
            true,
        );

        // ...but disappears if there's a subsequent `--select`.
        assert_override(
            vec![
                RuleSelection {
                    select: Some(vec![d417.clone()]),
                    ..RuleSelection::default()
                },
                RuleSelection {
                    select: Some(vec![d41.clone()]),
                    ..RuleSelection::default()
                },
            ],
            false,
        );

        // ...although an `--extend-select` is fine.
        assert_override(
            vec![
                RuleSelection {
                    select: Some(vec![d417.clone()]),
                    ..RuleSelection::default()
                },
                RuleSelection {
                    extend_select: vec![d41.clone()],
                    ..RuleSelection::default()
                },
            ],
            true,
        );
    }
}
