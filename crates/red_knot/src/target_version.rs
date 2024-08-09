/// Enumeration of all supported Python versions
///
/// TODO: unify with the `PythonVersion` enum in the linter/formatter crates?
#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord, Default, clap::ValueEnum)]
pub enum TargetVersion {
    Py37,
    #[default]
    Py38,
    Py39,
    Py310,
    Py311,
    Py312,
    Py313,
}

impl std::fmt::Display for TargetVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        red_knot_python_semantic::TargetVersion::from(*self).fmt(f)
    }
}

impl From<TargetVersion> for red_knot_python_semantic::TargetVersion {
    fn from(value: TargetVersion) -> Self {
        match value {
            TargetVersion::Py37 => Self::Py37,
            TargetVersion::Py38 => Self::Py38,
            TargetVersion::Py39 => Self::Py39,
            TargetVersion::Py310 => Self::Py310,
            TargetVersion::Py311 => Self::Py311,
            TargetVersion::Py312 => Self::Py312,
            TargetVersion::Py313 => Self::Py313,
        }
    }
}
