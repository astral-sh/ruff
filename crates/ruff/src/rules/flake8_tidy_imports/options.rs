//! Settings for the `flake8-tidy-imports` plugin.

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use ruff_macros::{CombineOptions, ConfigurationOptions};

use super::settings::{ApiBan, Settings, Strictness};

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, CombineOptions,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8TidyImportsOptions"
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    #[option(
        default = r#""parents""#,
        value_type = r#""parents" | "all""#,
        example = r#"
            # Disallow all relative imports.
            ban-relative-imports = "all"
        "#
    )]
    /// Whether to ban all relative imports (`"all"`), or only those imports
    /// that extend into the parent module or beyond (`"parents"`).
    pub ban_relative_imports: Option<Strictness>,
    #[option(
        default = r#"{}"#,
        value_type = r#"dict[str, { "msg": str }]"#,
        example = r#"
            [tool.ruff.flake8-tidy-imports.banned-api]
            "cgi".msg = "The cgi module is deprecated, see https://peps.python.org/pep-0594/#cgi."
            "typing.TypedDict".msg = "Use typing_extensions.TypedDict instead."
        "#
    )]
    /// Specific modules or module members that may not be imported or accessed.
    /// Note that this rule is only meant to flag accidental uses,
    /// and can be circumvented via `eval` or `importlib`.
    pub banned_api: Option<FxHashMap<String, ApiBan>>,
    #[option(
        default = r#"[]"#,
        value_type = r#"list[str]"#,
        example = r#"
            # Ban certain modules from being imported at module level.
            # This does not ban these modules from being imported inline.
            banned-module-level-imports = ["torch", "tensorflow"]
        "#
    )]
    /// List of specific modules that can't be imported at module level. This does not ban these
    /// modules from being imported inline.
    pub banned_module_level_imports: Option<Vec<String>>,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            ban_relative_imports: options.ban_relative_imports.unwrap_or(Strictness::Parents),
            banned_api: options.banned_api.unwrap_or_default(),
            banned_module_level_imports: options.banned_module_level_imports.unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            ban_relative_imports: Some(settings.ban_relative_imports),
            banned_api: Some(settings.banned_api),
            banned_module_level_imports: Some(settings.banned_module_level_imports),
        }
    }
}
