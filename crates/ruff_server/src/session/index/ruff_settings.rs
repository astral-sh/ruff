use ruff_linter::{
    display_settings, fs::normalize_path_to, settings::types::FilePattern,
    settings::types::PreviewMode,
};
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
use walkdir::{DirEntry, WalkDir};

use crate::session::settings::{ConfigurationPreference, ResolvedEditorSettings};

#[derive(Default)]
pub(crate) struct RuffSettings {
    /// Settings to pass into the Ruff linter.
    linter: ruff_linter::settings::LinterSettings,
    /// Settings to pass into the Ruff formatter.
    formatter: ruff_workspace::FormatterSettings,
}

pub(super) struct RuffSettingsIndex {
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
    pub(crate) fn linter(&self) -> &ruff_linter::settings::LinterSettings {
        &self.linter
    }

    pub(crate) fn formatter(&self) -> &ruff_workspace::FormatterSettings {
        &self.formatter
    }
}

impl RuffSettingsIndex {
    pub(super) fn new(root: &Path, editor_settings: &ResolvedEditorSettings) -> Self {
        let mut index = BTreeMap::default();

        for directory in WalkDir::new(root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_dir())
            .map(DirEntry::into_path)
        {
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
                        linter: settings.linter,
                        formatter: settings.formatter,
                    }),
                );
            }
        }

        let fallback = find_user_settings_toml()
            .and_then(|user_settings| {
                ruff_workspace::resolver::resolve_root_settings(
                    &user_settings,
                    Relativity::Cwd,
                    &EditorConfigurationTransformer(editor_settings, root),
                )
                .ok()
            })
            .unwrap_or_else(|| {
                let default_configuration = ruff_workspace::configuration::Configuration::default();
                EditorConfigurationTransformer(editor_settings, root)
                    .transform(default_configuration)
                    .into_settings(root)
                    .expect(
                        "editor configuration should merge successfully with default configuration",
                    )
            });

        Self {
            index,
            fallback: Arc::new(RuffSettings {
                formatter: fallback.formatter,
                linter: fallback.linter,
            }),
        }
    }

    pub(super) fn get(&self, document_path: &Path) -> Arc<RuffSettings> {
        if let Some((_, settings)) = self
            .index
            .range(..document_path.to_path_buf())
            .rev()
            .find(|(path, _)| document_path.starts_with(path))
        {
            return settings.clone();
        }

        tracing::info!(
            "No Ruff settings file found for {}; falling back to default configuration",
            document_path.display()
        );

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
