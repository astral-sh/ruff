use anyhow::anyhow;
use lsp_types::Url;
use ruff_workspace::resolver::{ConfigurationTransformer, Relativity};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Default)]
pub(crate) struct RuffSettings {
    // settings to pass into the ruff linter
    pub(crate) linter: ruff_linter::settings::LinterSettings,
    // settings to pass into the ruff formatter
    pub(crate) formatter: ruff_workspace::FormatterSettings,
}

#[derive(Default)]
pub(super) struct RuffSettingsIndex {
    index: BTreeMap<PathBuf, Arc<RuffSettings>>,
}

impl RuffSettingsIndex {
    pub(super) fn get_or_insert(&mut self, document_url: &Url) -> Arc<RuffSettings> {
        let document_path = document_url
            .to_file_path()
            .expect("document URL should be a valid path");
        let folder = document_path
            .parent()
            .expect("document URL should be a file path and have a parent");
        if let Some(config) = self.index.get(folder) {
            return config.clone();
        }

        let config = Arc::new(Self::find_configuration_at_path(folder).unwrap_or_else(|err| {
            tracing::error!("The following error occurred when trying to find a configuration file at `{}`:\n{err}", document_path.display());
            tracing::error!("Falling back to default configuration for `{}`", document_path.display());
            RuffSettings::default()
        }));

        self.index.insert(folder.to_path_buf(), config.clone());

        config
    }

    pub(super) fn clear(&mut self) {
        self.index.clear();
    }

    fn find_configuration_at_path(folder: &Path) -> crate::Result<RuffSettings> {
        let pyproject = ruff_workspace::pyproject::find_settings_toml(folder)?
            .ok_or_else(|| anyhow!("No pyproject.toml/ruff.toml/.ruff.toml file was found"))?;
        let settings = ruff_workspace::resolver::resolve_root_settings(
            &pyproject,
            Relativity::Parent,
            &LSPConfigTransformer,
        )?;
        Ok(RuffSettings {
            linter: settings.linter,
            formatter: settings.formatter,
        })
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
