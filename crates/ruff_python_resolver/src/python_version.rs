/// Enum to represent a Python version.
#[derive(Debug, Copy, Clone)]
pub(crate) enum PythonVersion {
    Py37,
    Py38,
    Py39,
    Py310,
    Py311,
    Py312,
}

impl PythonVersion {
    /// The directory name (e.g., in a virtual environment) for this Python version.
    pub(crate) fn dir(self) -> &'static str {
        match self {
            PythonVersion::Py37 => "python3.7",
            PythonVersion::Py38 => "python3.8",
            PythonVersion::Py39 => "python3.9",
            PythonVersion::Py310 => "python3.10",
            PythonVersion::Py311 => "python3.11",
            PythonVersion::Py312 => "python3.12",
        }
    }
}
