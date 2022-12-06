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
        doc = "The conventional aliases for imports. These aliases can be extended by the \
               `extend_aliases` option.",
        default = r#"{"altair": "alt", "matplotlib.pyplot": "plt", "numpy": "np", "pandas": "pd", "seaborn": "sns"}"#,
        value_type = "FxHashMap<String, String>",
        example = r#"
            # Declare the default aliases
            [tool.ruff.flake8-import-conventions.aliases]
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
            [tool.ruff.flake8-import-conventions.extend-aliases]
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
        self.aliases.iter().for_each(|(k, v)| {
            k.hash(state);
            v.hash(state);
        });
    }
}

fn default_aliases() -> FxHashMap<String, String> {
    CONVENTIONAL_ALIASES
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect::<FxHashMap<_, _>>()
}

fn merge(defaults: &mut FxHashMap<String, String>, overrides: &FxHashMap<String, String>) {
    defaults.extend(
        overrides
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string())),
    );
}

fn resolve_aliases(options: Options) -> FxHashMap<String, String> {
    let mut aliases = match options.aliases {
        Some(options_aliases) => options_aliases,
        None => default_aliases(),
    };
    if let Some(extend_aliases) = &options.extend_aliases {
        merge(&mut aliases, extend_aliases);
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
