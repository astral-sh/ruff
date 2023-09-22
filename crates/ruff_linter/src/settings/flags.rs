#[derive(Debug, Copy, Clone, Hash, is_macro::Is)]
pub enum FixMode {
    Generate,
    Apply,
    Diff,
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
