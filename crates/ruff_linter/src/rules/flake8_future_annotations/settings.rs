use ruff_macros::CacheKey;

#[derive(CacheKey, Clone, Debug, Default)]
pub struct Settings {
    pub aggressive: bool,
}
