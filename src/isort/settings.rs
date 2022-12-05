//! Settings for the `isort` plugin.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    pub combine_as_imports: Option<bool>,
    pub force_wrap_aliases: Option<bool>,
    pub known_first_party: Option<Vec<String>>,
    pub known_third_party: Option<Vec<String>>,
    pub extra_standard_library: Option<Vec<String>>,
}

#[derive(Debug, Hash, Default)]
pub struct Settings {
    pub combine_as_imports: bool,
    pub force_wrap_aliases: bool,
    pub known_first_party: BTreeSet<String>,
    pub known_third_party: BTreeSet<String>,
    pub extra_standard_library: BTreeSet<String>,
}

impl Settings {
    pub fn from_options(options: Options) -> Self {
        Self {
            combine_as_imports: options.combine_as_imports.unwrap_or_default(),
            force_wrap_aliases: options.force_wrap_aliases.unwrap_or_default(),
            known_first_party: BTreeSet::from_iter(options.known_first_party.unwrap_or_default()),
            known_third_party: BTreeSet::from_iter(options.known_third_party.unwrap_or_default()),
            extra_standard_library: BTreeSet::from_iter(
                options.extra_standard_library.unwrap_or_default(),
            ),
        }
    }
}
