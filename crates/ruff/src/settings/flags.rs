use crate::fix;
use ruff_macros::CacheKey;

#[derive(Debug, Copy, Clone, CacheKey, result_like::BoolLike)]
pub enum Autofix {
    Enabled,
    Disabled,
}

impl From<fix::FixMode> for Autofix {
    fn from(value: fix::FixMode) -> Self {
        match value {
            fix::FixMode::Generate | fix::FixMode::Diff | fix::FixMode::Apply => Self::Enabled,
            fix::FixMode::None => Self::Disabled,
        }
    }
}

#[derive(Debug, Copy, Clone, Hash, result_like::BoolLike)]
pub enum Noqa {
    Enabled,
    Disabled,
}

#[derive(Debug, Copy, Clone, Hash, result_like::BoolLike)]
pub enum Cache {
    Enabled,
    Disabled,
}
