//! Settings for the `isort` plugin.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    pub known_first_party: Option<Vec<String>>,
    pub known_third_party: Option<Vec<String>>,
    pub extra_standard_library: Option<Vec<String>>,
}

#[derive(Debug, Hash)]
pub struct Settings {
    pub known_first_party: BTreeSet<String>,
    pub known_third_party: BTreeSet<String>,
    pub extra_standard_library: BTreeSet<String>,
}

impl Settings {
    pub fn from_options(options: Options) -> Self {
        Self {
            known_first_party: BTreeSet::from_iter(options.known_first_party.unwrap_or_default()),
            known_third_party: BTreeSet::from_iter(options.known_third_party.unwrap_or_default()),
            extra_standard_library: BTreeSet::from_iter(
                options.extra_standard_library.unwrap_or_default(),
            ),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            known_first_party: BTreeSet::new(),
            known_third_party: BTreeSet::new(),
            extra_standard_library: BTreeSet::new(),
        }
    }
}
