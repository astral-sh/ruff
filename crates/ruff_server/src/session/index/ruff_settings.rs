use std::collections::BTreeMap;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Context;
use ignore::{WalkBuilder, WalkState};

use ruff_linter::settings::types::GlobPath;
use ruff_linter::{settings::types::FilePattern, settings::types::PreviewMode};
use ruff_workspace::pyproject::find_fallback_target_version;
use ruff_workspace::resolver::match_exclusion;
use ruff_workspace::Settings;
use ruff_workspace::{
    configuration::{Configuration, FormatConfiguration, LintConfiguration, RuleSelection},
    pyproject::{find_user_settings_toml, settings_toml},
    resolver::ConfigurationTransformer,
};

use crate::session::settings::{
    ConfigurationPreference, ResolvedConfiguration, ResolvedEditorSettings,
};

#[derive(Debug)]
pub struct RuffSettings {
    /// The path to this configuration file, used for debugging.
    /// The default fallback configuration does not have a file path.
    path: Option<PathBuf>,
    /// The resolved settings.
    settings: Settings,
}

impl RuffSettings {
    pub(crate) fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

impl Deref for RuffSettings {
    type Target = Settings;

    fn deref(&self) -> &Settings {
        &self.settings
    }
}

pub(super) struct RuffSettingsIndex {
    /// Index from folder to the resolved ruff settings.
    index: BTreeMap<PathBuf, Arc<RuffSettings>>,
    fallback: Arc<RuffSettings>,
}

impl std::fmt::Display for RuffSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.settings, f)
    }
}

impl RuffSettings {
    /// Constructs [`RuffSettings`] by attempting to resolve settings from a user-provided
    /// configuration file, such as `pyproject.toml` or `ruff.toml`, within the
    /// user's workspace.
    ///
    /// In the absence of a valid configuration file, it gracefully falls back to
    /// editor-only settings.
    pub(crate) fn fallback(editor_settings: &ResolvedEditorSettings, root: &Path) -> RuffSettings {
        struct FallbackTransformer<'a> {
            inner: EditorConfigurationTransformer<'a>,
        }

        impl ConfigurationTransformer for FallbackTransformer<'_> {
            fn transform(&self, mut configuration: Configuration) -> Configuration {
                let fallback = find_fallback_target_version(self.inner.1);
                if let Some(fallback) = fallback {
                    tracing::debug!(
                        "Derived `target-version` from found `requires-python`: {fallback:?}"
                    );
                    configuration.target_version = Some(fallback.into());
                }

                self.inner.transform(configuration)
            }
        }

        find_user_settings_toml()
            .and_then(|user_settings| {
                tracing::debug!(
                    "Loading settings from user configuration file: `{}`",
                    user_settings.display()
                );
                ruff_workspace::resolver::resolve_root_settings(
                    &user_settings,
                    &FallbackTransformer {
                        inner: EditorConfigurationTransformer(editor_settings, root),
                    },
                    ruff_workspace::resolver::ConfigurationOrigin::UserSettings,
                )
                .ok()
                .map(|settings| RuffSettings {
                    path: Some(user_settings),
                    settings,
                })
            })
            .unwrap_or_else(|| {
                let fallback = find_fallback_target_version(root);
                if let Some(fallback) = fallback {
                    tracing::debug!(
                        "Derived `target-version` from found `requires-python` for fallback configuration: {fallback:?}"
                    );
                }

                let configuration = Configuration {
                    target_version: fallback.map(Into::into),
                    ..Configuration::default()
                };
                Self::with_editor_settings(editor_settings, root, configuration).expect(
                    "editor configuration should merge successfully with default configuration",
                )
            })
    }

    /// Constructs [`RuffSettings`] by merging the editor-defined settings with the
    /// default configuration.
    fn editor_only(editor_settings: &ResolvedEditorSettings, root: &Path) -> RuffSettings {
        Self::with_editor_settings(editor_settings, root, Configuration::default())
            .expect("editor configuration should merge successfully with default configuration")
    }

    /// Merges the `configuration` with the editor defined settings.
    fn with_editor_settings(
        editor_settings: &ResolvedEditorSettings,
        root: &Path,
        configuration: Configuration,
    ) -> anyhow::Result<RuffSettings> {
        let settings = EditorConfigurationTransformer(editor_settings, root)
            .transform(configuration)
            .into_settings(root)?;

        Ok(RuffSettings {
            path: None,
            settings,
        })
    }
}

