use ruff_macros::CacheKey;
use std::fmt;

#[derive(Debug, Default, Clone, CacheKey)]
pub struct ImportMapSettings {}

impl fmt::Display for ImportMapSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Ok(())
    }
}
