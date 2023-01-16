use self::banned_api::ApiBan;
use self::relative_imports::Strictness;
use crate::settings::hashable::HashableHashMap;

pub mod options;

pub mod banned_api;
pub mod relative_imports;

#[derive(Debug, Hash)]
pub struct Settings {
    pub ban_relative_imports: Strictness,
    pub banned_api: HashableHashMap<String, ApiBan>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ban_relative_imports: Strictness::Parents,
            banned_api: HashableHashMap::default(),
        }
    }
}
