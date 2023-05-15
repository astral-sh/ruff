use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, CombineOptions, ConfigurationOptions};

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, CombineOptions,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8GetTextOptions"
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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
    /// Additional function names to consider as internationalization calls, in addition to those
    /// included in `function-names`.
    pub extend_function_names: Option<Vec<String>>,
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub functions_names: Vec<String>,
}

fn default_func_names() -> Vec<String> {
    vec![
        "_".to_string(),
        "gettext".to_string(),
        "ngettext".to_string(),
    ]
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
                .chain(
                    options
                        .extend_function_names
                        .unwrap_or_default()
                        .into_iter(),
                )
                .collect(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            function_names: Some(settings.functions_names),
            extend_function_names: Some(Vec::new()),
        }
    }
}
