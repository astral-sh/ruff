//! Settings for import conventions.

use std::fmt::{Display, Formatter};

use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use ruff_macros::CacheKey;

use crate::display_settings;
use crate::settings::types::PreviewMode;

const CONVENTIONAL_ALIASES: &[(&str, &str)] = &[
    ("altair", "alt"),
    ("matplotlib", "mpl"),
    ("matplotlib.pyplot", "plt"),
    ("networkx", "nx"),
    ("numpy", "np"),
    ("numpy.typing", "npt"),
    ("pandas", "pd"),
    ("plotly.express", "px"),
    ("seaborn", "sns"),
    ("tensorflow", "tf"),
    ("tkinter", "tk"),
    ("holoviews", "hv"),
    ("panel", "pn"),
    ("polars", "pl"),
    ("pyarrow", "pa"),
    ("xml.etree.ElementTree", "ET"),
];

const PREVIEW_ALIASES: &[(&str, &str)] =
    &[("plotly.graph_objects", "go"), ("statsmodels.api", "sm")];

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

pub fn default_aliases(preview: PreviewMode) -> FxHashMap<String, String> {
    let mut aliases = CONVENTIONAL_ALIASES
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect::<FxHashMap<_, _>>();

    if preview.is_enabled() {
        aliases.extend(preview_aliases());
    }
    aliases
}

pub fn default_banned_aliases(preview: PreviewMode) -> FxHashMap<String, BannedAliases> {
    let mut banned_aliases = FxHashMap::default();
    if preview.is_enabled() {
        banned_aliases.extend(preview_banned_aliases());
    }
    banned_aliases
}

pub fn preview_aliases() -> FxHashMap<String, String> {
    PREVIEW_ALIASES
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect::<FxHashMap<_, _>>()
}

pub fn preview_banned_aliases() -> FxHashMap<String, BannedAliases> {
    FxHashMap::from_iter([(
        "geopandas".to_string(),
        BannedAliases::from_iter(["gpd".to_string()]),
    )])
}

impl Settings {
    pub fn new(preview: PreviewMode) -> Self {
        Self {
            aliases: default_aliases(preview),
            banned_aliases: default_banned_aliases(preview),
            banned_from: FxHashSet::default(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            aliases: default_aliases(PreviewMode::Disabled),
            banned_aliases: default_banned_aliases(PreviewMode::Disabled),
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
