//! Track live bindings per place, applicable constraints per binding, and live declarations.
//!
//! These data structures operate entirely on scope-local newtype-indices for definitions and
//! constraints, referring to their location in the `all_definitions` and `all_constraints`
//! indexvecs in [`super::UseDefMapBuilder`].
//!
//! We need to track arbitrary associations between bindings and constraints, not just a single set
//! of currently dominating constraints (where "dominating" means "control flow must have passed
//! through it to reach this point"), because we can have dominating constraints that apply to some
//! bindings but not others, as in this code:
//!
//! ```python
//! x = 1 if flag else None
//! if x is not None:
//!     if flag2:
//!         x = 2 if flag else None
//!     x
//! ```
//!
//! The `x is not None` constraint dominates the final use of `x`, but it applies only to the first
//! binding of `x`, not the second, so `None` is a possible value for `x`.
//!
//! And we can't just track, for each binding, an index into a list of dominating constraints,
//! either, because we can have bindings which are still visible, but subject to constraints that
//! are no longer dominating, as in this code:
//!
//! ```python
//! x = 0
//! if flag1:
//!     x = 1 if flag2 else None
//!     assert x is not None
//! x
//! ```
//!
//! From the point of view of the final use of `x`, the `x is not None` constraint no longer
//! dominates, but it does dominate the `x = 1 if flag2 else None` binding, so we have to keep
//! track of that.
//!
//! The data structures use `IndexVec` arenas to store all data compactly and contiguously, while
//! supporting very cheap clones.
//!
//! Tracking live declarations is simpler, since constraints are not involved, but otherwise very
//! similar to tracking live bindings.

use itertools::{EitherOrBoth, Itertools};
use ruff_index::newtype_index;
use smallvec::{SmallVec, smallvec};

use crate::semantic_index::narrowing_constraints::ScopedNarrowingConstraint;
use crate::semantic_index::reachability_constraints::{
    ReachabilityConstraintsBuilder, ScopedReachabilityConstraintId,
};

/// A newtype-index for a definition in a particular scope.
#[newtype_index]
#[derive(Ord, PartialOrd, salsa::Update, get_size2::GetSize)]
pub(crate) struct ScopedDefinitionId;

impl ScopedDefinitionId {
    /// A special ID that is used to describe an implicit start-of-scope state. When
    /// we see that this definition is live, we know that the place is (possibly)
    /// unbound or undeclared at a given usage site.
    /// When creating a use-def-map builder, we always add an empty `DefinitionState::Undefined` definition
    /// at index 0, so this ID is always present.
    pub(crate) const UNBOUND: ScopedDefinitionId = ScopedDefinitionId::from_u32(0);

    pub(crate) fn is_unbound(self) -> bool {
        self == Self::UNBOUND
    }
}

/// Live declarations for a single place at some point in control flow, with their
/// corresponding reachability constraints.
#[derive(Clone, Debug, Default, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(super) struct Declarations {
    /// A list of live declarations for this place, sorted by their `ScopedDefinitionId`
    live_declarations: SmallVec<[LiveDeclaration; 2]>,
}

/// One of the live declarations for a single place at some point in control flow.
#[derive(Clone, Debug, PartialEq, Eq, get_size2::GetSize)]
pub(super) struct LiveDeclaration {
    pub(super) declaration: ScopedDefinitionId,
    pub(super) reachability_constraint: ScopedReachabilityConstraintId,
}

pub(super) type LiveDeclarationsIterator<'a> = std::slice::Iter<'a, LiveDeclaration>;

#[derive(Clone, Copy, Debug)]
pub(in crate::semantic_index) enum PreviousDefinitions {
    AreShadowed,
    AreKept,
}

impl PreviousDefinitions {
    pub(super) fn are_shadowed(self) -> bool {
        matches!(self, PreviousDefinitions::AreShadowed)
    }
}

