use ruff_linter::{
    display_settings, fs::normalize_path_to, settings::types::FilePattern,
    settings::types::PreviewMode,
};
use ruff_workspace::{
    configuration::{Configuration, FormatConfiguration, LintConfiguration, RuleSelection},
    pyproject::settings_toml,
    resolver::{ConfigurationTransformer, Relativity},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use walkdir::{DirEntry, WalkDir};

use crate::session::settings::{ConfigResolutionStrategy, ResolvedEditorSettings};

#[derive(Default)]
pub(crate) struct RuffSettings {
    // settings to pass into the ruff linter
    linter: ruff_linter::settings::LinterSettings,
    // settings to pass into the ruff formatter
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

        Self {
            index,
            fallback: Arc::default(),
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

        tracing::info!("No ruff settings file (pyproject.toml/ruff.toml/.ruff.toml) found for {} - falling back to default configuration", document_path.display());

        self.fallback.clone()
    }
}

struct EditorConfigurationTransformer<'a>(&'a ResolvedEditorSettings, &'a Path);

impl<'a> ConfigurationTransformer for EditorConfigurationTransformer<'a> {
    fn transform(
        &self,
        project_configuration: ruff_workspace::configuration::Configuration,
    ) -> ruff_workspace::configuration::Configuration {
        let ResolvedEditorSettings {
            format_preview,
            lint_preview,
            select,
            extend_select,
            ignore,
            exclude,
            line_length,
            configuration_resolution_strategy,
        } = self.0.clone();

        let project_root = self.1;

        let editor_configuration = Configuration {
            lint: LintConfiguration {
                preview: lint_preview.map(PreviewMode::from),
                rule_selections: vec![RuleSelection {
                    select,
                    extend_select: extend_select.unwrap_or_default(),
                    ignore: ignore.unwrap_or_default(),
                    ..Default::default()
                }],
                ..Default::default()
            },
            format: FormatConfiguration {
                preview: format_preview.map(PreviewMode::from),
                ..Default::default()
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
            ..Default::default()
        };

        match configuration_resolution_strategy {
            ConfigResolutionStrategy::Default => {
                editor_configuration.combine(project_configuration)
            }
            ConfigResolutionStrategy::PrioritizeWorkspace => {
                project_configuration.combine(editor_configuration)
            }
        }
    }
}
