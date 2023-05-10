#[derive(Debug, Copy, Clone, Hash)]
pub enum FixMode {
    Generate,
    Apply,
    Diff,
    None,
}

impl From<bool> for FixMode {
    fn from(value: bool) -> Self {
        if value {
            Self::Apply
        } else {
            Self::None
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
