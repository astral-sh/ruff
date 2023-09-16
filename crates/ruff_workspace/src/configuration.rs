//! User-provided program settings, taking into account pyproject.toml and
//! command-line options. Structure mirrors the user-facing representation of
//! the various parameters.

use std::borrow::Cow;
use std::env::VarError;
use std::path::{Path, PathBuf};

use crate::options::{
    Flake8AnnotationsOptions, Flake8BanditOptions, Flake8BugbearOptions, Flake8BuiltinsOptions,
    Flake8ComprehensionsOptions, Flake8CopyrightOptions, Flake8ErrMsgOptions, Flake8GetTextOptions,
    Flake8ImplicitStrConcatOptions, Flake8ImportConventionsOptions, Flake8PytestStyleOptions,
    Flake8QuotesOptions, Flake8SelfOptions, Flake8TidyImportsOptions, Flake8TypeCheckingOptions,
    Flake8UnusedArgumentsOptions, IsortOptions, McCabeOptions, Options, Pep8NamingOptions,
    PyUpgradeOptions, PycodestyleOptions, PydocstyleOptions, PyflakesOptions, PylintOptions,
};
use anyhow::{anyhow, Result};
use glob::{glob, GlobError, Paths, PatternError};
use regex::Regex;
use ruff::line_width::{LineLength, TabSize};
use ruff::registry::RuleNamespace;
use ruff::registry::{Rule, RuleSet, INCOMPATIBLE_CODES};
use ruff::rule_selector::Specificity;
use ruff::settings::rule_table::RuleTable;
use ruff::settings::types::{
    FilePattern, FilePatternSet, PerFileIgnore, PreviewMode, PythonVersion, SerializationFormat,
    Version,
};
use ruff::settings::{defaults, resolve_per_file_ignores, AllSettings, CliSettings, Settings};
use ruff::{fs, warn_user, warn_user_once, warn_user_once_by_id, RuleSelector, RUFF_PKG_VERSION};
use ruff_cache::cache_dir;
use rustc_hash::{FxHashMap, FxHashSet};
use shellexpand;
use shellexpand::LookupError;
use strum::IntoEnumIterator;

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
    pub logger_objects: Option<Vec<String>>,
    pub namespace_packages: Option<Vec<PathBuf>>,
    pub preview: Option<PreviewMode>,
    pub required_version: Option<Version>,
    pub respect_gitignore: Option<bool>,
    pub show_fixes: Option<bool>,
    pub show_source: Option<bool>,
    pub src: Option<Vec<PathBuf>>,
    pub tab_size: Option<TabSize>,
    pub target_version: Option<PythonVersion>,
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

impl Configuration {
    pub fn into_all_settings(self, project_root: &Path) -> Result<AllSettings> {
        Ok(AllSettings {
            cli: CliSettings {
                cache_dir: self
                    .cache_dir
                    .clone()
                    .unwrap_or_else(|| cache_dir(project_root)),
                fix: self.fix.unwrap_or(false),
                fix_only: self.fix_only.unwrap_or(false),
                format: self.format.unwrap_or_default(),
                show_fixes: self.show_fixes.unwrap_or(false),
                show_source: self.show_source.unwrap_or(false),
            },
            lib: self.into_settings(project_root)?,
        })
    }

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

