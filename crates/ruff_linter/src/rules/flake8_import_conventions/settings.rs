//! Settings for import conventions.

use std::fmt::{Display, Formatter};

use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use ruff_macros::CacheKey;

use crate::display_settings;

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
    ("xml.etree.ElementTree", "ET"),
];

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize, CacheKey)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct BannedAliases(Vec<String>);

impl Display for BannedAliases {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "[")?;
        for (i, alias) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{alias}")?;
        }
        write!(f, "]")
    }
}

impl BannedAliases {
    /// Returns an iterator over the banned aliases.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }
}

impl FromIterator<String> for BannedAliases {
    fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub aliases: FxHashMap<String, String>,
    pub banned_aliases: FxHashMap<String, BannedAliases>,
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

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_import_conventions",
            fields = [
                self.aliases | map,
                self.banned_aliases | map,
                self.banned_from | set,
            ]
        }
        Ok(())
    }
}
