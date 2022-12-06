//! Settings for import conventions.

use std::collections::BTreeMap;

use ruff_macros::ConfigurationOptions;
use serde::{Deserialize, Serialize};

const CONVENTIONAL_ALIASES: &[(&str, &str)] = &[
    ("altair", "alt"),
    ("matplotlib.pyplot", "plt"),
    ("numpy", "np"),
    ("pandas", "pd"),
    ("seaborn", "sns"),
];

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    #[option(
        doc = "The conventional aliases for imports. These aliases can be extended by the \
               `extend_aliases` option.",
        default = r#"{"altair": "alt", "matplotlib.pyplot": "plt", "numpy": "np", "pandas": "pd", "seaborn": "sns"}"#,
        value_type = "BTreeMap<String, String>",
        example = r#"
            # Declare the default aliases.
            altair = "alt"
            matplotlib.pyplot = "plt"
            numpy = "np"
            pandas = "pd"
            seaborn = "sns"
        "#
    )]
    pub aliases: Option<BTreeMap<String, String>>,
    #[option(
        doc = "A mapping of modules to their conventional import aliases. These aliases will be \
               added to the `aliases` mapping.",
        default = r#"{}"#,
        value_type = "BTreeMap<String, String>",
        example = r#"
            # Declare a custom alias for the `matplotlib` module.
            "dask.dataframe" = "dd"
        "#
    )]
    pub extend_aliases: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Hash)]
pub struct Settings {
    pub aliases: BTreeMap<String, String>,
}

fn default_aliases() -> BTreeMap<String, String> {
    CONVENTIONAL_ALIASES
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect::<BTreeMap<_, _>>()
}

fn resolve_aliases(options: Options) -> BTreeMap<String, String> {
    let mut aliases = match options.aliases {
        Some(options_aliases) => options_aliases,
        None => default_aliases(),
    };
    if let Some(extend_aliases) = options.extend_aliases {
        aliases.extend(extend_aliases);
    }
    aliases
}

impl Settings {
    pub fn from_options(options: Options) -> Self {
        Self {
            aliases: resolve_aliases(options),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            aliases: default_aliases(),
        }
    }
}
