use std::fmt;

pub use pyversion::PyVersion;

mod pyversion {
    use log::debug;
    use strum::IntoEnumIterator;
    use strum_macros::EnumIter;

    use pep440_rs::{Operator, Version as Pep440Version, Version, VersionSpecifiers};

    /// Representation of supported Python versions.
    ///
    /// Unlike [`PythonVersion`](super::PythonVersion), this is deserialized from versions like
    /// `py39` rather than dotted versions like `3.9`.
    #[derive(Clone, Copy, Debug, PartialOrd, Ord, PartialEq, Eq, Default, EnumIter)]
    #[cfg_attr(feature = "ruff_macros", derive(ruff_macros::CacheKey))]
    #[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
    #[cfg_attr(
        feature = "serde",
        derive(serde::Serialize, serde::Deserialize),
        serde(rename_all = "lowercase")
    )]
    #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
    pub enum PyVersion {
        Py37,
        Py38,
        #[default]
        Py39,
        Py310,
        Py311,
        Py312,
        Py313,
        // Remember to update the `latest()` function
        // when adding new versions here!
    }

    impl From<PyVersion> for Pep440Version {
        fn from(version: PyVersion) -> Self {
            let (major, minor) = version.as_tuple();
            Self::new([u64::from(major), u64::from(minor)])
        }
    }

    impl PyVersion {
        /// Return the latest supported Python version.
        pub const fn latest() -> Self {
            Self::Py313
        }

        pub const fn minimal_supported() -> Self {
            Self::Py37
        }

        pub const fn as_tuple(self) -> (u8, u8) {
            match self {
                Self::Py37 => (3, 7),
                Self::Py38 => (3, 8),
                Self::Py39 => (3, 9),
                Self::Py310 => (3, 10),
                Self::Py311 => (3, 11),
                Self::Py312 => (3, 12),
                Self::Py313 => (3, 13),
            }
        }

        pub const fn major(self) -> u8 {
            self.as_tuple().0
        }

        pub const fn minor(self) -> u8 {
            self.as_tuple().1
        }

        /// Infer the minimum supported [`PyVersion`] from a `requires-python` specifier.
        pub fn get_minimum_supported_version(requires_version: &VersionSpecifiers) -> Option<Self> {
            /// Truncate a version to its major and minor components.
            fn major_minor(version: &Version) -> Option<Version> {
                let major = version.release().first()?;
                let minor = version.release().get(1)?;
                Some(Version::new([major, minor]))
            }

            // Extract the minimum supported version from the specifiers.
            let minimum_version = requires_version
                .iter()
                .filter(|specifier| {
                    matches!(
                        specifier.operator(),
                        Operator::Equal
                            | Operator::EqualStar
                            | Operator::ExactEqual
                            | Operator::TildeEqual
                            | Operator::GreaterThan
                            | Operator::GreaterThanEqual
                    )
                })
                .filter_map(|specifier| major_minor(specifier.version()))
                .min()?;

            debug!("Detected minimum supported `requires-python` version: {minimum_version}");

            // Find the Python version that matches the minimum supported version.
            PyVersion::iter().find(|version| Version::from(*version) == minimum_version)
        }

        /// Return `true` if the current version supports [PEP 701].
        ///
        /// [PEP 701]: https://peps.python.org/pep-0701/
        pub fn supports_pep701(self) -> bool {
            self >= Self::Py312
        }
    }
}

/// Representation of a Python version.
///
/// Unlike the [`PyVersion`], this does not necessarily represent a Python version that we actually
/// support.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
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

    pub fn iter() -> impl Iterator<Item = PythonVersion> {
        [
            PythonVersion::PY37,
            PythonVersion::PY38,
            PythonVersion::PY39,
            PythonVersion::PY310,
            PythonVersion::PY311,
            PythonVersion::PY312,
            PythonVersion::PY313,
        ]
        .iter()
        .copied()
    }

    pub fn free_threaded_build_available(self) -> bool {
        self >= PythonVersion::PY313
    }
}

impl Default for PythonVersion {
    fn default() -> Self {
        Self::PY39
    }
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

#[cfg(feature = "serde")]
mod serde {
    use super::PythonVersion;

    impl<'de> serde::Deserialize<'de> for PythonVersion {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            let as_str = String::deserialize(deserializer)?;

            if let Some((major, minor)) = as_str.split_once('.') {
                let major = major.parse().map_err(|err| {
                    serde::de::Error::custom(format!("invalid major version: {err}"))
                })?;
                let minor = minor.parse().map_err(|err| {
                    serde::de::Error::custom(format!("invalid minor version: {err}"))
                })?;

                Ok((major, minor).into())
            } else {
                let major = as_str.parse().map_err(|err| {
                    serde::de::Error::custom(format!(
                        "invalid python-version: {err}, expected: `major.minor`"
                    ))
                })?;

                Ok((major, 0).into())
            }
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
    use schemars::schema::{Metadata, Schema, SchemaObject, SubschemaValidation};
    use schemars::JsonSchema;
    use schemars::_serde_json::Value;

    impl JsonSchema for PythonVersion {
        fn schema_name() -> String {
            "PythonVersion".to_string()
        }

        fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> Schema {
            let sub_schemas = std::iter::once(Schema::Object(SchemaObject {
                instance_type: Some(schemars::schema::InstanceType::String.into()),
                string: Some(Box::new(schemars::schema::StringValidation {
                    pattern: Some(r"^\d+\.\d+$".to_string()),
                    ..Default::default()
                })),
                ..Default::default()
            }))
            .chain(Self::iter().map(|v| {
                Schema::Object(SchemaObject {
                    const_value: Some(Value::String(v.to_string())),
                    metadata: Some(Box::new(Metadata {
                        description: Some(format!("Python {v}")),
                        ..Metadata::default()
                    })),
                    ..SchemaObject::default()
                })
            }));

            Schema::Object(SchemaObject {
                subschemas: Some(Box::new(SubschemaValidation {
                    any_of: Some(sub_schemas.collect()),
                    ..Default::default()
                })),
                ..SchemaObject::default()
            })
        }
    }
}

#[cfg(feature = "clap")]
mod clap {
    use clap::builder::PossibleValue;

    impl clap::ValueEnum for super::PythonVersion {
        fn value_variants<'a>() -> &'a [Self] {
            &[
                Self::PY37,
                Self::PY38,
                Self::PY39,
                Self::PY310,
                Self::PY311,
                Self::PY312,
                Self::PY313,
            ]
        }

        fn to_possible_value(&self) -> Option<PossibleValue> {
            match (self.major, self.minor) {
                (3, 7) => Some(PossibleValue::new("3.7")),
                (3, 8) => Some(PossibleValue::new("3.8")),
                (3, 9) => Some(PossibleValue::new("3.9")),
                (3, 10) => Some(PossibleValue::new("3.10")),
                (3, 11) => Some(PossibleValue::new("3.11")),
                (3, 12) => Some(PossibleValue::new("3.12")),
                (3, 13) => Some(PossibleValue::new("3.13")),
                _ => None,
            }
        }
    }
}