impl RuffSettingsIndex {
    /// Create the settings index for the given workspace root.
    ///
    /// This will create the index in the following order:
    /// 1. Resolve any settings from above the workspace root
    /// 2. Resolve any settings from the workspace root itself
    /// 3. Resolve any settings from within the workspace directory tree
    ///
    /// If this is the default workspace i.e., the client did not specify any workspace and so the
    /// server will be running in a single file mode, then only (1) and (2) will be resolved,
    /// skipping (3).
    pub(super) fn new(
        root: &Path,
        editor_settings: &ResolvedEditorSettings,
        is_default_workspace: bool,
    ) -> Self {
        if editor_settings.configuration_preference == ConfigurationPreference::EditorOnly {
            tracing::debug!(
                "Using editor-only settings for workspace: {} (skipped indexing)",
                root.display()
            );
            return RuffSettingsIndex {
                index: BTreeMap::default(),
                fallback: Arc::new(RuffSettings::editor_only(editor_settings, root)),
            };
        }

        tracing::debug!("Indexing settings for workspace: {}", root.display());

        let mut has_error = false;
        let mut respect_gitignore = None;
        let mut index = BTreeMap::default();

        // If this is *not* the default workspace, then we should skip the workspace root itself
        // because it will be resolved when walking the workspace directory tree. This is done by
        // the `WalkBuilder` below.
        let should_skip_workspace = usize::from(!is_default_workspace);

        // Add any settings from above the workspace root, skipping the workspace root itself if
        // this is *not* the default workspace.
        for directory in root.ancestors().skip(should_skip_workspace) {
            match settings_toml(directory) {
                Ok(Some(pyproject)) => {
                    match ruff_workspace::resolver::resolve_root_settings(
                        &pyproject,
                        &EditorConfigurationTransformer(editor_settings, root),
                        ruff_workspace::resolver::ConfigurationOrigin::Ancestor,
                    ) {
                        Ok(settings) => {
                            tracing::debug!("Loaded settings from: `{}`", pyproject.display());
                            respect_gitignore = Some(settings.file_resolver.respect_gitignore);

                            index.insert(
                                directory.to_path_buf(),
                                Arc::new(RuffSettings {
                                    path: Some(pyproject),
                                    settings,
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

        // If this is the default workspace, the server is running in single-file mode. What this
        // means is that the user opened a file directly (not the folder) in the editor and the
        // server didn't receive a workspace folder during initialization. In this case, we default
        // to the current working directory and skip walking the workspace directory tree for any
        // settings.
        //
        // Refer to https://github.com/astral-sh/ruff/pull/13770 to understand what this behavior
        // means for different editors.
        if is_default_workspace {
            if has_error {
                show_err_msg!(
                    "Error while resolving settings from workspace {}. Please refer to the logs for more details.",
                    root.display()
                );
            }

            return RuffSettingsIndex { index, fallback };
        }

        // Add any settings within the workspace itself
        let mut builder = WalkBuilder::new(root);
        builder.standard_filters(
            respect_gitignore.unwrap_or_else(|| fallback.file_resolver.respect_gitignore),
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
                            &EditorConfigurationTransformer(editor_settings, root),
                            ruff_workspace::resolver::ConfigurationOrigin::Ancestor,
                        ) {
                            Ok(settings) => {
                                tracing::debug!(
                                    "Loaded settings from: `{}` for `{}`",
                                    pyproject.display(),
                                    directory.display()
                                );
                                index.write().unwrap().insert(
                                    directory,
                                    Arc::new(RuffSettings {
                                        path: Some(pyproject),
                                        settings,
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
            show_err_msg!(
                "Error while resolving settings from workspace {}. Please refer to the logs for more details.",
                root.display()
            );
        }

        RuffSettingsIndex {
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

    pub(super) fn fallback(&self) -> Arc<RuffSettings> {
        self.fallback.clone()
    }

    /// Returns an iterator over the paths to the configuration files in the index.
    pub(crate) fn config_file_paths(&self) -> impl Iterator<Item = &Path> {
        self.index
            .values()
            .filter_map(|settings| settings.path.as_deref())
    }
}

struct EditorConfigurationTransformer<'a>(&'a ResolvedEditorSettings, &'a Path);

impl ConfigurationTransformer for EditorConfigurationTransformer<'_> {
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
                        let absolute = GlobPath::normalize(&pattern, project_root);
                        FilePattern::User(pattern, absolute)
                    })
                    .collect()
            }),
            line_length,
            ..Configuration::default()
        };

        // Merge in the editor-specified configuration.
        let editor_configuration = if let Some(configuration) = configuration {
            match configuration {
                ResolvedConfiguration::FilePath(path) => {
                    tracing::debug!(
                        "Combining settings from editor-specified configuration file at: {}",
                        path.display()
                    );
                    match open_configuration_file(&path) {
                        Ok(config_from_file) => editor_configuration.combine(config_from_file),
                        err => {
                            tracing::error!(
                                "{:?}",
                                err.context("Unable to load editor-specified configuration file")
                                    .unwrap_err()
                            );
                            editor_configuration
                        }
                    }
                }
                ResolvedConfiguration::Inline(options) => {
                    tracing::debug!(
                        "Combining settings from editor-specified inline configuration"
                    );
                    match Configuration::from_options(options, None, project_root) {
                        Ok(configuration) => editor_configuration.combine(configuration),
                        Err(err) => {
                            tracing::error!(
                                "Unable to load editor-specified inline configuration: {err:?}",
                            );
                            editor_configuration
                        }
                    }
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
        &IdentityTransformer,
        ruff_workspace::resolver::ConfigurationOrigin::UserSpecified,
    )
}

struct IdentityTransformer;

impl ConfigurationTransformer for IdentityTransformer {
    fn transform(&self, config: Configuration) -> Configuration {
        config
    }
}

#[cfg(test)]
mod tests {
    use ruff_linter::line_width::LineLength;
    use ruff_workspace::options::Options;

    use super::*;

    /// This test ensures that the inline configuration is correctly applied to the configuration.
    #[test]
    fn inline_settings() {
        let editor_settings = ResolvedEditorSettings {
            configuration: Some(ResolvedConfiguration::Inline(Options {
                line_length: Some(LineLength::try_from(120).unwrap()),
                ..Default::default()
            })),
            ..Default::default()
        };

        let config = EditorConfigurationTransformer(&editor_settings, Path::new("/src/project"))
            .transform(Configuration::default());

        assert_eq!(config.line_length.unwrap().value(), 120);
    }

    /// This test ensures that between the inline configuration and specific settings, the specific
    /// settings is prioritized.
    #[test]
    fn inline_and_specific_settings_resolution_order() {
        let editor_settings = ResolvedEditorSettings {
            configuration: Some(ResolvedConfiguration::Inline(Options {
                line_length: Some(LineLength::try_from(120).unwrap()),
                ..Default::default()
            })),
            line_length: Some(LineLength::try_from(100).unwrap()),
            ..Default::default()
        };

        let config = EditorConfigurationTransformer(&editor_settings, Path::new("/src/project"))
            .transform(Configuration::default());

        assert_eq!(config.line_length.unwrap().value(), 100);
    }
}
