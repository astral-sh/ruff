//! Settings for import conventions.

use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, CombineOptions, ConfigurationOptions};

const CONVENTIONAL_ALIASES: &[(&str, &str)] = &[
    ("altair", "alt"),
    ("matplotlib", "mpl"),
    ("matplotlib.pyplot", "plt"),
    ("numpy", "np"),
    ("pandas", "pd"),
    ("seaborn", "sns"),
    ("tensorflow", "tf"),
    ("holoviews", "hv"),
    ("panel", "pn"),
    ("plotly.express", "px"),
    ("polars", "pl"),
    ("pyarrow", "pa"),
];

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, CombineOptions,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8ImportConventionsOptions"
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    #[option(
        default = r#"{"altair": "alt", "matplotlib": "mpl", "matplotlib.pyplot": "plt", "numpy": "np", "pandas": "pd", "seaborn": "sns", "tensorflow": "tf", "holoviews": "hv", "panel": "pn", "plotly.express": "px", "polars": "pl", "pyarrow": "pa"}"#,
        value_type = "dict[str, str]",
        example = r#"
            [tool.ruff.flake8-import-conventions.aliases]
            # Declare the default aliases.
            altair = "alt"
            "matplotlib.pyplot" = "plt"
            numpy = "np"
            pandas = "pd"
            seaborn = "sns"
            scipy = "sp"
        "#
    )]
    /// The conventional aliases for imports. These aliases can be extended by
    /// the `extend_aliases` option.
    pub aliases: Option<FxHashMap<String, String>>,
    #[option(
        default = r#"{}"#,
        value_type = "dict[str, str]",
        example = r#"
            [tool.ruff.flake8-import-conventions.extend-aliases]
            # Declare a custom alias for the `matplotlib` module.
            "dask.dataframe" = "dd"
        "#
    )]
    /// A mapping from module to conventional import alias. These aliases will
    /// be added to the `aliases` mapping.
    pub extend_aliases: Option<FxHashMap<String, String>>,
    #[option(
        default = r#"{}"#,
        value_type = "dict[str, list[str]]",
        example = r#"
            [tool.ruff.flake8-import-conventions.banned-aliases]
            # Declare the banned aliases.
            "tensorflow.keras.backend" = ["K"]
    "#
    )]
    /// A mapping from module to its banned import aliases.
    pub banned_aliases: Option<FxHashMap<String, Vec<String>>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            # Declare the banned `from` imports.
            banned-from = ["typing"]
    "#
    )]
    /// A list of modules that are allowed to be imported from
    pub banned_from: Option<FxHashSet<String>>,
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub aliases: FxHashMap<String, String>,
    pub banned_aliases: FxHashMap<String, Vec<String>>,
    pub banned_from: FxHashSet<String>,
}

fn default_aliases() -> FxHashMap<String, String> {
    CONVENTIONAL_ALIASES
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect::<FxHashMap<_, _>>()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            aliases: default_aliases(),
            banned_aliases: FxHashMap::default(),
            banned_from: FxHashSet::default(),
        }
    }
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        let mut aliases = match options.aliases {
            Some(options_aliases) => options_aliases,
            None => default_aliases(),
        };
        if let Some(extend_aliases) = options.extend_aliases {
            aliases.extend(extend_aliases);
        }

        Self {
            aliases,
            banned_aliases: options.banned_aliases.unwrap_or_default(),
            banned_from: options.banned_from.unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            aliases: Some(settings.aliases),
            extend_aliases: None,
            banned_aliases: None,
            banned_from: None,
        }
    }
}
