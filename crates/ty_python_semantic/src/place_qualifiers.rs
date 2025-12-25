//! Place qualifier enums that are used by both semantic and type analysis.
//!
//! These simple enums don't depend on type information and are used by
//! `use_def.rs` and other semantic index components.

/// Specifies how the boundness of a place should be determined.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update)]
pub enum BoundnessAnalysis {
    /// The place is always considered bound.
    AssumeBound,
    /// The boundness of the place is determined based on the visibility of the implicit
    /// `unbound` binding. In the example below, when analyzing the visibility of the
    /// `x = <unbound>` binding from the position of the end of the scope, it would be
    /// `Truthiness::Ambiguous`, because it could either be visible or not, depending on the
    /// `flag()` return value. This would result in a `Definedness::PossiblyUndefined` for `x`.
    ///
    /// ```py
    /// x = <unbound>
    ///
    /// if flag():
    ///     x = 1
    /// ```
    BasedOnUnboundVisibility,
}

/// Specifies whether a place is always defined or might be undefined.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub enum Definedness {
    /// The place is always defined at this point.
    AlwaysDefined,
    /// The place might or might not be defined at this point.
    PossiblyUndefined,
}

/// Specifies how a type was determined.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Default, get_size2::GetSize)]
pub enum TypeOrigin {
    /// The type was explicitly declared.
    Declared,
    /// The type was inferred from bindings.
    #[default]
    Inferred,
}

/// Specifies whether to widen the type with `Unknown`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Default, get_size2::GetSize)]
pub enum Widening {
    /// Don't widen the type.
    #[default]
    None,
    /// Widen the type with `Unknown` to handle undefined bindings.
    WithUnknown,
}

/// Specifies whether a re-export requires explicit `__all__` inclusion.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum RequiresExplicitReExport {
    Yes,
    No,
}

/// Specifies which definitions to consider when looking up a place.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Default)]
pub enum ConsideredDefinitions {
    /// Consider all reachable definitions at a specific use.
    AllReachable,
    /// Consider definitions visible at the end of the scope.
    #[default]
    EndOfScope,
}
