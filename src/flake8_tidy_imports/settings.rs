//! Settings for the `flake8-tidy-imports` plugin.

use std::hash::{Hash, Hasher};

use itertools::Itertools;
use ruff_macros::ConfigurationOptions;
use rustc_hash::FxHashMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub enum Strictness {
    /// Ban imports that extend into the parent module or beyond.
    Parents,
    /// Ban all relative imports.
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct BannedApi {
    pub msg: String,
    // we may add a `fix_to: String` here in the future to support --fix
}

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8TidyImportsOptions"
)]
pub struct Options {
    #[option(
        default = r#""parents""#,
        value_type = "Strictness",
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
        value_type = "HashMap<String, BannedApi>",
        example = r#"
            [tool.ruff.flake8-tidy-imports.banned-api]
            "cgi".msg = "The cgi module is deprecated, see https://peps.python.org/pep-0594/#cgi."
            "typing.TypedDict".msg = "Use typing_extensions.TypedDict instead."
        "#
    )]
    /// Specific modules or module members that may not be imported/accessed.
    ///
    /// Note that this check is only meant to flag accidental uses,
    /// it can be easily circumvented via `eval` or `importlib` and
    /// attempting to ban those via this setting is a futile endeavor.
    pub banned_api: FxHashMap<String, BannedApi>,
}

#[derive(Debug)]
pub struct Settings {
    pub ban_relative_imports: Strictness,
    pub banned_api: FxHashMap<String, BannedApi>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ban_relative_imports: Strictness::Parents,
        }
    }
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            ban_relative_imports: options.ban_relative_imports.unwrap_or(Strictness::Parents),
            banned_api: options.banned_api,
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            ban_relative_imports: Some(settings.ban_relative_imports),
            banned_api: settings.banned_api,
        }
    }
}

impl Hash for Settings {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ban_relative_imports.hash(state);
        for (key, value) in self.banned_api.iter().sorted() {
            key.hash(state);
            value.hash(state);
        }
    }
}
