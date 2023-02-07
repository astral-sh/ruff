use crate::fix;

#[derive(Debug, Copy, Clone, Hash)]
pub enum Autofix {
    Enabled,
    Disabled,
}

impl From<bool> for Autofix {
    fn from(value: bool) -> Self {
        if value {
            Self::Enabled
        } else {
            Self::Disabled
        }
    }
}

impl From<fix::FixMode> for Autofix {
    fn from(value: fix::FixMode) -> Self {
        match value {
            fix::FixMode::Generate | fix::FixMode::Diff | fix::FixMode::Apply => Self::Enabled,
            fix::FixMode::None => Self::Disabled,
        }
    }
}

#[derive(Debug, Copy, Clone, Hash)]
pub enum Noqa {
    Enabled,
    Disabled,
}

impl From<bool> for Noqa {
    fn from(value: bool) -> Self {
        if value {
            Self::Enabled
        } else {
            Self::Disabled
        }
    }
}

#[derive(Debug, Copy, Clone, Hash)]
pub enum Cache {
    Enabled,
    Disabled,
}

impl From<bool> for Cache {
    fn from(value: bool) -> Self {
        if value {
            Self::Enabled
        } else {
            Self::Disabled
        }
    }
}
