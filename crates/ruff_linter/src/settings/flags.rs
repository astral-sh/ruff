#[derive(Debug, Copy, Clone, Hash, is_macro::Is)]
pub enum FixMode {
    Generate(SuggestedFixes),
    Apply(SuggestedFixes),
    Diff(SuggestedFixes),
}

impl FixMode {
    pub fn suggested_fixes(&self) -> &SuggestedFixes {
        match self {
            FixMode::Generate(suggested) => suggested,
            FixMode::Apply(suggested) => suggested,
            FixMode::Diff(suggested) => suggested,
        }
    }
}

#[derive(Debug, Copy, Clone, Hash, is_macro::Is)]
pub enum SuggestedFixes {
    Apply,
    Disable,
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
