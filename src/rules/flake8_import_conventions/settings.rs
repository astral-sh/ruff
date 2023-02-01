//! Settings for import conventions.

use std::hash::Hash;

use ruff_macros::ConfigurationOptions;
use rustc_hash::FxHashMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::settings::hashable::HashableHashMap;

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
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8ImportConventionsOptions"
)]
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
    /// A mapping of modules to their conventional import aliases. These aliases
    /// will be added to the `aliases` mapping.
    pub extend_aliases: Option<FxHashMap<String, String>>,
}

#[derive(Debug, Hash)]
pub struct Settings {
    pub aliases: HashableHashMap<String, String>,
}

fn default_aliases() -> FxHashMap<String, String> {
    CONVENTIONAL_ALIASES
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect::<FxHashMap<_, _>>()
}

fn resolve_aliases(options: Options) -> FxHashMap<String, String> {
    let mut aliases = match options.aliases {
        Some(options_aliases) => options_aliases,
        None => default_aliases(),
    };
    if let Some(extend_aliases) = options.extend_aliases {
        aliases.extend(extend_aliases);
    }
    aliases
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            aliases: default_aliases().into(),
        }
    }
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            aliases: resolve_aliases(options).into(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            aliases: Some(settings.aliases.into()),
            extend_aliases: None,
        }
    }
}
