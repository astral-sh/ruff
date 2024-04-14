use lsp_types::Url;
use ruff_workspace::resolver::ConfigurationTransformer;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

#[derive(Default)]
pub(crate) struct RuffConfiguration {
    // settings to pass into the ruff linter
    pub(crate) linter: ruff_linter::settings::LinterSettings,
    // settings to pass into the ruff formatter
    pub(crate) formatter: ruff_workspace::FormatterSettings,
}

#[derive(Default)]
pub(super) struct ConfigurationIndex {
    index: BTreeMap<PathBuf, Arc<RuffConfiguration>>,
}

impl ConfigurationIndex {
    pub(super) fn get_or_insert(&mut self, path: &Url) -> Arc<RuffConfiguration> {
        todo!("impl");
    }

    pub(super) fn clear(&mut self) {
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
