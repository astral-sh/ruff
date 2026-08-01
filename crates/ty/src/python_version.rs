/// Enumeration of the Python versions accepted by the ty CLI.
#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub(crate) enum PythonVersion {
    #[value(name = "3.7")]
    Py37,
    #[value(name = "3.8")]
    Py38,
    #[value(name = "3.9")]
    Py39,
    #[value(name = "3.10")]
    Py310,
    #[value(name = "3.11")]
    Py311,
    #[value(name = "3.12")]
    Py312,
    #[value(name = "3.13")]
    Py313,
    #[value(name = "3.14")]
    Py314,
    #[value(name = "3.15")]
    Py315,
}

impl PythonVersion {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Py37 => "3.7",
            Self::Py38 => "3.8",
            Self::Py39 => "3.9",
            Self::Py310 => "3.10",
            Self::Py311 => "3.11",
            Self::Py312 => "3.12",
            Self::Py313 => "3.13",
            Self::Py314 => "3.14",
            Self::Py315 => "3.15",
        }
    }
}

impl std::fmt::Display for PythonVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<PythonVersion> for ty_project::metadata::python_version::SupportedPythonVersion {
    fn from(value: PythonVersion) -> Self {
        match value {
            PythonVersion::Py37 => Self::Py37,
            PythonVersion::Py38 => Self::Py38,
            PythonVersion::Py39 => Self::Py39,
            PythonVersion::Py310 => Self::Py310,
            PythonVersion::Py311 => Self::Py311,
            PythonVersion::Py312 => Self::Py312,
            PythonVersion::Py313 => Self::Py313,
            PythonVersion::Py314 => Self::Py314,
            PythonVersion::Py315 => Self::Py315,
        }
    }
}
