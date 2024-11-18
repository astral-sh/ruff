/// Enumeration of all supported Python versions
///
/// TODO: unify with the `PythonVersion` enum in the linter/formatter crates?
#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord, Default, clap::ValueEnum)]
pub enum TargetVersion {
    Py37,
    Py38,
    #[default]
    Py39,
    Py310,
    Py311,
    Py312,
    Py313,
}

impl TargetVersion {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Py37 => "py37",
            Self::Py38 => "py38",
            Self::Py39 => "py39",
            Self::Py310 => "py310",
            Self::Py311 => "py311",
            Self::Py312 => "py312",
            Self::Py313 => "py313",
        }
    }
}

impl std::fmt::Display for TargetVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<TargetVersion> for red_knot_python_semantic::PythonVersion {
    fn from(value: TargetVersion) -> Self {
        match value {
            TargetVersion::Py37 => Self::PY37,
            TargetVersion::Py38 => Self::PY38,
            TargetVersion::Py39 => Self::PY39,
            TargetVersion::Py310 => Self::PY310,
            TargetVersion::Py311 => Self::PY311,
            TargetVersion::Py312 => Self::PY312,
            TargetVersion::Py313 => Self::PY313,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::target_version::TargetVersion;
    use red_knot_python_semantic::PythonVersion;

    #[test]
    fn same_default_as_python_version() {
        assert_eq!(
            PythonVersion::from(TargetVersion::default()),
            PythonVersion::default()
        );
    }
}
