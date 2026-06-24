use ruff_linter::display_settings;
use ruff_linter::settings::types::{ExtensionMapping, FilePatternSet, PreviewMode};
use ruff_macros::CacheKey;
use ruff_python_ast::PythonVersion;
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, CacheKey)]
pub struct AnalyzeSettings {
    pub exclude: FilePatternSet,
    pub preview: PreviewMode,
    pub target_version: PythonVersion,
    pub string_imports: StringImports,
    pub include_dependencies: BTreeMap<PathBuf, (PathBuf, Vec<String>)>,
    pub extension: ExtensionMapping,
    pub type_checking_imports: bool,
}

impl Default for AnalyzeSettings {
    fn default() -> Self {
        Self {
            exclude: FilePatternSet::default(),
            preview: PreviewMode::default(),
            target_version: PythonVersion::default(),
            string_imports: StringImports::default(),
            include_dependencies: BTreeMap::default(),
            extension: ExtensionMapping::default(),
            type_checking_imports: true,
        }
    }
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
                self.string_imports,
                self.extension | debug,
                self.include_dependencies | debug,
                self.type_checking_imports,
            ]
        }
        Ok(())
    }
}

#[derive(Debug, Copy, Clone, CacheKey)]
pub struct StringImports {
    pub enabled: bool,
    pub min_dots: usize,
}

impl Default for StringImports {
    fn default() -> Self {
        Self {
            enabled: false,
            min_dots: 2,
        }
    }
}

impl fmt::Display for StringImports {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.enabled {
            write!(f, "enabled (min_dots: {})", self.min_dots)
        } else {
            write!(f, "disabled")
        }
    }
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, CacheKey)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "kebab-case")
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum Direction {
    /// Construct a map from module to its dependencies (i.e., the modules that it imports).
    #[default]
    #[cfg_attr(feature = "serde", serde(alias = "Dependencies"))]
    Dependencies,
    /// Construct a map from module to its dependents (i.e., the modules that import it).
    #[cfg_attr(feature = "serde", serde(alias = "Dependents"))]
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
