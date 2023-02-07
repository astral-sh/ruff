//! Settings for the `pylint` plugin.

use std::hash::Hash;

use anyhow::anyhow;
use ruff_macros::ConfigurationOptions;
use rustpython_parser::ast::Constant;
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
        default = r#"["str", "bytes"]"#,
        value_type = r#"list["str" | "bytes" | "complex" | "float" | "int" | "tuple"]"#,
        example = r#"
            allow-magic-value-types = ["int"]
        "#
    )]
    /// Constant types to ignore when used as "magic values" (see: `PLR2004`).
    pub allow_magic_value_types: Option<Vec<ConstantType>>,
    #[option(default = r"12", value_type = "int", example = r"max-branches = 12")]
    /// Maximum number of branches allowed for a function or method body (see:
    /// `PLR0912`).
    pub max_branches: Option<usize>,
    #[option(default = r"6", value_type = "int", example = r"max-returns = 6")]
    /// Maximum number of return statements allowed for a function or method
    /// body (see `PLR0911`)
    pub max_returns: Option<usize>,
    #[option(default = r"5", value_type = "int", example = r"max-args = 5")]
    /// Maximum number of arguments allowed for a function or method definition
    /// (see: `PLR0913`).
    pub max_args: Option<usize>,
    #[option(default = r"50", value_type = "int", example = r"max-statements = 50")]
    /// Maximum number of statements allowed for a function or method body (see:
    /// `PLR0915`).
    pub max_statements: Option<usize>,
}

#[derive(Debug, Hash)]
pub struct Settings {
    pub allow_magic_value_types: Vec<ConstantType>,
    pub max_args: usize,
    pub max_returns: usize,
    pub max_branches: usize,
    pub max_statements: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            allow_magic_value_types: vec![ConstantType::Str, ConstantType::Bytes],
            max_args: 5,
            max_returns: 6,
            max_branches: 12,
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
            max_returns: options.max_returns.unwrap_or(defaults.max_returns),
            max_branches: options.max_branches.unwrap_or(defaults.max_branches),
            max_statements: options.max_statements.unwrap_or(defaults.max_statements),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            allow_magic_value_types: Some(settings.allow_magic_value_types),
            max_args: Some(settings.max_args),
            max_returns: Some(settings.max_returns),
            max_branches: Some(settings.max_branches),
            max_statements: Some(settings.max_statements),
        }
    }
}
