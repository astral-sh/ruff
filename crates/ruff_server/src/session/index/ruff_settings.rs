use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Context;
use ignore::{WalkBuilder, WalkState};

use ruff_linter::{
    display_settings, fs::normalize_path_to, settings::types::FilePattern,
    settings::types::PreviewMode,
};
use ruff_workspace::resolver::match_exclusion;
use ruff_workspace::{
    configuration::{Configuration, FormatConfiguration, LintConfiguration, RuleSelection},
    pyproject::{find_user_settings_toml, settings_toml},
    resolver::{ConfigurationTransformer, Relativity},
};

use crate::session::settings::{ConfigurationPreference, ResolvedEditorSettings};

pub struct RuffSettings {
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
    /// Index from folder to the resolved ruff settings.
    index: BTreeMap<PathBuf, Arc<RuffSettings>>,
    fallback: Arc<RuffSettings>,
}

impl std::fmt::Display for RuffSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            fields = [
                self.file_resolver,
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
        let mut has_error = false;
        let mut index = BTreeMap::default();
        let mut respect_gitignore = None;

        // Add any settings from above the workspace root, excluding the workspace root itself.
        for directory in root.ancestors().skip(1) {
            match settings_toml(directory) {
                Ok(Some(pyproject)) => {
                    match ruff_workspace::resolver::resolve_root_settings(
                        &pyproject,
                        Relativity::Parent,
                        &EditorConfigurationTransformer(editor_settings, root),
                    ) {
                        Ok(settings) => {
                            respect_gitignore = Some(settings.file_resolver.respect_gitignore);

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
                        error => {
                            tracing::error!(
                                "{:#}",
                                error
                                    .with_context(|| {
                                        format!(
                                            "Failed to resolve settings for {}",
                                            pyproject.display()
                                        )
                                    })
                                    .unwrap_err()
                            );
                            has_error = true;
                            continue;
                        }
                    }
                }
                Ok(None) => continue,
                Err(err) => {
                    tracing::error!("{err:#}");
                    has_error = true;
                    continue;
                }
            }
        }

        let fallback = Arc::new(RuffSettings::fallback(editor_settings, root));

        // Add any settings within the workspace itself
        let mut builder = WalkBuilder::new(root);
        builder.standard_filters(
            respect_gitignore.unwrap_or_else(|| fallback.file_resolver().respect_gitignore),
        );
        builder.hidden(false);
        builder.threads(
            std::thread::available_parallelism()
                .map_or(1, std::num::NonZeroUsize::get)
                .min(12),
        );
        let walker = builder.build_parallel();

        let index = std::sync::RwLock::new(index);
        let has_error = AtomicBool::new(has_error);

        walker.run(|| {
            Box::new(|result| {
                let Ok(entry) = result else {
                    return WalkState::Continue;
                };

                // Skip non-directories.
                if !entry
                    .file_type()
                    .is_some_and(|file_type| file_type.is_dir())
                {
                    return WalkState::Continue;
                }

                let directory = entry.into_path();

                // If the directory is excluded from the workspace, skip it.
                if let Some(file_name) = directory.file_name() {
                    let settings = index
                        .read()
                        .unwrap()
                        .range(..directory.clone())
                        .rfind(|(path, _)| directory.starts_with(path))
                        .map(|(_, settings)| settings.clone())
                        .unwrap_or_else(|| fallback.clone());

                    if match_exclusion(&directory, file_name, &settings.file_resolver.exclude) {
                        tracing::debug!("Ignored path via `exclude`: {}", directory.display());
                        return WalkState::Skip;
                    } else if match_exclusion(
                        &directory,
                        file_name,
                        &settings.file_resolver.extend_exclude,
                    ) {
                        tracing::debug!(
                            "Ignored path via `extend-exclude`: {}",
                            directory.display()
                        );
                        return WalkState::Skip;
                    }
                }

                match settings_toml(&directory) {
                    Ok(Some(pyproject)) => {
                        match ruff_workspace::resolver::resolve_root_settings(
                            &pyproject,
                            Relativity::Parent,
                            &EditorConfigurationTransformer(editor_settings, root),
                        ) {
                            Ok(settings) => {
                                index.write().unwrap().insert(
                                    directory,
                                    Arc::new(RuffSettings {
                                        path: Some(pyproject),
                                        file_resolver: settings.file_resolver,
                                        linter: settings.linter,
                                        formatter: settings.formatter,
                                    }),
                                );
                            }
                            error => {
                                tracing::error!(
                                    "{:#}",
                                    error
                                        .with_context(|| {
                                            format!(
                                                "Failed to resolve settings for {}",
                                                pyproject.display()
                                            )
                                        })
                                        .unwrap_err()
                                );
                                has_error.store(true, Ordering::Relaxed);
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(err) => {
                        tracing::error!("{err:#}");
                        has_error.store(true, Ordering::Relaxed);
                    }
                }

                WalkState::Continue
            })
        });

        if has_error.load(Ordering::Relaxed) {
            let root = root.display();
            show_err_msg!(
                "Error while resolving settings from workspace {root}. Please refer to the logs for more details.",
            );
        }

        Self {
            index: index.into_inner().unwrap(),
            fallback,
        }
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
            match open_configuration_file(&config_file_path) {
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

fn open_configuration_file(config_path: &Path) -> crate::Result<Configuration> {
    ruff_workspace::resolver::resolve_configuration(
        config_path,
        Relativity::Parent,
        &IdentityTransformer,
    )
}

struct IdentityTransformer;

impl ConfigurationTransformer for IdentityTransformer {
    fn transform(&self, config: Configuration) -> Configuration {
        config
    }
}
