use std::fmt;
use std::str::FromStr;

use ruff_python_ast::{PythonVersion, PythonVersionDeserializationError};

/// A Python version explicitly supported by ty configuration and CLI parsing.
#[derive(
    Debug,
    Copy,
    Clone,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    get_size2::GetSize,
)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum SupportedPythonVersion {
    /// Python 3.7
    #[serde(rename = "3.7")]
    #[cfg_attr(feature = "clap", value(name = "3.7"))]
    Py37,
    /// Python 3.8
    #[serde(rename = "3.8")]
    #[cfg_attr(feature = "clap", value(name = "3.8"))]
    Py38,
    /// Python 3.9
    #[serde(rename = "3.9")]
    #[cfg_attr(feature = "clap", value(name = "3.9"))]
    Py39,
    /// Python 3.10
    #[serde(rename = "3.10")]
    #[cfg_attr(feature = "clap", value(name = "3.10"))]
    Py310,
    /// Python 3.11
    #[serde(rename = "3.11")]
    #[cfg_attr(feature = "clap", value(name = "3.11"))]
    Py311,
    /// Python 3.12
    #[serde(rename = "3.12")]
    #[cfg_attr(feature = "clap", value(name = "3.12"))]
    Py312,
    /// Python 3.13
    #[serde(rename = "3.13")]
    #[cfg_attr(feature = "clap", value(name = "3.13"))]
    Py313,
    /// Python 3.14
    #[serde(rename = "3.14")]
    #[cfg_attr(feature = "clap", value(name = "3.14"))]
    Py314,
    /// Python 3.15
    #[serde(rename = "3.15")]
    #[cfg_attr(feature = "clap", value(name = "3.15"))]
    Py315,
}

impl SupportedPythonVersion {
    pub fn iter() -> impl Iterator<Item = Self> {
        [
            Self::Py37,
            Self::Py38,
            Self::Py39,
            Self::Py310,
            Self::Py311,
            Self::Py312,
            Self::Py313,
            Self::Py314,
            Self::Py315,
        ]
        .into_iter()
    }

    pub const fn as_str(self) -> &'static str {
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

    pub const fn into_inner(self) -> PythonVersion {
        match self {
            Self::Py37 => PythonVersion::PY37,
            Self::Py38 => PythonVersion::PY38,
            Self::Py39 => PythonVersion::PY39,
            Self::Py310 => PythonVersion::PY310,
            Self::Py311 => PythonVersion::PY311,
            Self::Py312 => PythonVersion::PY312,
            Self::Py313 => PythonVersion::PY313,
            Self::Py314 => PythonVersion::PY314,
            Self::Py315 => PythonVersion::PY315,
        }
    }
}

impl fmt::Display for SupportedPythonVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<SupportedPythonVersion> for PythonVersion {
    fn from(value: SupportedPythonVersion) -> Self {
        value.into_inner()
    }
}

impl ty_combine::Combine for SupportedPythonVersion {
    fn combine_with(&mut self, _other: Self) {}
}

impl TryFrom<PythonVersion> for SupportedPythonVersion {
    type Error = PythonVersion;

    fn try_from(value: PythonVersion) -> Result<Self, Self::Error> {
        match value {
            PythonVersion::PY37 => Ok(Self::Py37),
            PythonVersion::PY38 => Ok(Self::Py38),
            PythonVersion::PY39 => Ok(Self::Py39),
            PythonVersion::PY310 => Ok(Self::Py310),
            PythonVersion::PY311 => Ok(Self::Py311),
            PythonVersion::PY312 => Ok(Self::Py312),
            PythonVersion::PY313 => Ok(Self::Py313),
            PythonVersion::PY314 => Ok(Self::Py314),
            PythonVersion::PY315 => Ok(Self::Py315),
            _ => Err(value),
        }
    }
}

impl FromStr for SupportedPythonVersion {
    type Err = SupportedPythonVersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let version = PythonVersion::from_str(s).map_err(SupportedPythonVersionError::Parse)?;

        Self::try_from(version).map_err(SupportedPythonVersionError::Unsupported)
    }
}

const EXPECTED_SUPPORTED_PYTHON_VERSIONS: &str =
    "`3.7`, `3.8`, `3.9`, `3.10`, `3.11`, `3.12`, `3.13`, `3.14`, `3.15`";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SupportedPythonVersionError {
    #[error(transparent)]
    Parse(#[from] PythonVersionDeserializationError),
    #[error(
        "unsupported value `{0}` for `python-version`; expected one of {expected}",
        expected = EXPECTED_SUPPORTED_PYTHON_VERSIONS
    )]
    Unsupported(PythonVersion),
}
