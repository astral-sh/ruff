use std::fmt::{Display, Formatter};

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
    /// Assume a specific target platform like `linux`, `darwin` or `win32`.
    ///
    /// We use a string (instead of individual enum variants), as the set of possible platforms
    /// may change over time. See <https://docs.python.org/3/library/sys.html#sys.platform> for
    /// some known platform identifiers.
    #[cfg_attr(feature = "serde", serde(untagged))]
    Identifier(String),
}

impl Display for PythonPlatform {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PythonPlatform::All => f.write_str("all"),
            PythonPlatform::Identifier(name) => f.write_str(name),
        }
    }
}
