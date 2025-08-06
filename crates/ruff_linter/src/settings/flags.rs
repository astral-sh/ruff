#[derive(Debug, Copy, Clone, Hash, is_macro::Is)]
pub enum FixMode {
    Generate,
    Apply,
    Diff,
}

#[derive(Debug, Copy, Clone, Hash)]
pub enum Noqa {
    Enabled,
    Disabled,
}

impl Noqa {
    pub const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

impl From<bool> for Noqa {
    fn from(value: bool) -> Self {
        if value { Self::Enabled } else { Self::Disabled }
    }
}

#[derive(Debug, Copy, Clone, Hash)]
pub enum Cache {
    Enabled,
    Disabled,
}

impl Cache {
    pub const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

impl From<bool> for Cache {
    fn from(value: bool) -> Self {
        if value { Self::Enabled } else { Self::Disabled }
    }
}
