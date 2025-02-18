use std::fmt;

/// Representation of a Python version.
///
/// N.B. This does not necessarily represent a Python version that we actually support.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "cache", derive(ruff_macros::CacheKey))]
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
        .into_iter()
    }

    pub const fn latest() -> Self {
        Self::PY313
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
