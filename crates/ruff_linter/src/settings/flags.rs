#[derive(Debug, Copy, Clone, Hash, is_macro::Is)]
pub enum FixMode {
    Generate(UnsafeFixes),
    Apply(UnsafeFixes),
    Diff(UnsafeFixes),
}

impl FixMode {
    pub fn suggested_fixes(&self) -> &UnsafeFixes {
        match self {
            FixMode::Generate(suggested) => suggested,
            FixMode::Apply(suggested) => suggested,
            FixMode::Diff(suggested) => suggested,
        }
    }
}

#[derive(Debug, Copy, Clone, Hash, is_macro::Is)]
pub enum UnsafeFixes {
    Enabled,
    Disabled,
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