impl Declarations {
    pub(super) fn undeclared(reachability_constraint: ScopedReachabilityConstraintId) -> Self {
        let initial_declaration = LiveDeclaration {
            declaration: ScopedDefinitionId::UNBOUND,
            reachability_constraint,
        };
        Self {
            live_declarations: smallvec![initial_declaration],
        }
    }

    /// Record a newly-encountered declaration for this place.
    pub(super) fn record_declaration(
        &mut self,
        declaration: ScopedDefinitionId,
        reachability_constraint: ScopedReachabilityConstraintId,
        previous_definitions: PreviousDefinitions,
    ) {
        if previous_definitions.are_shadowed() {
            // The new declaration replaces all previous live declaration in this path.
            self.live_declarations.clear();
        }
        self.live_declarations.push(LiveDeclaration {
            declaration,
            reachability_constraint,
        });
    }

    /// Add given reachability constraint to all live declarations.
    pub(super) fn record_reachability_constraint(
        &mut self,
        reachability_constraints: &mut ReachabilityConstraintsBuilder,
        constraint: ScopedReachabilityConstraintId,
    ) {
        for declaration in &mut self.live_declarations {
            declaration.reachability_constraint = reachability_constraints
                .add_and_constraint(declaration.reachability_constraint, constraint);
        }
    }

    /// Return an iterator over live declarations for this place.
    pub(super) fn iter(&self) -> LiveDeclarationsIterator<'_> {
        self.live_declarations.iter()
    }

    fn merge(&mut self, b: Self, reachability_constraints: &mut ReachabilityConstraintsBuilder) {
        let a = std::mem::take(self);

        // Invariant: merge_join_by consumes the two iterators in sorted order, which ensures that
        // the merged `live_declarations` vec remains sorted. If a definition is found in both `a`
        // and `b`, we compose the constraints from the two paths in an appropriate way
        // (intersection for narrowing constraints; ternary OR for reachability constraints). If a
        // definition is found in only one path, it is used as-is.
        let a = a.live_declarations.into_iter();
        let b = b.live_declarations.into_iter();
        for zipped in a.merge_join_by(b, |a, b| a.declaration.cmp(&b.declaration)) {
            match zipped {
                EitherOrBoth::Both(a, b) => {
                    let reachability_constraint = reachability_constraints
                        .add_or_constraint(a.reachability_constraint, b.reachability_constraint);
                    self.live_declarations.push(LiveDeclaration {
                        declaration: a.declaration,
                        reachability_constraint,
                    });
                }

                EitherOrBoth::Left(declaration) | EitherOrBoth::Right(declaration) => {
                    self.live_declarations.push(declaration);
                }
            }
        }
    }

    pub(super) fn finish(&mut self, reachability_constraints: &mut ReachabilityConstraintsBuilder) {
        self.live_declarations.shrink_to_fit();
        for declaration in &self.live_declarations {
            reachability_constraints.mark_used(declaration.reachability_constraint);
        }
    }
}

/// A snapshot of a place state that can be used to resolve a reference in a nested scope.
/// If there are bindings in a (non-class) scope, they are stored in `Bindings`.
/// Even if it's a class scope (class variables are not visible to nested scopes) or there are no
/// bindings, the current narrowing constraint is necessary for narrowing, so it's stored in
/// `Constraint`.
#[derive(Clone, Debug, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(super) enum EnclosingSnapshot {
    Constraint(ScopedNarrowingConstraint),
    Bindings(Bindings),
}

impl EnclosingSnapshot {
    pub(super) fn finish(&mut self, reachability_constraints: &mut ReachabilityConstraintsBuilder) {
        match self {
            Self::Constraint(constraint) => {
                reachability_constraints.mark_used(*constraint);
            }
            Self::Bindings(bindings) => {
                bindings.finish(reachability_constraints);
            }
        }
    }
}

