//! Settings for import conventions.

use std::hash::{Hash, Hasher};

use itertools::Itertools;
use ruff_macros::ConfigurationOptions;
use rustc_hash::FxHashMap;
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
        value_type = "FxHashMap<String, String>",
        example = r#"
            # Declare the default aliases.
            altair = "alt"
            matplotlib.pyplot = "plt"
            numpy = "np"
            pandas = "pd"
            seaborn = "sns"
        "#
    )]
    pub aliases: Option<FxHashMap<String, String>>,
    #[option(
        doc = "A mapping of modules to their conventional import aliases. These aliases will be \
               added to the `aliases` mapping.",
        default = r#"{}"#,
        value_type = "FxHashMap<String, String>",
        example = r#"
            # Declare a custom alias for the `matplotlib` module.
            "dask.dataframe" = "dd"
        "#
    )]
    pub extend_aliases: Option<FxHashMap<String, String>>,
}

#[derive(Debug)]
pub struct Settings {
    pub aliases: FxHashMap<String, String>,
}

impl Hash for Settings {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for value in self.aliases.iter().sorted() {
            value.hash(state);
        }
    }
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
