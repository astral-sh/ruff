//! The `Truthiness` enum represents the possible boolean evaluation of a value.

/// The possible boolean values of a value.
///
/// This is used for type narrowing and reachability analysis.
#[derive(Debug, Copy, Clone, PartialEq, Eq, get_size2::GetSize)]
pub enum Truthiness {
    /// For an object `x`, `bool(x)` will always return `True`
    AlwaysTrue,
    /// For an object `x`, `bool(x)` will always return `False`
    AlwaysFalse,
    /// For an object `x`, `bool(x)` could return either `True` or `False`
    Ambiguous,
}

impl Truthiness {
    pub const fn is_ambiguous(self) -> bool {
        matches!(self, Truthiness::Ambiguous)
    }

    pub const fn is_always_false(self) -> bool {
        matches!(self, Truthiness::AlwaysFalse)
    }

    pub const fn may_be_true(self) -> bool {
        !self.is_always_false()
    }

    pub const fn is_always_true(self) -> bool {
        matches!(self, Truthiness::AlwaysTrue)
    }

    #[must_use]
    pub const fn negate(self) -> Self {
        match self {
            Self::AlwaysTrue => Self::AlwaysFalse,
            Self::AlwaysFalse => Self::AlwaysTrue,
            Self::Ambiguous => Self::Ambiguous,
        }
    }

    #[must_use]
    pub const fn negate_if(self, condition: bool) -> Self {
        if condition { self.negate() } else { self }
    }

    #[must_use]
    pub fn or(self, other: Self) -> Self {
        match (self, other) {
            (Truthiness::AlwaysFalse, Truthiness::AlwaysFalse) => Truthiness::AlwaysFalse,
            (Truthiness::AlwaysTrue, _) | (_, Truthiness::AlwaysTrue) => Truthiness::AlwaysTrue,
            _ => Truthiness::Ambiguous,
        }
    }
}

impl From<bool> for Truthiness {
    fn from(value: bool) -> Self {
        if value {
            Truthiness::AlwaysTrue
        } else {
            Truthiness::AlwaysFalse
        }
    }
}
