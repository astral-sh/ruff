use std::fmt::{Display, Formatter};

/// The target platform to assume when resolving types.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "kebab-case")
)]
pub enum PythonPlatform {
    /// Do not make any assumptions about the target platform.
    All,

    /// Assume a specific target platform like `linux`, `darwin` or `win32`.
    ///
    /// We use a string (instead of individual enum variants), as the set of possible platforms
    /// may change over time. See <https://docs.python.org/3/library/sys.html#sys.platform> for
    /// some known platform identifiers.
    #[cfg_attr(feature = "serde", serde(untagged))]
    Identifier(String),
}

impl From<String> for PythonPlatform {
    fn from(platform: String) -> Self {
        match platform.as_str() {
            "all" => PythonPlatform::All,
            _ => PythonPlatform::Identifier(platform.to_string()),
        }
    }
}

impl Display for PythonPlatform {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PythonPlatform::All => f.write_str("all"),
            PythonPlatform::Identifier(name) => f.write_str(name),
        }
    }
}

impl Default for PythonPlatform {
    fn default() -> Self {
        if cfg!(target_os = "windows") {
            PythonPlatform::Identifier("win32".to_string())
        } else if cfg!(target_os = "macos") {
            PythonPlatform::Identifier("darwin".to_string())
        } else if cfg!(target_os = "android") {
            PythonPlatform::Identifier("android".to_string())
        } else if cfg!(target_os = "ios") {
            PythonPlatform::Identifier("ios".to_string())
        } else {
            PythonPlatform::Identifier("linux".to_string())
        }
    }
}

#[cfg(feature = "schemars")]
mod schema {
    use crate::PythonPlatform;
    use schemars::_serde_json::Value;
    use schemars::gen::SchemaGenerator;
    use schemars::schema::{Metadata, Schema, SchemaObject, SubschemaValidation};
    use schemars::JsonSchema;

    impl JsonSchema for PythonPlatform {
        fn schema_name() -> String {
            "PythonPlatform".to_string()
        }

        fn json_schema(_gen: &mut SchemaGenerator) -> Schema {
            Schema::Object(SchemaObject {
                // Hard code some well known values, but allow any other string as well.
                subschemas: Some(Box::new(SubschemaValidation {
                    any_of: Some(vec![
                        Schema::Object(SchemaObject {
                            instance_type: Some(schemars::schema::InstanceType::String.into()),
                            ..SchemaObject::default()
                        }),
                        // Promote well-known values for better auto-completion.
                        // Using `const` over `enumValues` as recommended [here](https://github.com/SchemaStore/schemastore/blob/master/CONTRIBUTING.md#documenting-enums).
                        Schema::Object(SchemaObject {
                            const_value: Some(Value::String("all".to_string())),
                            metadata: Some(Box::new(Metadata {
                                description: Some(
                                    "Do not make any assumptions about the target platform."
                                        .to_string(),
                                ),
                                ..Metadata::default()
                            })),

                            ..SchemaObject::default()
                        }),
                        Schema::Object(SchemaObject {
                            const_value: Some(Value::String("darwin".to_string())),
                            metadata: Some(Box::new(Metadata {
                                description: Some("Darwin".to_string()),
                                ..Metadata::default()
                            })),

                            ..SchemaObject::default()
                        }),
                        Schema::Object(SchemaObject {
                            const_value: Some(Value::String("linux".to_string())),
                            metadata: Some(Box::new(Metadata {
                                description: Some("Linux".to_string()),
                                ..Metadata::default()
                            })),

                            ..SchemaObject::default()
                        }),
                        Schema::Object(SchemaObject {
                            const_value: Some(Value::String("win32".to_string())),
                            metadata: Some(Box::new(Metadata {
                                description: Some("Windows".to_string()),
                                ..Metadata::default()
                            })),

                            ..SchemaObject::default()
                        }),
                    ]),

                    ..SubschemaValidation::default()
                })),

                ..SchemaObject::default()
            })
        }
    }
}
