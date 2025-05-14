use ruff_linter::display_settings;
use ruff_linter::settings::types::{ExtensionMapping, FilePatternSet, PreviewMode};
use ruff_macros::CacheKey;
use ruff_python_ast::PythonVersion;
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Default, Clone, CacheKey)]
pub struct AnalyzeSettings {
    pub exclude: FilePatternSet,
    pub preview: PreviewMode,
    pub target_version: PythonVersion,
    pub detect_string_imports: bool,
    pub include_dependencies: BTreeMap<PathBuf, (PathBuf, Vec<String>)>,
    pub extension: ExtensionMapping,
}

impl fmt::Display for AnalyzeSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "\n# Analyze Settings")?;
        display_settings! {
            formatter = f,
            namespace = "analyze",
            fields = [
                self.exclude,
                self.preview,
                self.target_version,
                self.detect_string_imports,
                self.extension | debug,
                self.include_dependencies | debug,
            ]
        }
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

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dependencies => write!(f, "\"dependencies\""),
            Self::Dependents => write!(f, "\"dependents\""),
        }
    }
}
