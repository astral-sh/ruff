use globset::Candidate;
use ruff_linter::{
    display_settings, fs::normalize_path_to, settings::types::FilePattern,
    settings::types::PreviewMode,
};
use ruff_workspace::resolver::match_candidate_exclusion;
use ruff_workspace::{
    configuration::{Configuration, FormatConfiguration, LintConfiguration, RuleSelection},
    pyproject::{find_user_settings_toml, settings_toml},
    resolver::{ConfigurationTransformer, Relativity},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use walkdir::WalkDir;

use crate::session::settings::{ConfigurationPreference, ResolvedEditorSettings};

pub(crate) struct RuffSettings {
    /// The path to this configuration file, used for debugging.
    /// The default fallback configuration does not have a file path.
    path: Option<PathBuf>,
    /// Settings used to manage file inclusion and exclusion.
    file_resolver: ruff_workspace::FileResolverSettings,
    /// Settings to pass into the Ruff linter.
    linter: ruff_linter::settings::LinterSettings,
    /// Settings to pass into the Ruff formatter.
    formatter: ruff_workspace::FormatterSettings,
}

pub(super) struct RuffSettingsIndex {
    /// Index from folder to the resoled ruff settings.
    index: BTreeMap<PathBuf, Arc<RuffSettings>>,
    fallback: Arc<RuffSettings>,
}

impl std::fmt::Display for RuffSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            fields = [
                self.linter,
                self.formatter
            ]
        }
        Ok(())
    }
}

impl RuffSettings {
    pub(crate) fn fallback(editor_settings: &ResolvedEditorSettings, root: &Path) -> RuffSettings {
        let mut path = None;
        let fallback = find_user_settings_toml()
            .and_then(|user_settings| {
                let settings = ruff_workspace::resolver::resolve_root_settings(
                    &user_settings,
                    Relativity::Cwd,
                    &EditorConfigurationTransformer(editor_settings, root),
                )
                .ok();
                path = Some(user_settings);
                settings
            })
            .unwrap_or_else(|| {
                let default_configuration = Configuration::default();
                EditorConfigurationTransformer(editor_settings, root)
                    .transform(default_configuration)
                    .into_settings(root)
                    .expect(
                        "editor configuration should merge successfully with default configuration",
                    )
            });

        RuffSettings {
            path,
            file_resolver: fallback.file_resolver,
            formatter: fallback.formatter,
            linter: fallback.linter,
        }
    }

    /// Return the [`ruff_workspace::FileResolverSettings`] for this [`RuffSettings`].
    pub(crate) fn file_resolver(&self) -> &ruff_workspace::FileResolverSettings {
        &self.file_resolver
    }

    /// Return the [`ruff_linter::settings::LinterSettings`] for this [`RuffSettings`].
    pub(crate) fn linter(&self) -> &ruff_linter::settings::LinterSettings {
        &self.linter
    }

    /// Return the [`ruff_workspace::FormatterSettings`] for this [`RuffSettings`].
    pub(crate) fn formatter(&self) -> &ruff_workspace::FormatterSettings {
        &self.formatter
    }
}

