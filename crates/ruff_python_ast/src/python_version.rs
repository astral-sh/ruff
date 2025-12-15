use std::{fmt, str::FromStr};

/// Representation of a Python version.
///
/// N.B. This does not necessarily represent a Python version that we actually support.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "cache", derive(ruff_macros::CacheKey))]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct PythonVersion {
    pub major: u8,
    pub minor: u8,
}

impl PythonVersion {
    pub const PY37: PythonVersion = PythonVersion { major: 3, minor: 7 };
    pub const PY38: PythonVersion = PythonVersion { major: 3, minor: 8 };
    pub const PY39: PythonVersion = PythonVersion { major: 3, minor: 9 };
    pub const PY310: PythonVersion = PythonVersion {
        major: 3,
        minor: 10,
    };
    pub const PY311: PythonVersion = PythonVersion {
        major: 3,
        minor: 11,
    };
    pub const PY312: PythonVersion = PythonVersion {
        major: 3,
        minor: 12,
    };
    pub const PY313: PythonVersion = PythonVersion {
        major: 3,
        minor: 13,
    };
    pub const PY314: PythonVersion = PythonVersion {
        major: 3,
        minor: 14,
    };

    pub fn iter() -> impl Iterator<Item = PythonVersion> {
        [
            PythonVersion::PY37,
            PythonVersion::PY38,
            PythonVersion::PY39,
            PythonVersion::PY310,
            PythonVersion::PY311,
            PythonVersion::PY312,
            PythonVersion::PY313,
            PythonVersion::PY314,
        ]
        .into_iter()
    }

    /// The minimum supported Python version.
    pub const fn lowest() -> Self {
        Self::PY37
    }

    pub const fn latest() -> Self {
        Self::PY314
    }

    /// The latest Python version supported in preview
    pub fn latest_preview() -> Self {
        let latest_preview = Self::PY314;
        debug_assert!(latest_preview >= Self::latest());
        latest_preview
    }

    pub const fn latest_ty() -> Self {
        // Make sure to update the default value for `EnvironmentOptions::python_version` when bumping this version.
        Self::PY314
    }

    pub const fn as_tuple(self) -> (u8, u8) {
        (self.major, self.minor)
    }

    pub fn free_threaded_build_available(self) -> bool {
        self >= PythonVersion::PY313
    }

    /// Return `true` if the current version supports [PEP 701].
    ///
    /// [PEP 701]: https://peps.python.org/pep-0701/
    pub fn supports_pep_701(self) -> bool {
        self >= Self::PY312
    }

    pub fn defers_annotations(self) -> bool {
        self >= Self::PY314
    }
}

impl Default for PythonVersion {
    fn default() -> Self {
        Self::PY310
    }
}

impl From<(u8, u8)> for PythonVersion {
    fn from(value: (u8, u8)) -> Self {
        let (major, minor) = value;
        Self { major, minor }
    }
}

impl fmt::Display for PythonVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let PythonVersion { major, minor } = self;
        write!(f, "{major}.{minor}")
    }
}

#[derive(thiserror::Error, Debug, PartialEq, Eq, Clone)]
pub enum PythonVersionDeserializationError {
    #[error("Invalid python version `{0}`: expected `major.minor`")]
    WrongPeriodNumber(Box<str>),
    #[error("Invalid major version `{0}`: {1}")]
    InvalidMajorVersion(Box<str>, #[source] std::num::ParseIntError),
    #[error("Invalid minor version `{0}`: {1}")]
    InvalidMinorVersion(Box<str>, #[source] std::num::ParseIntError),
}

impl TryFrom<(&str, &str)> for PythonVersion {
    type Error = PythonVersionDeserializationError;

    fn try_from(value: (&str, &str)) -> Result<Self, Self::Error> {
        let (major, minor) = value;
        Ok(Self {
            major: major.parse().map_err(|err| {
                PythonVersionDeserializationError::InvalidMajorVersion(Box::from(major), err)
            })?,
            minor: minor.parse().map_err(|err| {
                PythonVersionDeserializationError::InvalidMinorVersion(Box::from(minor), err)
            })?,
        })
    }
}

impl FromStr for PythonVersion {
    type Err = PythonVersionDeserializationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (major, minor) = s
            .split_once('.')
            .ok_or_else(|| PythonVersionDeserializationError::WrongPeriodNumber(Box::from(s)))?;

        Self::try_from((major, minor)).map_err(|err| {
            // Give a better error message for something like `3.8.5` or `3..8`
            if matches!(
                err,
                PythonVersionDeserializationError::InvalidMinorVersion(_, _)
            ) && minor.contains('.')
            {
                PythonVersionDeserializationError::WrongPeriodNumber(Box::from(s))
            } else {
                err
            }
        })
    }
}

#[cfg(feature = "serde")]
mod serde {
    use super::PythonVersion;

    impl<'de> serde::Deserialize<'de> for PythonVersion {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            String::deserialize(deserializer)?
                .parse()
                .map_err(serde::de::Error::custom)
        }
    }

    impl serde::Serialize for PythonVersion {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            serializer.serialize_str(&self.to_string())
        }
    }
}

#[cfg(feature = "schemars")]
mod schemars {
    use super::PythonVersion;
    use schemars::{JsonSchema, Schema, SchemaGenerator};
    use serde_json::Value;

    impl JsonSchema for PythonVersion {
        fn schema_name() -> std::borrow::Cow<'static, str> {
            std::borrow::Cow::Borrowed("PythonVersion")
        }

        fn json_schema(_gen: &mut SchemaGenerator) -> Schema {
            let mut any_of: Vec<Value> = vec![
                schemars::json_schema!({
                    "type": "string",
                    "pattern": r"^\d+\.\d+$",
                })
                .into(),
            ];

            for version in Self::iter() {
                let mut schema = schemars::json_schema!({
                    "const": version.to_string(),
                });
                schema.ensure_object().insert(
                    "description".to_string(),
                    Value::String(format!("Python {version}")),
                );
                any_of.push(schema.into());
            }

            let mut schema = Schema::default();
            schema
                .ensure_object()
                .insert("anyOf".to_string(), Value::Array(any_of));
            schema
        }
    }
}
