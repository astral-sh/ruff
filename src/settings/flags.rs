use crate::fix;

#[derive(Debug, Copy, Clone, Hash)]
pub enum Autofix {
    Enabled,
    Disabled,
}

impl From<bool> for Autofix {
    fn from(value: bool) -> Self {
        if value {
            Autofix::Enabled
        } else {
            Autofix::Disabled
        }
    }
}

impl From<fix::FixMode> for Autofix {
    fn from(value: fix::FixMode) -> Self {
        match value {
            fix::FixMode::Generate | fix::FixMode::Diff | fix::FixMode::Apply => Autofix::Enabled,
            fix::FixMode::None => Autofix::Disabled,
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
            Noqa::Enabled
        } else {
            Noqa::Disabled
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
            Cache::Enabled
        } else {
            Cache::Disabled
        }
    }
}
