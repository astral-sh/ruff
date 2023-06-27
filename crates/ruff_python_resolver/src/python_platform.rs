/// Enum to represent a Python platform.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum PythonPlatform {
    Darwin,
    Linux,
    Windows,
}

impl PythonPlatform {
    /// Returns the platform-specific library names. These are the candidate names for the top-level
    /// subdirectory within a virtual environment that contains the `site-packages` directory
    /// (with a `pythonX.Y` directory in-between).
    pub(crate) fn lib_names(&self) -> &[&'static str] {
        match self {
            PythonPlatform::Darwin => &["lib"],
            PythonPlatform::Linux => &["lib", "lib64"],
            PythonPlatform::Windows => &["Lib"],
        }
    }
}