impl RuffSettingsIndex {
    pub(super) fn new(root: &Path, editor_settings: &ResolvedEditorSettings) -> Self {
        let mut index = BTreeMap::default();

        // Add any settings from above the workspace root.
        for directory in root.ancestors() {
            if let Some(pyproject) = settings_toml(directory).ok().flatten() {
                let Ok(settings) = ruff_workspace::resolver::resolve_root_settings(
                    &pyproject,
                    Relativity::Parent,
                    &EditorConfigurationTransformer(editor_settings, root),
                ) else {
                    continue;
                };

                index.insert(
                    directory.to_path_buf(),
                    Arc::new(RuffSettings {
                        path: Some(pyproject),
                        file_resolver: settings.file_resolver,
                        linter: settings.linter,
                        formatter: settings.formatter,
                    }),
                );
                break;
            }
        }

        // Add any settings within the workspace itself
        let mut walker = WalkDir::new(root).into_iter();

        while let Some(entry) = walker.next() {
            let Ok(entry) = entry else {
                continue;
            };

            // Skip non-directories.
            if !entry.file_type().is_dir() {
                continue;
            }

            let directory = entry.into_path();

            // If the directory is excluded from the workspace, skip it.
            if let Some(file_name) = directory.file_name() {
                if let Some((_, settings)) = index
                    .range(..directory.clone())
                    .rfind(|(path, _)| directory.starts_with(path))
                {
                    let candidate = Candidate::new(&directory);
                    let basename = Candidate::new(file_name);
                    if match_candidate_exclusion(
                        &candidate,
                        &basename,
                        &settings.file_resolver.exclude,
                    ) {
                        tracing::debug!("Ignored path via `exclude`: {}", directory.display());

                        walker.skip_current_dir();
                        continue;
                    } else if match_candidate_exclusion(
                        &candidate,
                        &basename,
                        &settings.file_resolver.extend_exclude,
                    ) {
                        tracing::debug!(
                            "Ignored path via `extend-exclude`: {}",
                            directory.display()
                        );

                        walker.skip_current_dir();
                        continue;
                    }
                }
            }

            if let Some(pyproject) = settings_toml(&directory).ok().flatten() {
                let Ok(settings) = ruff_workspace::resolver::resolve_root_settings(
                    &pyproject,
                    Relativity::Parent,
                    &EditorConfigurationTransformer(editor_settings, root),
                ) else {
                    continue;
                };
                index.insert(
                    directory,
                    Arc::new(RuffSettings {
                        path: Some(pyproject),
                        file_resolver: settings.file_resolver,
                        linter: settings.linter,
                        formatter: settings.formatter,
                    }),
                );
            }
        }

        let fallback = Arc::new(RuffSettings::fallback(editor_settings, root));

        Self { index, fallback }
    }

    pub(super) fn get(&self, document_path: &Path) -> Arc<RuffSettings> {
        self.index
            .range(..document_path.to_path_buf())
            .rfind(|(path, _)| document_path.starts_with(path))
            .map(|(_, settings)| settings)
            .unwrap_or_else(|| &self.fallback)
            .clone()
    }

    pub(crate) fn list_files(&self) -> impl Iterator<Item = &Path> {
        self.index
            .values()
            .filter_map(|settings| settings.path.as_deref())
    }

    pub(super) fn fallback(&self) -> Arc<RuffSettings> {
        self.fallback.clone()
    }
}

struct EditorConfigurationTransformer<'a>(&'a ResolvedEditorSettings, &'a Path);

impl<'a> ConfigurationTransformer for EditorConfigurationTransformer<'a> {
    fn transform(&self, filesystem_configuration: Configuration) -> Configuration {
        let ResolvedEditorSettings {
            configuration,
            format_preview,
            lint_preview,
            select,
            extend_select,
            ignore,
            exclude,
            line_length,
            configuration_preference,
        } = self.0.clone();

        let project_root = self.1;

        let editor_configuration = Configuration {
            lint: LintConfiguration {
                preview: lint_preview.map(PreviewMode::from),
                rule_selections: vec![RuleSelection {
                    select,
                    extend_select: extend_select.unwrap_or_default(),
                    ignore: ignore.unwrap_or_default(),
                    ..RuleSelection::default()
                }],
                ..LintConfiguration::default()
            },
            format: FormatConfiguration {
                preview: format_preview.map(PreviewMode::from),
                ..FormatConfiguration::default()
            },
            exclude: exclude.map(|exclude| {
                exclude
                    .into_iter()
                    .map(|pattern| {
                        let absolute = normalize_path_to(&pattern, project_root);
                        FilePattern::User(pattern, absolute)
                    })
                    .collect()
            }),
            line_length,
            ..Configuration::default()
        };

        // Merge in the editor-specified configuration file, if it exists.
        let editor_configuration = if let Some(config_file_path) = configuration {
            match open_configuration_file(&config_file_path, project_root) {
                Ok(config_from_file) => editor_configuration.combine(config_from_file),
                Err(err) => {
                    tracing::error!("Unable to find editor-specified configuration file: {err}");
                    editor_configuration
                }
            }
        } else {
            editor_configuration
        };

        match configuration_preference {
            ConfigurationPreference::EditorFirst => {
                editor_configuration.combine(filesystem_configuration)
            }
            ConfigurationPreference::FilesystemFirst => {
                filesystem_configuration.combine(editor_configuration)
            }
            ConfigurationPreference::EditorOnly => editor_configuration,
        }
    }
}

