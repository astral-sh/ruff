use std::fmt::{Display, Formatter};

/// The target platform to assume when resolving types.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize, ruff_macros::RustDoc),
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
    use schemars::{json_schema, JsonSchema, SchemaGenerator, Schema};
    use std::borrow::Cow;

    impl JsonSchema for PythonPlatform {
        fn schema_name() -> Cow<'static, str> {
            "PythonPlatform".into()
        }

        fn json_schema(_gen: &mut SchemaGenerator) -> Schema {
            json_schema!({
                "description": "Do not make any assumptions about the target platform.\n\nAssume a specific target platform like `linux`, `darwin` or `win32`.\n\nWe use a string (instead of individual enum variants), as the set of possible platforms may change over time. See <https://docs.python.org/3/library/sys.html#sys.platform> for some known platform identifiers.",
                "anyOf": [
                    {
                        "type": "string"
                    },
                    {
                        "const": "all",
                        "description": "Do not make any assumptions about the target platform."
                    },
                    {
                        "const": "darwin",
                        "description": "Darwin"
                    },
                    {
                        "const": "linux",
                        "description": "Linux"
                    },
                    {
                        "const": "win32",
                        "description": "Windows"
                    }
                ]
            })
        }
    }
}
