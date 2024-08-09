use std::fmt;

/// Enumeration of all supported Python versions
///
/// TODO: unify with the `PythonVersion` enum in the linter/formatter crates?
#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Default)]
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
    pub fn major_version(self) -> u8 {
        PythonVersion::from(self).major
    }

    pub fn minor_version(self) -> u8 {
        PythonVersion::from(self).minor
    }

    const fn as_display_str(self) -> &'static str {
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
        f.write_str(self.as_display_str())
    }
}

impl fmt::Debug for TargetVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Generic representation for a Python version.
///
/// Unlike [`TargetVersion`], this does not necessarily represent
/// a Python version that we actually support.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct PythonVersion {
    pub major: u8,
    pub minor: u8,
}

impl TryFrom<(&str, &str)> for PythonVersion {
    type Error = std::num::ParseIntError;

    fn try_from(value: (&str, &str)) -> Result<Self, Self::Error> {
        let (major, minor) = value;
        Ok(Self {
            major: major.parse()?,
            minor: minor.parse()?,
        })
    }
}

impl fmt::Display for PythonVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let PythonVersion { major, minor } = self;
        write!(f, "{major}.{minor}")
    }
}

impl From<TargetVersion> for PythonVersion {
    fn from(value: TargetVersion) -> Self {
        match value {
            TargetVersion::Py37 => PythonVersion { major: 3, minor: 7 },
            TargetVersion::Py38 => PythonVersion { major: 3, minor: 8 },
            TargetVersion::Py39 => PythonVersion { major: 3, minor: 9 },
            TargetVersion::Py310 => PythonVersion {
                major: 3,
                minor: 10,
            },
            TargetVersion::Py311 => PythonVersion {
                major: 3,
                minor: 11,
            },
            TargetVersion::Py312 => PythonVersion {
                major: 3,
                minor: 12,
            },
            TargetVersion::Py313 => PythonVersion {
                major: 3,
                minor: 13,
            },
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct UnsupportedPythonVersion(PythonVersion);

impl fmt::Display for UnsupportedPythonVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Python version {} is unsupported", self.0)
    }
}

impl std::error::Error for UnsupportedPythonVersion {}

impl TryFrom<PythonVersion> for TargetVersion {
    type Error = UnsupportedPythonVersion;

    fn try_from(value: PythonVersion) -> Result<Self, Self::Error> {
        let PythonVersion { major: 3, minor } = value else {
            return Err(UnsupportedPythonVersion(value));
        };
        match minor {
            7 => Ok(TargetVersion::Py37),
            8 => Ok(TargetVersion::Py38),
            9 => Ok(TargetVersion::Py39),
            10 => Ok(TargetVersion::Py310),
            11 => Ok(TargetVersion::Py311),
            12 => Ok(TargetVersion::Py312),
            13 => Ok(TargetVersion::Py313),
            _ => Err(UnsupportedPythonVersion(value)),
        }
    }
}
