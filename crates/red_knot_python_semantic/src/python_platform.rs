/// The target platform to assume when resolving types.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "kebab-case")
)]
pub enum PythonPlatform {
    /// Do not make any assumptions about the target platform.
    #[default]
    All,
    /// Assume a target platform like `linux`, `darwin`, `win32`, etc.
    #[cfg_attr(feature = "serde", serde(untagged))]
    Individual(String),
}
