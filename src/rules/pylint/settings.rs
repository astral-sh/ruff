//! Settings for the `pylint` plugin.

use anyhow::anyhow;
use ruff_macros::ConfigurationOptions;
use rustpython_ast::Constant;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::hash::Hash;
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Hash, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub enum ConstantType {
    Bytes,
    Complex,
    Float,
    Int,
    Str,
    Tuple,
}

impl TryFrom<&Constant> for ConstantType {
    type Error = anyhow::Error;

    fn try_from(value: &Constant) -> Result<Self, Self::Error> {
        match value {
            Constant::Bytes(..) => Ok(Self::Bytes),
            Constant::Complex { .. } => Ok(Self::Complex),
            Constant::Float(..) => Ok(Self::Float),
            Constant::Int(..) => Ok(Self::Int),
            Constant::Str(..) => Ok(Self::Str),
            Constant::Tuple(..) => Ok(Self::Tuple),
            Constant::Bool(..) | Constant::Ellipsis | Constant::None => {
                Err(anyhow!("Singleton constants are unsupported"))
            }
        }
    }
}

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "PylintOptions"
)]
pub struct Options {
    #[option(
        default = r#"["str", "bytes"]"#,
        value_type = r#"list["str" | "bytes" | "complex" | "float" | "int" | "tuple"]"#,
        example = r#"
            allow-magic-value-types = ["int"]
        "#
    )]
    /// Constant types to ignore when used as "magic values" (see: `PLR2004`).
    pub allow_magic_value_types: Option<Vec<ConstantType>>,
    #[option(default = r"5", value_type = "int", example = r"max-args = 5")]
    /// Maximum number of arguments allowed for a function definition (see: `PLR0913`).
    pub max_args: Option<usize>,
    #[option(default = r"50", value_type = "int", example = r"max-statements = 50")]
    /// Maximum number of statements allowed for a method or a statement (see: `PLR0915`).
    pub max_statements: Option<usize>,
}

#[derive(Debug, Hash)]
pub struct Settings {
    pub allow_magic_value_types: Vec<ConstantType>,
    pub max_args: usize,
    pub max_statements: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            allow_magic_value_types: vec![ConstantType::Str, ConstantType::Bytes],
            max_args: 5,
            max_statements: 50,
        }
    }
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        let defaults = Settings::default();
        Self {
            allow_magic_value_types: options
                .allow_magic_value_types
                .unwrap_or(defaults.allow_magic_value_types),
            max_args: options.max_args.unwrap_or(defaults.max_args),
            max_statements: options.max_statements.unwrap_or(defaults.max_statements),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            allow_magic_value_types: Some(settings.allow_magic_value_types),
            max_args: Some(settings.max_args),
            max_statements: Some(settings.max_statements),
        }
    }
}
