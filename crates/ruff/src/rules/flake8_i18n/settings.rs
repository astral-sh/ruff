use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, ConfigurationOptions};

const I18_FUNCTIONS_NAMES: &[&str] = &["_", "gettext", "ngettext"];

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8I18NOptions"
)]
pub struct Options {
    #[option(
        default = r#"["_", "gettext", "ngettext"]"#,
        value_type = "list[str]",
        example = r#"
            [tool.ruff.flake8-i18n]
            # Declare the functions names to be checked
            function-names = ["_", "gettext", "ngettext", "ugettetxt"]
        "#
    )]
    /// The function_names for to check. These can be extended by
    /// the `extend_function_names` option.
    pub function_names: Option<Vec<String>>,

    #[option(
        default = r#"["_", "gettext", "ngettext"]"#,
        value_type = "list[str]",
        example = r#"
            [tool.ruff.flake8-i18n]
            extend-function-names = ["ugettetxt"]
        "#
    )]
    /// This will be appended to the function_names
    /// (or to the default values if that field is missing)
    pub extend_function_names: Vec<String>,
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub functions_names: Vec<String>,
}

fn default_func_names() -> Vec<String> {
    I18_FUNCTIONS_NAMES
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
            functions_names: {
                let mut res = {
                    match options.function_names {
                        Some(v) => v,
                        None => default_func_names(),
                    }
                };
                res.extend(options.extend_function_names);
                res
            },
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
