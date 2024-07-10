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
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use walkdir::WalkDir;

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
    /// Index from folder to the resoled ruff settings.
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
                    if match_exclusion(&directory, file_name, &settings.file_resolver.exclude) {
                        tracing::debug!("Ignored path via `exclude`: {}", directory.display());

                        walker.skip_current_dir();
                        continue;
                    } else if match_exclusion(
                        &directory,
                        file_name,
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