fn open_configuration_file(
    config_path: &Path,
    project_root: &Path,
) -> crate::Result<Configuration> {
    let options = ruff_workspace::pyproject::load_options(config_path)?;

    Configuration::from_options(options, Some(config_path), project_root)
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, str::FromStr};

    use ruff_linter::{settings::types::PreviewMode, RuleSelector};
    use ruff_workspace::{
        configuration::{Configuration, LintConfiguration, RuleSelection},
        options::Flake8AnnotationsOptions,
        resolver::ConfigurationTransformer,
    };

    use crate::session::{
        index::ruff_settings::EditorConfigurationTransformer,
        settings::{ConfigurationPreference, ResolvedEditorSettings},
    };

    #[test]
    fn test_editor_and_filesystem_configuration_resolution() {
        let test_path = PathBuf::from_str("/Users/jane/projects/test").unwrap();

        let filesystem_configuration = Configuration {
            lint: LintConfiguration {
                exclude: Some(vec![]),
                preview: Some(PreviewMode::Enabled),
                rule_selections: vec![RuleSelection {
                    select: Some(vec![]),
                    ignore: vec![],
                    fixable: Some(vec![]),
                    ..Default::default()
                }],
                flake8_annotations: Some(Flake8AnnotationsOptions {
                    mypy_init_return: Some(true),
                    suppress_dummy_args: Some(false),
                    ignore_fully_untyped: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };

        let mut resolved_editor_settings = ResolvedEditorSettings {
            configuration: Some(PathBuf::from_str("/Users/test/.config/ruff/ruff.toml").unwrap()),
            lint_preview: Some(false),
            format_preview: Some(false),
            select: Some(vec![RuleSelector::Linter(
                ruff_linter::registry::Linter::Flake8Annotations,
            )]),
            extend_select: Some(vec![RuleSelector::All]),
            ignore: None,
            exclude: None,
            line_length: Some(serde_json::from_value(serde_json::json!(80)).unwrap()),
            configuration_preference: ConfigurationPreference::EditorFirst,
        };

        insta::assert_debug_snapshot!(EditorConfigurationTransformer(&resolved_editor_settings, &test_path).transform(filesystem_configuration.clone()), @r###"
        Configuration {
            cache_dir: None,
            extend: None,
            fix: None,
            fix_only: None,
            unsafe_fixes: None,
            output_format: None,
            preview: None,
            required_version: None,
            extension: None,
            show_fixes: None,
            exclude: None,
            extend_exclude: [],
            extend_include: [],
            force_exclude: None,
            include: None,
            respect_gitignore: None,
            builtins: None,
            namespace_packages: None,
            src: None,
            target_version: None,
            line_length: Some(
                LineLength(
                    80,
                ),
            ),
            indent_width: None,
            lint: LintConfiguration {
                exclude: Some(
                    [],
                ),
                preview: Some(
                    Disabled,
                ),
                extend_per_file_ignores: [],
                per_file_ignores: None,
                rule_selections: [
                    RuleSelection {
                        select: Some(
                            [],
                        ),
                        ignore: [],
                        extend_select: [],
                        fixable: Some(
                            [],
                        ),
                        unfixable: [],
                        extend_fixable: [],
                    },
                    RuleSelection {
                        select: Some(
                            [
                                Linter(
                                    Flake8Annotations,
                                ),
                            ],
                        ),
                        ignore: [],
                        extend_select: [
                            All,
                        ],
                        fixable: None,
                        unfixable: [],
                        extend_fixable: [],
                    },
                ],
                explicit_preview_rules: None,
                extend_unsafe_fixes: [],
                extend_safe_fixes: [],
                allowed_confusables: None,
                dummy_variable_rgx: None,
                external: None,
                ignore_init_module_imports: None,
                logger_objects: None,
                task_tags: None,
                typing_modules: None,
                flake8_annotations: Some(
                    Flake8AnnotationsOptions {
                        mypy_init_return: Some(
                            true,
                        ),
                        suppress_dummy_args: Some(
                            false,
                        ),
                        suppress_none_returning: None,
                        allow_star_arg_any: None,
                        ignore_fully_untyped: Some(
                            true,
                        ),
                    },
                ),
                flake8_bandit: None,
                flake8_boolean_trap: None,
                flake8_bugbear: None,
                flake8_builtins: None,
                flake8_comprehensions: None,
                flake8_copyright: None,
                flake8_errmsg: None,
                flake8_gettext: None,
                flake8_implicit_str_concat: None,
                flake8_import_conventions: None,
                flake8_pytest_style: None,
                flake8_quotes: None,
                flake8_self: None,
                flake8_tidy_imports: None,
                flake8_type_checking: None,
                flake8_unused_arguments: None,
                isort: None,
                mccabe: None,
                pep8_naming: None,
                pycodestyle: None,
                pydocstyle: None,
                pyflakes: None,
                pylint: None,
                pyupgrade: None,
            },
            format: FormatConfiguration {
                exclude: None,
                preview: Some(
                    Disabled,
                ),
                extension: None,
                indent_style: None,
                quote_style: None,
                magic_trailing_comma: None,
                line_ending: None,
                docstring_code_format: None,
                docstring_code_line_width: None,
            },
        }
        "###);

        resolved_editor_settings.configuration_preference =
            ConfigurationPreference::FilesystemFirst;

        insta::assert_debug_snapshot!(EditorConfigurationTransformer(&resolved_editor_settings, &test_path).transform(filesystem_configuration.clone()), @r###"
        Configuration {
            cache_dir: None,
            extend: None,
            fix: None,
            fix_only: None,
            unsafe_fixes: None,
            output_format: None,
            preview: None,
            required_version: None,
            extension: None,
            show_fixes: None,
            exclude: None,
            extend_exclude: [],
            extend_include: [],
            force_exclude: None,
            include: None,
            respect_gitignore: None,
            builtins: None,
            namespace_packages: None,
            src: None,
            target_version: None,
            line_length: Some(
                LineLength(
                    80,
                ),
            ),
            indent_width: None,
            lint: LintConfiguration {
                exclude: Some(
                    [],
                ),
                preview: Some(
                    Enabled,
                ),
                extend_per_file_ignores: [],
                per_file_ignores: None,
                rule_selections: [
                    RuleSelection {
                        select: Some(
                            [
                                Linter(
                                    Flake8Annotations,
                                ),
                            ],
                        ),
                        ignore: [],
                        extend_select: [
                            All,
                        ],
                        fixable: None,
                        unfixable: [],
                        extend_fixable: [],
                    },
                    RuleSelection {
                        select: Some(
                            [],
                        ),
                        ignore: [],
                        extend_select: [],
                        fixable: Some(
                            [],
                        ),
                        unfixable: [],
                        extend_fixable: [],
                    },
                ],
                explicit_preview_rules: None,
                extend_unsafe_fixes: [],
                extend_safe_fixes: [],
                allowed_confusables: None,
                dummy_variable_rgx: None,
                external: None,
                ignore_init_module_imports: None,
                logger_objects: None,
                task_tags: None,
                typing_modules: None,
                flake8_annotations: Some(
                    Flake8AnnotationsOptions {
                        mypy_init_return: Some(
                            true,
                        ),
                        suppress_dummy_args: Some(
                            false,
                        ),
                        suppress_none_returning: None,
                        allow_star_arg_any: None,
                        ignore_fully_untyped: Some(
                            true,
                        ),
                    },
                ),
                flake8_bandit: None,
                flake8_boolean_trap: None,
                flake8_bugbear: None,
                flake8_builtins: None,
                flake8_comprehensions: None,
                flake8_copyright: None,
                flake8_errmsg: None,
                flake8_gettext: None,
                flake8_implicit_str_concat: None,
                flake8_import_conventions: None,
                flake8_pytest_style: None,
                flake8_quotes: None,
                flake8_self: None,
                flake8_tidy_imports: None,
                flake8_type_checking: None,
                flake8_unused_arguments: None,
                isort: None,
                mccabe: None,
                pep8_naming: None,
                pycodestyle: None,
                pydocstyle: None,
                pyflakes: None,
                pylint: None,
                pyupgrade: None,
            },
            format: FormatConfiguration {
                exclude: None,
                preview: Some(
                    Disabled,
                ),
                extension: None,
                indent_style: None,
                quote_style: None,
                magic_trailing_comma: None,
                line_ending: None,
                docstring_code_format: None,
                docstring_code_line_width: None,
            },
        }
        "###);

        resolved_editor_settings.configuration_preference = ConfigurationPreference::EditorOnly;

        insta::assert_debug_snapshot!(EditorConfigurationTransformer(&resolved_editor_settings, &test_path).transform(filesystem_configuration), @r###"
        Configuration {
            cache_dir: None,
            extend: None,
            fix: None,
            fix_only: None,
            unsafe_fixes: None,
            output_format: None,
            preview: None,
            required_version: None,
            extension: None,
            show_fixes: None,
            exclude: None,
            extend_exclude: [],
            extend_include: [],
            force_exclude: None,
            include: None,
            respect_gitignore: None,
            builtins: None,
            namespace_packages: None,
            src: None,
            target_version: None,
            line_length: Some(
                LineLength(
                    80,
                ),
            ),
            indent_width: None,
            lint: LintConfiguration {
                exclude: None,
                preview: Some(
                    Disabled,
                ),
                extend_per_file_ignores: [],
                per_file_ignores: None,
                rule_selections: [
                    RuleSelection {
                        select: Some(
                            [
                                Linter(
                                    Flake8Annotations,
                                ),
                            ],
                        ),
                        ignore: [],
                        extend_select: [
                            All,
                        ],
                        fixable: None,
                        unfixable: [],
                        extend_fixable: [],
                    },
                ],
                explicit_preview_rules: None,
                extend_unsafe_fixes: [],
                extend_safe_fixes: [],
                allowed_confusables: None,
                dummy_variable_rgx: None,
                external: None,
                ignore_init_module_imports: None,
                logger_objects: None,
                task_tags: None,
                typing_modules: None,
                flake8_annotations: None,
                flake8_bandit: None,
                flake8_boolean_trap: None,
                flake8_bugbear: None,
                flake8_builtins: None,
                flake8_comprehensions: None,
                flake8_copyright: None,
                flake8_errmsg: None,
                flake8_gettext: None,
                flake8_implicit_str_concat: None,
                flake8_import_conventions: None,
                flake8_pytest_style: None,
                flake8_quotes: None,
                flake8_self: None,
                flake8_tidy_imports: None,
                flake8_type_checking: None,
                flake8_unused_arguments: None,
                isort: None,
                mccabe: None,
                pep8_naming: None,
                pycodestyle: None,
                pydocstyle: None,
                pyflakes: None,
                pylint: None,
                pyupgrade: None,
            },
            format: FormatConfiguration {
                exclude: None,
                preview: Some(
                    Disabled,
                ),
                extension: None,
                indent_style: None,
                quote_style: None,
                magic_trailing_comma: None,
                line_ending: None,
                docstring_code_format: None,
                docstring_code_line_width: None,
            },
        }
        "###);
    }
}
