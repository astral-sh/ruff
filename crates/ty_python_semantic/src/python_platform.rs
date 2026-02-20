use std::fmt::{Display, Formatter};
use ty_combine::Combine;

/// The target platform to assume when resolving types.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
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
            _ => PythonPlatform::Identifier(platform.clone()),
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

impl Combine for PythonPlatform {
    fn combine_with(&mut self, _other: Self) {}
}

#[cfg(feature = "schemars")]
mod schema {
    use crate::PythonPlatform;
    use ruff_db::RustDoc;
    use schemars::{JsonSchema, Schema, SchemaGenerator};
    use serde_json::Value;

    impl JsonSchema for PythonPlatform {
        fn schema_name() -> std::borrow::Cow<'static, str> {
            std::borrow::Cow::Borrowed("PythonPlatform")
        }

        fn json_schema(_gen: &mut SchemaGenerator) -> Schema {
            fn constant(value: &str, description: &str) -> Value {
                let mut schema = schemars::json_schema!({ "const": value });
                schema.ensure_object().insert(
                    "description".to_string(),
                    Value::String(description.to_string()),
                );
                schema.into()
            }

            // Hard code some well known values, but allow any other string as well.
            let mut any_of = vec![schemars::json_schema!({ "type": "string" }).into()];
            // Promote well-known values for better auto-completion.
            // Using `const` over `enumValues` as recommended [here](https://github.com/SchemaStore/schemastore/blob/master/CONTRIBUTING.md#documenting-enums).
            any_of.push(constant(
                "all",
                "Do not make any assumptions about the target platform.",
            ));
            any_of.push(constant("darwin", "Darwin"));
            any_of.push(constant("linux", "Linux"));
            any_of.push(constant("win32", "Windows"));

            let mut schema = Schema::default();
            let object = schema.ensure_object();
            object.insert("anyOf".to_string(), Value::Array(any_of));
            object.insert(
                "description".to_string(),
                Value::String(<PythonPlatform as RustDoc>::rust_doc().to_string()),
            );

            schema
        }
    }
}