        Ok(Settings {
            rules: self.as_rule_table(),
            allowed_confusables: self
                .allowed_confusables
                .map(FxHashSet::from_iter)
                .unwrap_or_default(),
            builtins: self.builtins.unwrap_or_default(),
            dummy_variable_rgx: self
                .dummy_variable_rgx
                .unwrap_or_else(|| defaults::DUMMY_VARIABLE_RGX.clone()),
            exclude: FilePatternSet::try_from_vec(
                self.exclude.unwrap_or_else(|| defaults::EXCLUDE.clone()),
            )?,
            extend_exclude: FilePatternSet::try_from_vec(self.extend_exclude)?,
            extend_include: FilePatternSet::try_from_vec(self.extend_include)?,
            external: FxHashSet::from_iter(self.external.unwrap_or_default()),
            force_exclude: self.force_exclude.unwrap_or(false),
            include: FilePatternSet::try_from_vec(
                self.include.unwrap_or_else(|| defaults::INCLUDE.clone()),
            )?,
            ignore_init_module_imports: self.ignore_init_module_imports.unwrap_or_default(),
            line_length: self.line_length.unwrap_or_default(),
            tab_size: self.tab_size.unwrap_or_default(),
            namespace_packages: self.namespace_packages.unwrap_or_default(),
            per_file_ignores: resolve_per_file_ignores(
                self.per_file_ignores
                    .unwrap_or_default()
                    .into_iter()
                    .chain(self.extend_per_file_ignores)
                    .collect(),
            )?,
            respect_gitignore: self.respect_gitignore.unwrap_or(true),
            src: self.src.unwrap_or_else(|| vec![project_root.to_path_buf()]),
            project_root: project_root.to_path_buf(),
            target_version: self.target_version.unwrap_or_default(),
            task_tags: self.task_tags.unwrap_or_else(|| {
                defaults::TASK_TAGS
                    .iter()
                    .map(ToString::to_string)
                    .collect()
            }),
            logger_objects: self.logger_objects.unwrap_or_default(),
            preview: self.preview.unwrap_or_default(),
            typing_modules: self.typing_modules.unwrap_or_default(),
            // Plugins
            flake8_annotations: self
                .flake8_annotations
                .map(Flake8AnnotationsOptions::into_settings)
                .unwrap_or_default(),
            flake8_bandit: self
                .flake8_bandit
                .map(Flake8BanditOptions::into_settings)
                .unwrap_or_default(),
            flake8_bugbear: self
                .flake8_bugbear
                .map(Flake8BugbearOptions::into_settings)
                .unwrap_or_default(),
            flake8_builtins: self
                .flake8_builtins
                .map(Flake8BuiltinsOptions::into_settings)
                .unwrap_or_default(),
            flake8_comprehensions: self
                .flake8_comprehensions
                .map(Flake8ComprehensionsOptions::into_settings)
                .unwrap_or_default(),
            flake8_copyright: self
                .flake8_copyright
                .map(Flake8CopyrightOptions::try_into_settings)
                .transpose()?
                .unwrap_or_default(),
            flake8_errmsg: self
                .flake8_errmsg
                .map(Flake8ErrMsgOptions::into_settings)
                .unwrap_or_default(),
            flake8_implicit_str_concat: self
                .flake8_implicit_str_concat
                .map(Flake8ImplicitStrConcatOptions::into_settings)
                .unwrap_or_default(),
            flake8_import_conventions: self
                .flake8_import_conventions
                .map(Flake8ImportConventionsOptions::into_settings)
                .unwrap_or_default(),
            flake8_pytest_style: self
                .flake8_pytest_style
                .map(Flake8PytestStyleOptions::try_into_settings)
                .transpose()?
                .unwrap_or_default(),
            flake8_quotes: self
                .flake8_quotes
                .map(Flake8QuotesOptions::into_settings)
                .unwrap_or_default(),
            flake8_self: self
                .flake8_self
                .map(Flake8SelfOptions::into_settings)
                .unwrap_or_default(),
            flake8_tidy_imports: self
                .flake8_tidy_imports
                .map(Flake8TidyImportsOptions::into_settings)
                .unwrap_or_default(),
            flake8_type_checking: self
                .flake8_type_checking
                .map(Flake8TypeCheckingOptions::into_settings)
                .unwrap_or_default(),
            flake8_unused_arguments: self
                .flake8_unused_arguments
                .map(Flake8UnusedArgumentsOptions::into_settings)
                .unwrap_or_default(),
            flake8_gettext: self
                .flake8_gettext
                .map(Flake8GetTextOptions::into_settings)
                .unwrap_or_default(),
            isort: self
                .isort
                .map(IsortOptions::try_into_settings)
                .transpose()?
                .unwrap_or_default(),
            mccabe: self
                .mccabe
                .map(McCabeOptions::into_settings)
                .unwrap_or_default(),
            pep8_naming: self
                .pep8_naming
                .map(Pep8NamingOptions::try_into_settings)
                .transpose()?
                .unwrap_or_default(),
            pycodestyle: self
                .pycodestyle
                .map(PycodestyleOptions::into_settings)
                .unwrap_or_default(),
            pydocstyle: self
                .pydocstyle
                .map(PydocstyleOptions::into_settings)
                .unwrap_or_default(),
            pyflakes: self
                .pyflakes
                .map(PyflakesOptions::into_settings)
                .unwrap_or_default(),
            pylint: self
                .pylint
                .map(PylintOptions::into_settings)
                .unwrap_or_default(),
            pyupgrade: self
                .pyupgrade
                .map(PyUpgradeOptions::into_settings)
                .unwrap_or_default(),
        })
    }

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
            preview: options.preview.map(PreviewMode::from),
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
            logger_objects: options.logger_objects,
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

    pub fn as_rule_table(&self) -> RuleTable {
        let preview = self.preview.unwrap_or_default();

        // The select_set keeps track of which rules have been selected.
        let mut select_set: RuleSet = defaults::PREFIXES
            .iter()
            .flat_map(|selector| selector.rules(preview))
            .collect();

        // The fixable set keeps track of which rules are fixable.
        let mut fixable_set: RuleSet = RuleSelector::All.rules(preview).collect();

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
                    for rule in selector.rules(preview) {
                        select_map_updates.insert(rule, true);
                    }
                }
                for selector in selection
                    .ignore
                    .iter()
                    .chain(carriedover_ignores.into_iter().flatten())
                    .filter(|s| s.specificity() == spec)
                {
                    for rule in selector.rules(preview) {
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
                    for rule in selector.rules(preview) {
                        fixable_map_updates.insert(rule, true);
                    }
                }
                for selector in selection
                    .unfixable
                    .iter()
                    .chain(carriedover_unfixables.into_iter().flatten())
                    .filter(|s| s.specificity() == spec)
                {
                    for rule in selector.rules(preview) {
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
            } else {
                // Otherwise we apply the updates on top of the existing select_set.
                for (rule, enabled) in select_map_updates {
                    if enabled {
                        select_set.insert(rule);
                    } else {
                        select_set.remove(rule);
                    }
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
            for selector in selection
                .select
                .iter()
                .chain(selection.fixable.iter())
                .flatten()
                .chain(selection.ignore.iter())
                .chain(selection.extend_select.iter())
                .chain(selection.unfixable.iter())
                .chain(selection.extend_fixable.iter())
            {
                #[allow(deprecated)]
                if matches!(selector, RuleSelector::Nursery) {
                    let suggestion = if preview.is_disabled() {
                        " Use the `--preview` flag instead."
                    } else {
                        // We have no suggested alternative since there is intentionally no "PREVIEW" selector
                        ""
                    };
                    warn_user_once!("The `NURSERY` selector has been deprecated.{suggestion}");
                }

                if preview.is_disabled() {
                    if let RuleSelector::Rule { prefix, .. } = selector {
                        if prefix.rules().any(|rule| rule.is_nursery()) {
                            deprecated_nursery_selectors.insert(selector);
                        }
                    }

                    // Check if the selector is empty because preview mode is disabled
                    if selector.rules(PreviewMode::Disabled).next().is_none() {
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

        // If a docstring convention is specified, force-disable any incompatible error
        // codes.
        if let Some(convention) = self
            .pydocstyle
            .as_ref()
            .and_then(|pydocstyle| pydocstyle.convention)
        {
            for rule in convention.rules_to_be_ignored() {
                rules.disable(*rule);
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
            logger_objects: self.logger_objects.or(config.logger_objects),
            tab_size: self.tab_size.or(config.tab_size),
            namespace_packages: self.namespace_packages.or(config.namespace_packages),
            per_file_ignores: self.per_file_ignores.or(config.per_file_ignores),
            required_version: self.required_version.or(config.required_version),
            respect_gitignore: self.respect_gitignore.or(config.respect_gitignore),
            show_source: self.show_source.or(config.show_source),
            show_fixes: self.show_fixes.or(config.show_fixes),
            src: self.src.or(config.src),
            target_version: self.target_version.or(config.target_version),
            preview: self.preview.or(config.preview),
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
    use crate::configuration::{Configuration, RuleSelection};
    use ruff::codes::{Flake8Copyright, Pycodestyle, Refurb};
    use ruff::registry::{Linter, Rule, RuleSet};
    use ruff::settings::types::PreviewMode;
    use ruff::RuleSelector;

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
        Rule::DirectLoggerInstantiation,
        Rule::ManualDictComprehension,
        Rule::SliceCopy,
        Rule::TooManyPublicMethods,
        Rule::TooManyPublicMethods,
        Rule::UndocumentedWarn,
    ];

    #[allow(clippy::needless_pass_by_value)]
    fn resolve_rules(
        selections: impl IntoIterator<Item = RuleSelection>,
        preview: Option<PreviewMode>,
    ) -> RuleSet {
        Configuration {
            rule_selections: selections.into_iter().collect(),
            preview,
            ..Configuration::default()
        }
        .as_rule_table()
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
            Some(PreviewMode::Disabled),
        );
        assert!(!actual.intersects(&RuleSet::from_rules(PREVIEW_RULES)));

        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![RuleSelector::All]),
                ..RuleSelection::default()
            }],
            Some(PreviewMode::Enabled),
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
            Some(PreviewMode::Disabled),
        );
        let expected = RuleSet::empty();
        assert_eq!(actual, expected);

        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Linter::Flake8Copyright.into()]),
                ..RuleSelection::default()
            }],
            Some(PreviewMode::Enabled),
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
            Some(PreviewMode::Disabled),
        );
        let expected = RuleSet::empty();
        assert_eq!(actual, expected);

        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Flake8Copyright::_0.into()]),
                ..RuleSelection::default()
            }],
            Some(PreviewMode::Enabled),
        );
        let expected = RuleSet::from_rule(Rule::MissingCopyrightNotice);
        assert_eq!(actual, expected);
    }

    #[test]
    fn select_rule_preview() {
        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Refurb::_145.into()]),
                ..RuleSelection::default()
            }],
            Some(PreviewMode::Disabled),
        );
        let expected = RuleSet::empty();
        assert_eq!(actual, expected);

        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Refurb::_145.into()]),
                ..RuleSelection::default()
            }],
            Some(PreviewMode::Enabled),
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
            Some(PreviewMode::Disabled),
        );
        let expected = RuleSet::from_rule(Rule::MissingCopyrightNotice);
        assert_eq!(actual, expected);

        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![Flake8Copyright::_001.into()]),
                ..RuleSelection::default()
            }],
            Some(PreviewMode::Enabled),
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
            Some(PreviewMode::Disabled),
        );
        let expected = RuleSet::from_rules(NURSERY_RULES);
        assert_eq!(actual, expected);

        let actual = resolve_rules(
            [RuleSelection {
                select: Some(vec![RuleSelector::Nursery]),
                ..RuleSelection::default()
            }],
            Some(PreviewMode::Enabled),
        );
        let expected = RuleSet::from_rules(NURSERY_RULES);
        assert_eq!(actual, expected);
    }
}
