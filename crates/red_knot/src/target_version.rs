use std::fmt;

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

impl fmt::Display for TargetVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<TargetVersion> for red_knot_module_resolver::TargetVersion {
    fn from(value: TargetVersion) -> Self {
        match value {
            TargetVersion::Py37 => red_knot_module_resolver::TargetVersion::Py37,
            TargetVersion::Py38 => red_knot_module_resolver::TargetVersion::Py38,
            TargetVersion::Py39 => red_knot_module_resolver::TargetVersion::Py39,
            TargetVersion::Py310 => red_knot_module_resolver::TargetVersion::Py310,
            TargetVersion::Py311 => red_knot_module_resolver::TargetVersion::Py311,
            TargetVersion::Py312 => red_knot_module_resolver::TargetVersion::Py312,
            TargetVersion::Py313 => red_knot_module_resolver::TargetVersion::Py313,
        }
    }
}
