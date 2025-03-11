/// Enumeration of all supported Python versions
///
/// TODO: unify with the `PythonVersion` enum in the linter/formatter crates?
#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord, Default, clap::ValueEnum)]
pub enum PythonVersion {
    #[value(name = "3.7")]
    Py37,
    #[value(name = "3.8")]
    Py38,
    #[default]
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
        }
    }
}

impl std::fmt::Display for PythonVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<PythonVersion> for ruff_python_ast::PythonVersion {
    fn from(value: PythonVersion) -> Self {
        match value {
            PythonVersion::Py37 => Self::PY37,
            PythonVersion::Py38 => Self::PY38,
            PythonVersion::Py39 => Self::PY39,
            PythonVersion::Py310 => Self::PY310,
            PythonVersion::Py311 => Self::PY311,
            PythonVersion::Py312 => Self::PY312,
            PythonVersion::Py313 => Self::PY313,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::python_version::PythonVersion;

    #[test]
    fn same_default_as_python_version() {
        assert_eq!(
            ruff_python_ast::PythonVersion::from(PythonVersion::default()),
            ruff_python_ast::PythonVersion::default()
        );
    }
}
