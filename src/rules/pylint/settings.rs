//! Settings for the `pylint` plugin.

use super::helpers::HashRegex;
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
        default = r#"["str"]"#,
        value_type = "Vec<ConstantType>",
        example = r#"
            allow-magic-value-types = ["int"]
        "#
    )]
    /// Constant types to ignore when used as "magic values".
    pub allow_magic_value_types: Option<Vec<ConstantType>>,
    #[option(default = r"5", value_type = "usize", example = r"max_args = 5")]
    /// Maximum number of arguments for function / method.
    pub max_args: Option<usize>,
    #[option(
        default = r"^_.*|^ignored_|^unused_",
        value_type = "String",
        example = r"ignored-argument-names = skip_.*"
    )]
    /// Argument names that match this expression will be ignored.
    pub ignored_argument_names: Option<String>,
}

#[derive(Debug, Hash)]
pub struct Settings {
    pub allow_magic_value_types: Vec<ConstantType>,
    pub max_args: usize,
    pub ignored_argument_names: HashRegex,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            allow_magic_value_types: vec![ConstantType::Str],
            max_args: 5,
            ignored_argument_names: r"^_.*|^ignored_|^unused_".try_into().unwrap(),
        }
    }
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        let settings_default = Settings::default();

        Self {
            allow_magic_value_types: options
                .allow_magic_value_types
                .unwrap_or(settings_default.allow_magic_value_types),
            max_args: options.max_args.unwrap_or(settings_default.max_args),
            ignored_argument_names: options.ignored_argument_names.map_or(
                settings_default.ignored_argument_names.clone(),
                |x| {
                    x.as_str()
                        .try_into()
                        .unwrap_or(settings_default.ignored_argument_names)
                },
            ),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            allow_magic_value_types: Some(settings.allow_magic_value_types),
            max_args: Some(settings.max_args),
            ignored_argument_names: Some(settings.ignored_argument_names.0.to_string()),
        }
    }
}
