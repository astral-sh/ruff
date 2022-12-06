//! Settings for import conventions.

use std::hash::{Hash, Hasher};

use once_cell::sync::Lazy;
use ruff_macros::ConfigurationOptions;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

static CONVENTIONAL_ALIASES: Lazy<FxHashMap<&'static str, &'static str>> = Lazy::new(|| {
    FxHashMap::from_iter([
        ("altair", "alt"),
        ("matplotlib.pyplot", "plt"),
        ("numpy", "np"),
        ("pandas", "pd"),
        ("seaborn", "sns"),
    ])
});

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    #[option(
        doc = "A mapping of modules to their conventional import aliases.",
        default = r#"{"altair": "alt", "matplotlib.pyplot": "plt", "numpy": "np", "pandas": "pd", "seaborn": "sns"}"#,
        value_type = "FxHashMap<String, String>",
        example = r#"
            # Declare a custom alias for the `matplotlib` module.
            [tool.ruff.flake8-import-conventions.aliases]
            "dask.dataframe" = "dd"
        "#
    )]
    pub aliases: Option<FxHashMap<String, String>>,
}

#[derive(Debug)]
pub struct Settings {
    pub aliases: FxHashMap<String, String>,
}

impl Hash for Settings {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.aliases.iter().for_each(|(k, v)| {
            k.hash(state);
            v.hash(state);
        });
    }
}

impl Settings {
    pub fn from_options(options: Options) -> Self {
        Self {
            aliases: options.aliases.unwrap_or_else(|| {
                CONVENTIONAL_ALIASES
                    .iter()
                    .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                    .collect()
            }),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            aliases: CONVENTIONAL_ALIASES
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        }
    }
}
