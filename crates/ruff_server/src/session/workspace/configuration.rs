use anyhow::anyhow;
use ruff_workspace::resolver::{ConfigurationTransformer, Relativity};
use std::path::Path;

#[derive(Default)]
pub(crate) struct RuffConfiguration {
    // settings to pass into the ruff linter
    pub(crate) linter: ruff_linter::settings::LinterSettings,
    // settings to pass into the ruff formatter
    pub(crate) formatter: ruff_workspace::FormatterSettings,
}

pub(crate) fn find_configuration_from_root(root: &Path) -> crate::Result<RuffConfiguration> {
    let pyproject = ruff_workspace::pyproject::find_settings_toml(root)?
        .ok_or_else(|| anyhow!("No pyproject.toml/ruff.toml/.ruff.toml file was found"))?;
    let settings = ruff_workspace::resolver::resolve_root_settings(
        &pyproject,
        Relativity::Parent,
        &LSPConfigTransformer,
    )?;
    Ok(RuffConfiguration {
        linter: settings.linter,
        formatter: settings.formatter,
    })
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
