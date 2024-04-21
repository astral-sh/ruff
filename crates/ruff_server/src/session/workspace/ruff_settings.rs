use ruff_linter::display_settings;
use ruff_workspace::{
    pyproject::settings_toml,
    resolver::{ConfigurationTransformer, Relativity},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use walkdir::{DirEntry, WalkDir};

use crate::session::settings::ResolvedEditorSettings;

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
    pub(crate) fn resolve(
        project_root: &Path,
        configuration: ruff_workspace::configuration::Configuration,
        editor_settings: &ResolvedEditorSettings,
    ) -> crate::Result<Self> {
        let settings = editor_settings.resolve(project_root, configuration)?;
        Ok(Self {
            linter: settings.linter,
            formatter: settings.formatter,
        })
    }

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
                let Ok(configuration) = ruff_workspace::resolver::resolve_configuration(
                    &pyproject,
                    Relativity::Parent,
                    &LSPConfigTransformer,
                ) else {
                    continue;
                };
                let Ok(settings) = RuffSettings::resolve(root, configuration, editor_settings)
                else {
                    continue;
                };
                index.insert(directory, Arc::new(settings));
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

struct LSPConfigTransformer;

impl ConfigurationTransformer for LSPConfigTransformer {
    fn transform(
        &self,
        config: ruff_workspace::configuration::Configuration,
    ) -> ruff_workspace::configuration::Configuration {
        config
    }
}