/// Live bindings for a single place at some point in control flow. Each live binding comes
/// with a set of narrowing constraints and a reachability constraint.
#[derive(Clone, Debug, Default, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(super) struct Bindings {
    /// The narrowing constraint applicable to the "unbound" binding, if we need access to it even
    /// when it's not visible. This happens in class scopes, where local name bindings are not visible
    /// to nested scopes, but we still need to know what narrowing constraints were applied to the
    /// "unbound" binding.
    unbound_narrowing_constraint: Option<ScopedNarrowingConstraint>,
    /// A list of live bindings for this place, sorted by their `ScopedDefinitionId`
    live_bindings: SmallVec<[LiveBinding; 2]>,
}

impl Bindings {
    pub(super) fn unbound_narrowing_constraint(&self) -> ScopedNarrowingConstraint {
        self.unbound_narrowing_constraint
            .unwrap_or(self.live_bindings[0].narrowing_constraint)
    }

    pub(super) fn finish(&mut self, reachability_constraints: &mut ReachabilityConstraintsBuilder) {
        self.live_bindings.shrink_to_fit();
        for binding in &self.live_bindings {
            reachability_constraints.mark_used(binding.reachability_constraint);
            reachability_constraints.mark_used(binding.narrowing_constraint);
        }
    }
}

/// One of the live bindings for a single place at some point in control flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(crate) struct LiveBinding {
    pub(crate) binding: ScopedDefinitionId,
    pub(crate) narrowing_constraint: ScopedNarrowingConstraint,
    pub(crate) reachability_constraint: ScopedReachabilityConstraintId,
}

pub(super) type LiveBindingsIterator<'a> = std::slice::Iter<'a, LiveBinding>;

impl Bindings {
    pub(super) fn unbound(reachability_constraint: ScopedReachabilityConstraintId) -> Self {
        let initial_binding = LiveBinding {
            binding: ScopedDefinitionId::UNBOUND,
            narrowing_constraint: ScopedNarrowingConstraint::ALWAYS_TRUE,
            reachability_constraint,
        };
        Self {
            unbound_narrowing_constraint: None,
            live_bindings: smallvec![initial_binding],
        }
    }

    /// Record a newly-encountered binding for this place.
    pub(super) fn record_binding(
        &mut self,
        binding: ScopedDefinitionId,
        reachability_constraint: ScopedReachabilityConstraintId,
        is_class_scope: bool,
        is_place_name: bool,
        previous_definitions: PreviousDefinitions,
    ) {
        // If we are in a class scope, and the unbound name binding was previously visible, but we will
        // now replace it, record the narrowing constraints on it:
        if is_class_scope && is_place_name && self.live_bindings[0].binding.is_unbound() {
            self.unbound_narrowing_constraint = Some(self.live_bindings[0].narrowing_constraint);
        }
        // The new binding replaces all previous live bindings in this path, and has no
        // constraints.
        if previous_definitions.are_shadowed() {
            self.live_bindings.clear();
        }
        self.live_bindings.push(LiveBinding {
            binding,
            narrowing_constraint: ScopedNarrowingConstraint::ALWAYS_TRUE,
            reachability_constraint,
        });
    }

    /// Add given constraint to all live bindings.
    pub(super) fn record_narrowing_constraint(
        &mut self,
        reachability_constraints: &mut ReachabilityConstraintsBuilder,
        constraint: ScopedNarrowingConstraint,
    ) {
        for binding in &mut self.live_bindings {
            binding.narrowing_constraint = reachability_constraints
                .add_and_constraint(binding.narrowing_constraint, constraint);
        }
    }

    /// Add given reachability constraint to all live bindings.
    pub(super) fn record_reachability_constraint(
        &mut self,
        reachability_constraints: &mut ReachabilityConstraintsBuilder,
        constraint: ScopedReachabilityConstraintId,
    ) {
        for binding in &mut self.live_bindings {
            binding.reachability_constraint = reachability_constraints
                .add_and_constraint(binding.reachability_constraint, constraint);
        }
    }

