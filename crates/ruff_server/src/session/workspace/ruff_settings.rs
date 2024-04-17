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

#[derive(Default)]
pub(crate) struct RuffSettings {
    // settings to pass into the ruff linter
    pub(crate) linter: ruff_linter::settings::LinterSettings,
    // settings to pass into the ruff formatter
    pub(crate) formatter: ruff_workspace::FormatterSettings,
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

impl RuffSettingsIndex {
    pub(super) fn new(root: &Path) -> Self {
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
                    &LSPConfigTransformer,
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

struct LSPConfigTransformer;

impl ConfigurationTransformer for LSPConfigTransformer {
    fn transform(
        &self,
        config: ruff_workspace::configuration::Configuration,
    ) -> ruff_workspace::configuration::Configuration {
        config
    }
}
