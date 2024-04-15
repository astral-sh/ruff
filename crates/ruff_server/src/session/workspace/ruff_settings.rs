use ruff_workspace::{
    pyproject::settings_toml,
    resolver::{ConfigurationTransformer, Relativity},
};
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
    fallback: Arc<RuffSettings>,
}

impl RuffSettingsIndex {
    pub(super) fn reload(&mut self, root: &Path) {
        self.clear();

        for directory in std::fs::read_dir(root).unwrap().filter_map(|entry| {
            entry
                .ok()
                .and_then(|entry| entry.file_type().ok()?.is_dir().then_some(entry))
        }) {
            let path = directory.path();
            if let Some(pyproject) = settings_toml(&path).ok().flatten() {
                let Ok(settings) = ruff_workspace::resolver::resolve_root_settings(
                    &pyproject,
                    Relativity::Parent,
                    &LSPConfigTransformer,
                ) else {
                    continue;
                };
                self.index.insert(
                    path,
                    Arc::new(RuffSettings {
                        linter: settings.linter,
                        formatter: settings.formatter,
                    }),
                );
            }
        }
    }

    pub(super) fn get(&self, document_path: &Path) -> &Arc<RuffSettings> {
        if let Some((_, ruff_settings)) =
            self.index.range(..document_path.to_path_buf()).next_back()
        {
            return ruff_settings;
        }

        tracing::warn!("No ruff settings file (pyproject.toml/ruff.toml/.ruff.toml) found for {} - falling back to default configuration", document_path.display());

        &self.fallback
    }

    fn clear(&mut self) {
        self.index.clear();
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
