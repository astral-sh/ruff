//! Settings for import conventions.

use rustc_hash::{FxHashMap, FxHashSet};

use ruff_macros::CacheKey;

const CONVENTIONAL_ALIASES: &[(&str, &str)] = &[
    ("altair", "alt"),
    ("matplotlib", "mpl"),
    ("matplotlib.pyplot", "plt"),
    ("networkx", "nx"),
    ("numpy", "np"),
    ("pandas", "pd"),
    ("seaborn", "sns"),
    ("tensorflow", "tf"),
    ("tkinter", "tk"),
    ("holoviews", "hv"),
    ("panel", "pn"),
    ("plotly.express", "px"),
    ("polars", "pl"),
    ("pyarrow", "pa"),
];

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub aliases: FxHashMap<String, String>,
    pub banned_aliases: FxHashMap<String, Vec<String>>,
    pub banned_from: FxHashSet<String>,
}

pub fn default_aliases() -> FxHashMap<String, String> {
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
