use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, ConfigurationOptions};

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8GetTextOptions"
)]
pub struct Options {
    #[option(
        default = r#"["_", "gettext", "ngettext"]"#,
        value_type = "list[str]",
        example = r#"function-names = ["_", "gettext", "ngettext", "ugettetxt"]"#
    )]
    /// The function names to consider as internationalization calls.
    pub function_names: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"extend-function-names = ["ugettetxt"]"#
    )]
    #[serde(default)]
    /// Additional function names to consider as internationalization calls, in addition to those
    /// included in `function-names`.
    pub extend_function_names: Vec<String>,
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub functions_names: Vec<String>,
}

fn default_func_names() -> Vec<String> {
    ["_", "gettext", "ngettext"]
        .iter()
        .map(std::string::ToString::to_string)
        .collect()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            functions_names: default_func_names(),
        }
    }
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            functions_names: options
                .function_names
                .unwrap_or_else(default_func_names)
                .into_iter()
                .chain(options.extend_function_names)
                .collect(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            function_names: Some(settings.functions_names),
            extend_function_names: vec![],
        }
    }
}
