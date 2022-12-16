/// Simple flags used to drive program behavior.
use crate::autofix::fixer;

#[derive(Debug, Copy, Clone)]
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

impl From<fixer::Mode> for Autofix {
    fn from(value: fixer::Mode) -> Self {
        match value {
            fixer::Mode::Generate | fixer::Mode::Apply => Autofix::Enabled,
            fixer::Mode::None => Autofix::Disabled,
        }
    }
}

#[derive(Debug, Copy, Clone)]
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

#[derive(Debug, Copy, Clone)]
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
