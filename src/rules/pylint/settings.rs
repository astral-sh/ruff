//! Settings for the `pylint` plugin.

use anyhow::anyhow;
use ruff_macros::ConfigurationOptions;
use rustpython_ast::Constant;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
        default = r#"["str"]"#,
        value_type = "Vec<ConstantType>",
        example = r#"
            allow-magic-value-types = ["int"]
        "#
    )]
    /// Constant types to ignore when used as "magic values".
    pub allow_magic_value_types: Option<Vec<ConstantType>>,
    #[option(
        default = r#"5"#,
        value_type = "usize",
        example = r#"
            allow-magic-value-types = 5
        "#
    )]
    /// Constant types to ignore when used as "magic values".
    pub max_args: Option<usize>,
}

#[derive(Debug, Hash)]
pub struct Settings {
    pub allow_magic_value_types: Vec<ConstantType>,
    pub max_args: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            allow_magic_value_types: vec![ConstantType::Str],
            max_args: 5,
        }
    }
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            allow_magic_value_types: options
                .allow_magic_value_types
                .unwrap_or_else(|| vec![ConstantType::Str]),
            max_args: options.max_args.unwrap_or_else(|| 5),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            allow_magic_value_types: Some(settings.allow_magic_value_types),
            max_args: Some(settings.max_args),
        }
    }
}
