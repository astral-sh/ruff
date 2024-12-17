use serde::{Deserialize, Serialize};

/// The target platform to assume when resolving types.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PythonPlatform {
    /// Do not make any assumptions about the target platform.
    #[default]
    All,
    /// Assume a target platform like `linux`, `darwin`, `win32`, etc.
    #[serde(untagged)]
    Individual(String),
}