    /// Iterate over currently live bindings for this place
    pub(super) fn iter(&self) -> LiveBindingsIterator<'_> {
        self.live_bindings.iter()
    }

    pub(super) fn merge(
        &mut self,
        b: Self,
        reachability_constraints: &mut ReachabilityConstraintsBuilder,
    ) {
        let a = std::mem::take(self);

        if let Some((a, b)) = a
            .unbound_narrowing_constraint
            .zip(b.unbound_narrowing_constraint)
        {
            self.unbound_narrowing_constraint =
                Some(reachability_constraints.add_or_constraint(a, b));
        }

        // Invariant: merge_join_by consumes the two iterators in sorted order, which ensures that
        // the merged `live_bindings` vec remains sorted. If a definition is found in both `a` and
        // `b`, we compose the constraints from the two paths using ternary OR for both narrowing
        // and reachability constraints. If a definition is found in only one path, it is used
        // as-is.
        let a = a.live_bindings.into_iter();
        let b = b.live_bindings.into_iter();
        for zipped in a.merge_join_by(b, |a, b| a.binding.cmp(&b.binding)) {
            match zipped {
                EitherOrBoth::Both(a, b) => {
                    // If the same definition is visible through both paths, we OR the narrowing
                    // constraints: the type should be narrowed by whichever path was taken.
                    let narrowing_constraint = reachability_constraints
                        .add_or_constraint(a.narrowing_constraint, b.narrowing_constraint);

                    // For reachability constraints, we also merge using a ternary OR operation:
                    let reachability_constraint = reachability_constraints
                        .add_or_constraint(a.reachability_constraint, b.reachability_constraint);

                    self.live_bindings.push(LiveBinding {
                        binding: a.binding,
                        narrowing_constraint,
                        reachability_constraint,
                    });
                }

                EitherOrBoth::Left(binding) | EitherOrBoth::Right(binding) => {
                    self.live_bindings.push(binding);
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, get_size2::GetSize)]
pub(in crate::semantic_index) struct PlaceState {
    declarations: Declarations,
    bindings: Bindings,
}

impl PlaceState {
    /// Return a new [`PlaceState`] representing an unbound, undeclared place.
    pub(super) fn undefined(reachability: ScopedReachabilityConstraintId) -> Self {
        Self {
            declarations: Declarations::undeclared(reachability),
            bindings: Bindings::unbound(reachability),
        }
    }

    /// Record a newly-encountered binding for this place.
    pub(super) fn record_binding(
        &mut self,
        binding_id: ScopedDefinitionId,
        reachability_constraint: ScopedReachabilityConstraintId,
        is_class_scope: bool,
        is_place_name: bool,
        previous_definitions: PreviousDefinitions,
    ) {
        debug_assert_ne!(binding_id, ScopedDefinitionId::UNBOUND);
        self.bindings.record_binding(
            binding_id,
            reachability_constraint,
            is_class_scope,
            is_place_name,
            previous_definitions,
        );
    }

    /// Add given constraint to all live bindings.
    pub(super) fn record_narrowing_constraint(
        &mut self,
        reachability_constraints: &mut ReachabilityConstraintsBuilder,
        constraint: ScopedNarrowingConstraint,
    ) {
        self.bindings
            .record_narrowing_constraint(reachability_constraints, constraint);
    }

    /// Add given reachability constraint to all live bindings.
    pub(super) fn record_reachability_constraint(
        &mut self,
        reachability_constraints: &mut ReachabilityConstraintsBuilder,
        constraint: ScopedReachabilityConstraintId,
    ) {
        self.bindings
            .record_reachability_constraint(reachability_constraints, constraint);
        self.declarations
            .record_reachability_constraint(reachability_constraints, constraint);
    }

    /// Record a newly-encountered declaration of this place.
    pub(super) fn record_declaration(
        &mut self,
        declaration_id: ScopedDefinitionId,
        reachability_constraint: ScopedReachabilityConstraintId,
    ) {
        self.declarations.record_declaration(
            declaration_id,
            reachability_constraint,
            PreviousDefinitions::AreShadowed,
        );
    }

    /// Merge another [`PlaceState`] into this one.
    pub(super) fn merge(
        &mut self,
        b: PlaceState,
        reachability_constraints: &mut ReachabilityConstraintsBuilder,
    ) {
        self.bindings.merge(b.bindings, reachability_constraints);
        self.declarations
            .merge(b.declarations, reachability_constraints);
    }

    pub(super) fn bindings(&self) -> &Bindings {
        &self.bindings
    }

    pub(super) fn declarations(&self) -> &Declarations {
        &self.declarations
    }

    pub(super) fn finish(&mut self, reachability_constraints: &mut ReachabilityConstraintsBuilder) {
        self.declarations.finish(reachability_constraints);
        self.bindings.finish(reachability_constraints);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_index::Idx;

    use crate::semantic_index::predicate::ScopedPredicateId;

    #[track_caller]
    fn assert_bindings(place: &PlaceState, expected: &[(u32, ScopedNarrowingConstraint)]) {
        let actual: Vec<(u32, ScopedNarrowingConstraint)> = place
            .bindings()
            .iter()
            .map(|live_binding| {
                (
                    live_binding.binding.as_u32(),
                    live_binding.narrowing_constraint,
                )
            })
            .collect();
        assert_eq!(actual, expected);
    }

    #[track_caller]
    pub(crate) fn assert_declarations(place: &PlaceState, expected: &[&str]) {
        let actual = place
            .declarations()
            .iter()
            .map(
                |LiveDeclaration {
                     declaration,
                     reachability_constraint: _,
                 }| {
                    if *declaration == ScopedDefinitionId::UNBOUND {
                        "undeclared".into()
                    } else {
                        declaration.as_u32().to_string()
                    }
                },
            )
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    #[test]
    fn unbound() {
        let sym = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);

        assert_bindings(&sym, &[(0, ScopedNarrowingConstraint::ALWAYS_TRUE)]);
    }

    #[test]
    fn with() {
        let mut sym = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        sym.record_binding(
            ScopedDefinitionId::from_u32(1),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
            false,
            true,
            PreviousDefinitions::AreShadowed,
        );

        assert_bindings(&sym, &[(1, ScopedNarrowingConstraint::ALWAYS_TRUE)]);
    }

    #[test]
    fn record_constraint() {
        let mut reachability_constraints = ReachabilityConstraintsBuilder::default();
        let mut sym = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        sym.record_binding(
            ScopedDefinitionId::from_u32(1),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
            false,
            true,
            PreviousDefinitions::AreShadowed,
        );
        let atom = reachability_constraints.add_atom(ScopedPredicateId::new(0));
        sym.record_narrowing_constraint(&mut reachability_constraints, atom);

        assert_bindings(&sym, &[(1, atom)]);
    }

    #[test]
    fn merge() {
        let mut reachability_constraints = ReachabilityConstraintsBuilder::default();

        // merging the same definition with the same constraint keeps the constraint
        let mut sym1a = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        sym1a.record_binding(
            ScopedDefinitionId::from_u32(1),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
            false,
            true,
            PreviousDefinitions::AreShadowed,
        );
        let atom0 = reachability_constraints.add_atom(ScopedPredicateId::new(0));
        sym1a.record_narrowing_constraint(&mut reachability_constraints, atom0);

        let mut sym1b = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        sym1b.record_binding(
            ScopedDefinitionId::from_u32(1),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
            false,
            true,
            PreviousDefinitions::AreShadowed,
        );
        sym1b.record_narrowing_constraint(&mut reachability_constraints, atom0);

        sym1a.merge(sym1b, &mut reachability_constraints);
        let mut sym1 = sym1a;
        // Same constraint on both sides → OR(atom0, atom0) = atom0
        assert_bindings(&sym1, &[(1, atom0)]);

        // merging the same definition with differing constraints produces OR (not empty)
        let mut sym2a = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        sym2a.record_binding(
            ScopedDefinitionId::from_u32(2),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
            false,
            true,
            PreviousDefinitions::AreShadowed,
        );
        let atom1 = reachability_constraints.add_atom(ScopedPredicateId::new(1));
        sym2a.record_narrowing_constraint(&mut reachability_constraints, atom1);

        let mut sym1b = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        sym1b.record_binding(
            ScopedDefinitionId::from_u32(2),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
            false,
            true,
            PreviousDefinitions::AreShadowed,
        );
        let atom2 = reachability_constraints.add_atom(ScopedPredicateId::new(2));
        sym1b.record_narrowing_constraint(&mut reachability_constraints, atom2);

        sym2a.merge(sym1b, &mut reachability_constraints);
        let sym2 = sym2a;
        // Different constraints: OR(atom1, atom2) produces a new TDD node (not a terminal)
        let merged_constraint = sym2.bindings().iter().next().unwrap().narrowing_constraint;
        assert_ne!(merged_constraint, ScopedNarrowingConstraint::ALWAYS_TRUE);
        assert_ne!(merged_constraint, ScopedNarrowingConstraint::ALWAYS_FALSE);
        assert_ne!(merged_constraint, atom1);
        assert_ne!(merged_constraint, atom2);

        // merging a constrained definition with unbound keeps both
        let mut sym3a = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        sym3a.record_binding(
            ScopedDefinitionId::from_u32(3),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
            false,
            true,
            PreviousDefinitions::AreShadowed,
        );
        let atom3 = reachability_constraints.add_atom(ScopedPredicateId::new(3));
        sym3a.record_narrowing_constraint(&mut reachability_constraints, atom3);

        let sym2b = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);

        sym3a.merge(sym2b, &mut reachability_constraints);
        let sym3 = sym3a;
        let bindings: Vec<_> = sym3
            .bindings()
            .iter()
            .map(|b| (b.binding.as_u32(), b.narrowing_constraint))
            .collect();
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].0, 0); // unbound
        assert_eq!(bindings[1].0, 3);
        assert_eq!(bindings[1].1, atom3);

        // merging different definitions keeps them each with their existing constraints
        sym1.merge(sym3, &mut reachability_constraints);
        let sym = sym1;
        let bindings: Vec<_> = sym
            .bindings()
            .iter()
            .map(|b| (b.binding.as_u32(), b.narrowing_constraint))
            .collect();
        assert_eq!(bindings.len(), 3);
        assert_eq!(bindings[0].0, 0); // unbound
        assert_eq!(bindings[1].0, 1);
        assert_eq!(bindings[1].1, atom0);
        assert_eq!(bindings[2].0, 3);
        assert_eq!(bindings[2].1, atom3);
    }

    #[test]
    fn no_declaration() {
        let sym = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);

        assert_declarations(&sym, &["undeclared"]);
    }

    #[test]
    fn record_declaration() {
        let mut sym = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        sym.record_declaration(
            ScopedDefinitionId::from_u32(1),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
        );

        assert_declarations(&sym, &["1"]);
    }

    #[test]
    fn record_declaration_override() {
        let mut sym = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        sym.record_declaration(
            ScopedDefinitionId::from_u32(1),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
        );
        sym.record_declaration(
            ScopedDefinitionId::from_u32(2),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
        );

        assert_declarations(&sym, &["2"]);
    }

    #[test]
    fn record_declaration_merge() {
        let mut reachability_constraints = ReachabilityConstraintsBuilder::default();
        let mut sym = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        sym.record_declaration(
            ScopedDefinitionId::from_u32(1),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
        );

        let mut sym2 = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        sym2.record_declaration(
            ScopedDefinitionId::from_u32(2),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
        );

        sym.merge(sym2, &mut reachability_constraints);

        assert_declarations(&sym, &["1", "2"]);
    }

    #[test]
    fn record_declaration_merge_partial_undeclared() {
        let mut reachability_constraints = ReachabilityConstraintsBuilder::default();
        let mut sym = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        sym.record_declaration(
            ScopedDefinitionId::from_u32(1),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
        );

        let sym2 = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);

        sym.merge(sym2, &mut reachability_constraints);

        assert_declarations(&sym, &["undeclared", "1"]);
    }
}
