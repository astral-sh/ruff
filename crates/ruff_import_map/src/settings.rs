use ruff_linter::settings::types::ExtensionMapping;
use ruff_macros::CacheKey;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Default, Clone, CacheKey)]
pub struct ImportMapSettings {
    pub extension: ExtensionMapping,
    pub src: Vec<PathBuf>,
    pub direction: Direction,
}

impl fmt::Display for ImportMapSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // STOPSHIP(charlie): Add these.
        Ok(())
    }
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, CacheKey)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum Direction {
    /// Construct a map from module to its dependencies (i.e., the modules that it imports).
    #[default]
    Dependencies,
    /// Construct a map from module to its dependents (i.e., the modules that import it).
    Dependents,
}
