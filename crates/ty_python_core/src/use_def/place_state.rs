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
//! Tracking live declarations is simpler, since narrowing constraints are not involved, but
//! otherwise very similar to tracking live bindings.
//!
//! We also store tagged entries for member and wildcard imports, whose source might be `Final`.
//! Semantic indexing cannot determine whether the imported value is actually `Final`, so type
//! inference checks these entries later. Imports are bindings, not type declarations, but an
//! inherited `Final` constrains later assignments even after an ordinary assignment replaces the
//! imported value binding. Reusing declaration flow preserves this metadata and gives it the same
//! reachability and branch-merging behavior without another flow channel. These entries neither
//! establish nor shadow a declared type, and the use-def map exposes them through separate
//! imported-`Final` queries.

use itertools::{EitherOrBoth, Itertools};
use ruff_index::newtype_index;
use smallvec::{SmallVec, smallvec};

use crate::ReachabilityConstraintsBuilder;
use crate::narrowing_constraints::{NarrowingConstraintsBuilder, ScopedNarrowingConstraint};
use crate::reachability_constraints::ScopedReachabilityConstraintId;

/// An index into a scope's use-def history. A combined definition can have separate declaration
/// and binding entries when they take effect at different points in control flow.
#[newtype_index]
#[derive(Ord, PartialOrd, get_size2::GetSize)]
pub struct ScopedDefinitionId;

impl ScopedDefinitionId {
    /// A special ID that is used to describe an implicit start-of-scope state. When
    /// we see that this definition is live, we know that the place is (possibly)
    /// unbound or undeclared at a given usage site.
    /// When creating a use-def-map builder, we always add an empty `DefinitionState::Undefined` definition
    /// at index 0, so this ID is always present.
    pub(super) const UNBOUND: ScopedDefinitionId = ScopedDefinitionId::from_u32(0);

    pub(crate) fn is_unbound(self) -> bool {
        self == Self::UNBOUND
    }
}

/// Live declarations for a single place at some point in control flow, with their
/// corresponding reachability constraints.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(super) struct Declarations {
    /// A list of live declarations for this place, sorted by their `ScopedDefinitionId`.
    live_declarations: SmallVec<[LiveDeclaration; 2]>,
}

/// One of the live declarations for a single place at some point in control flow.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(super) struct LiveDeclaration {
    declaration: PackedDeclarationId,
    pub(super) reachability_constraint: ScopedReachabilityConstraintId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
struct PackedDeclarationId(u32);

impl PackedDeclarationId {
    // Use the high bit to distinguish imported qualifiers without increasing the size of every
    // declaration. Scope-local IDs retain 31 bits.
    const IMPORTED_QUALIFIER: u32 = 1 << 31;
    const DEFINITION_MASK: u32 = !Self::IMPORTED_QUALIFIER;

    fn new(declaration: ScopedDefinitionId, is_imported_qualifier: bool) -> Self {
        let declaration = declaration.as_u32();
        assert_eq!(
            declaration & Self::IMPORTED_QUALIFIER,
            0,
            "scopes cannot contain more than 2^31 definitions"
        );

        let qualifier_bit = if is_imported_qualifier {
            Self::IMPORTED_QUALIFIER
        } else {
            0
        };

        Self(declaration | qualifier_bit)
    }

    const fn definition(self) -> ScopedDefinitionId {
        ScopedDefinitionId::from_u32(self.0 & Self::DEFINITION_MASK)
    }

    const fn is_imported_qualifier(self) -> bool {
        self.0 & Self::IMPORTED_QUALIFIER != 0
    }
}

impl LiveDeclaration {
    fn new(
        declaration: ScopedDefinitionId,
        reachability_constraint: ScopedReachabilityConstraintId,
        is_imported_qualifier: bool,
    ) -> Self {
        Self {
            declaration: PackedDeclarationId::new(declaration, is_imported_qualifier),
            reachability_constraint,
        }
    }

    pub(super) const fn declaration(&self) -> ScopedDefinitionId {
        self.declaration.definition()
    }

    pub(super) const fn is_imported_qualifier(&self) -> bool {
        self.declaration.is_imported_qualifier()
    }
}

static_assertions::assert_eq_size!(LiveDeclaration, [u32; 2]);

pub(super) type LiveDeclarationsIterator<'a> = std::slice::Iter<'a, LiveDeclaration>;

/// What happens to any preexisting definitions when a new binding of the same place is added.
/// `AreShadowed` is how normal assignments behave, but we model some features (loop headers,
/// `nonlocal` writes from nested scopes) as "synthetic" bindings that don't shadow other bindings.
#[derive(Clone, Copy, Debug)]
pub(crate) enum PreviousDefinitions {
    AreShadowed,
    AreKept,
}

/// What will happen to a definition if/when a when a new binding of the same place is added later.
/// `ShadowThisOne` is how normal assignments behave, and it's also how some "synthetic" bindings
/// behave (loop headers), but there are other synthetic bindings (nested `nonlocal` writes) that
/// cannot be shadowed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(crate) enum FutureDefinitions {
    ShadowThisOne,
    DontShadowThisOne,
}

impl PreviousDefinitions {
    fn are_shadowed(self) -> bool {
        matches!(self, PreviousDefinitions::AreShadowed)
    }
}

impl Declarations {
    pub(super) fn undeclared_reachability_constraint(
        &self,
    ) -> Option<ScopedReachabilityConstraintId> {
        let [declaration] = self.live_declarations.as_slice() else {
            return None;
        };

        (declaration.declaration() == ScopedDefinitionId::UNBOUND)
            .then_some(declaration.reachability_constraint)
    }

    pub(super) fn is_always_undeclared(&self) -> bool {
        self.undeclared_reachability_constraint()
            == Some(ScopedReachabilityConstraintId::ALWAYS_TRUE)
    }

    pub(super) fn undeclared(reachability_constraint: ScopedReachabilityConstraintId) -> Self {
        let initial_declaration =
            LiveDeclaration::new(ScopedDefinitionId::UNBOUND, reachability_constraint, false);
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
            // A real declaration replaces all earlier declarations, including imported qualifiers.
            self.live_declarations.clear();
        }
        self.live_declarations.push(LiveDeclaration::new(
            declaration,
            reachability_constraint,
            false,
        ));
    }

    /// Record an import that may contribute qualifiers without declaring a type.
    ///
    /// Imports replace earlier imported qualifiers, but keep real declarations and the undeclared
    /// sentinel so that an existing annotation or the absence of one remains visible.
    pub(super) fn record_imported_qualifier(
        &mut self,
        declaration: ScopedDefinitionId,
        reachability_constraint: ScopedReachabilityConstraintId,
        previous_definitions: PreviousDefinitions,
    ) {
        if previous_definitions.are_shadowed() {
            self.clear_imported_qualifiers();
        }

        self.live_declarations.push(LiveDeclaration::new(
            declaration,
            reachability_constraint,
            true,
        ));
    }

    fn clear_imported_qualifiers(&mut self) {
        self.live_declarations
            .retain(|declaration| !declaration.is_imported_qualifier());
    }

    /// Add given reachability constraint to all live declarations.
    fn record_reachability_constraint(
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

    pub(super) fn as_slice(&self) -> &[LiveDeclaration] {
        &self.live_declarations
    }

    fn merge(&mut self, b: Self, reachability_constraints: &mut ReachabilityConstraintsBuilder) {
        let a = std::mem::take(self);

        // Invariant: merge_join_by consumes the two iterators in sorted order, which ensures that
        // the merged `live_declarations` vec remains sorted. If a definition is found in both `a`
        // and `b`, we combine its reachability constraints. If a definition is found in only one
        // path, it is used as-is.
        let a = a.live_declarations.into_iter();
        let b = b.live_declarations.into_iter();
        for zipped in a.merge_join_by(b, |a, b| a.declaration().cmp(&b.declaration())) {
            match zipped {
                EitherOrBoth::Both(a, b) => {
                    let reachability_constraint = reachability_constraints
                        .add_or_constraint(a.reachability_constraint, b.reachability_constraint);
                    debug_assert_eq!(a.is_imported_qualifier(), b.is_imported_qualifier());
                    self.live_declarations.push(LiveDeclaration {
                        reachability_constraint,
                        ..a
                    });
                }

                EitherOrBoth::Left(declaration) | EitherOrBoth::Right(declaration) => {
                    self.live_declarations.push(declaration);
                }
            }
        }
    }
}

/// A snapshot of a place state that can be used to resolve a reference in a nested scope.
/// If there are bindings in a (non-class) scope, they are stored in `Bindings`.
/// Even if it's a class scope (class variables are not visible to nested scopes) or there are no
/// bindings, the current narrowing constraint is necessary for narrowing, so it's stored in
/// `Constraint`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(super) enum EnclosingSnapshot {
    Constraint(ScopedNarrowingConstraint),
    Bindings(Bindings),
}

/// Live bindings for a single place at some point in control flow. Each live binding comes
/// with a set of narrowing constraints and a reachability constraint.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, get_size2::GetSize)]
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
    pub(super) fn is_always_unbound(&self) -> bool {
        let [binding] = self.live_bindings.as_slice() else {
            return false;
        };
        self.unbound_narrowing_constraint.is_none()
            && binding.binding() == ScopedDefinitionId::UNBOUND
            && binding.narrowing_constraint == ScopedNarrowingConstraint::ALWAYS_TRUE
            && binding.reachability_constraint == ScopedReachabilityConstraintId::ALWAYS_TRUE
            && binding.can_be_shadowed() == FutureDefinitions::ShadowThisOne
    }

    pub(super) fn unbound_narrowing_constraint(&self) -> ScopedNarrowingConstraint {
        self.unbound_narrowing_constraint
            .unwrap_or(self.live_bindings[0].narrowing_constraint)
    }

    pub(super) fn finish(
        &mut self,
        narrowing_constraints: &mut NarrowingConstraintsBuilder,
        reachability_constraints: &mut ReachabilityConstraintsBuilder,
    ) {
        self.live_bindings.shrink_to_fit();
        for binding in &self.live_bindings {
            reachability_constraints.mark_used(binding.reachability_constraint);
            narrowing_constraints.mark_used(binding.narrowing_constraint);
        }
    }
}

/// One of the live bindings for a single place at some point in control flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
pub struct LiveBinding {
    binding: PackedDefinitionId,
    narrowing_constraint: ScopedNarrowingConstraint,
    reachability_constraint: ScopedReachabilityConstraintId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
struct PackedDefinitionId(u32);

impl PackedDefinitionId {
    // Scope-local definition IDs cannot practically use the high bit, so retain the shadowing
    // policy there instead of adding a byte plus padding to every `LiveBinding`.
    const DONT_SHADOW: u32 = 1 << 31;
    const DEFINITION_MASK: u32 = !Self::DONT_SHADOW;

    fn new(binding: ScopedDefinitionId, can_be_shadowed: FutureDefinitions) -> Self {
        let binding = binding.as_u32();
        assert_eq!(
            binding & Self::DONT_SHADOW,
            0,
            "scopes cannot contain more than 2^31 definitions"
        );
        Self(
            binding
                | match can_be_shadowed {
                    FutureDefinitions::ShadowThisOne => 0,
                    FutureDefinitions::DontShadowThisOne => Self::DONT_SHADOW,
                },
        )
    }

    const fn definition(self) -> ScopedDefinitionId {
        ScopedDefinitionId::from_u32(self.0 & Self::DEFINITION_MASK)
    }

    const fn can_be_shadowed(self) -> FutureDefinitions {
        if self.0 & Self::DONT_SHADOW == 0 {
            FutureDefinitions::ShadowThisOne
        } else {
            FutureDefinitions::DontShadowThisOne
        }
    }
}

impl LiveBinding {
    fn new(
        binding: ScopedDefinitionId,
        narrowing_constraint: ScopedNarrowingConstraint,
        reachability_constraint: ScopedReachabilityConstraintId,
        can_be_shadowed: FutureDefinitions,
    ) -> Self {
        Self {
            binding: PackedDefinitionId::new(binding, can_be_shadowed),
            narrowing_constraint,
            reachability_constraint,
        }
    }

    pub const fn binding(&self) -> ScopedDefinitionId {
        self.binding.definition()
    }

    pub const fn narrowing_constraint(&self) -> ScopedNarrowingConstraint {
        self.narrowing_constraint
    }

    pub const fn reachability_constraint(&self) -> ScopedReachabilityConstraintId {
        self.reachability_constraint
    }

    const fn can_be_shadowed(&self) -> FutureDefinitions {
        self.binding.can_be_shadowed()
    }
}

static_assertions::assert_eq_size!(LiveBinding, [u32; 3]);

pub(super) type LiveBindingsIterator<'a> = std::slice::Iter<'a, LiveBinding>;

impl Bindings {
    pub(super) fn unbound(reachability_constraint: ScopedReachabilityConstraintId) -> Self {
        let initial_binding = LiveBinding::new(
            ScopedDefinitionId::UNBOUND,
            ScopedNarrowingConstraint::ALWAYS_TRUE,
            reachability_constraint,
            FutureDefinitions::ShadowThisOne,
        );
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
        can_be_shadowed: FutureDefinitions,
    ) {
        // If we are in a class scope, and the unbound name binding was previously visible, but we will
        // now replace it, record the narrowing constraints on it:
        if is_class_scope
            && is_place_name
            && let Some(binding) = self.live_bindings.first()
            && binding.binding().is_unbound()
        {
            self.unbound_narrowing_constraint = Some(binding.narrowing_constraint);
        }
        // If the new binding is a shadowing type, it replaces previous live bindings in this path
        // (unless they're marked as not shadowable), and has no constraints.
        if previous_definitions.are_shadowed() {
            self.live_bindings
                .retain(|b| b.can_be_shadowed() == FutureDefinitions::DontShadowThisOne);
        }
        self.live_bindings.push(LiveBinding::new(
            binding,
            ScopedNarrowingConstraint::ALWAYS_TRUE,
            reachability_constraint,
            can_be_shadowed,
        ));
    }

    /// Add given constraint to all live bindings.
    fn record_narrowing_constraint(
        &mut self,
        narrowing_constraints: &mut NarrowingConstraintsBuilder,
        constraint: ScopedNarrowingConstraint,
    ) {
        for binding in &mut self.live_bindings {
            binding.narrowing_constraint =
                narrowing_constraints.add_and_constraint(binding.narrowing_constraint, constraint);
        }
    }

    /// Add given reachability constraint to all live bindings.
    fn record_reachability_constraint(
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

    pub(super) fn as_slice(&self) -> &[LiveBinding] {
        &self.live_bindings
    }

    pub(super) fn merge(
        &mut self,
        b: Self,
        narrowing_constraints: &mut NarrowingConstraintsBuilder,
        reachability_constraints: &mut ReachabilityConstraintsBuilder,
    ) {
        let a = std::mem::take(self);

        if let Some((a, b)) = a
            .unbound_narrowing_constraint
            .zip(b.unbound_narrowing_constraint)
        {
            self.unbound_narrowing_constraint = Some(narrowing_constraints.add_or_constraint(a, b));
        }

        // Invariant: merge_join_by consumes the two iterators in sorted order, which ensures that
        // the merged `live_bindings` vec remains sorted. If a definition is found in both `a` and
        // `b`, we combine its boolean narrowing constraints and its ternary reachability
        // constraints. If a definition is found in only one path, it is used as-is.
        let a = a.live_bindings.into_iter();
        let b = b.live_bindings.into_iter();
        for zipped in a.merge_join_by(b, |a, b| a.binding().cmp(&b.binding())) {
            match zipped {
                EitherOrBoth::Both(a, b) => {
                    // If the same definition is visible through both paths, we OR the narrowing
                    // constraints: the type should be narrowed by whichever path was taken.
                    let narrowing_constraint = narrowing_constraints
                        .add_or_constraint(a.narrowing_constraint, b.narrowing_constraint);

                    // For reachability constraints, we also merge using a ternary OR operation:
                    let reachability_constraint = reachability_constraints
                        .add_or_constraint(a.reachability_constraint, b.reachability_constraint);

                    debug_assert_eq!(a.can_be_shadowed(), b.can_be_shadowed());
                    self.live_bindings.push(LiveBinding::new(
                        a.binding(),
                        narrowing_constraint,
                        reachability_constraint,
                        a.can_be_shadowed(),
                    ));
                }

                EitherOrBoth::Left(binding) | EitherOrBoth::Right(binding) => {
                    self.live_bindings.push(binding);
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(crate) struct PlaceState {
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
        can_be_shadowed: FutureDefinitions,
    ) {
        debug_assert_ne!(binding_id, ScopedDefinitionId::UNBOUND);
        self.bindings.record_binding(
            binding_id,
            reachability_constraint,
            is_class_scope,
            is_place_name,
            previous_definitions,
            can_be_shadowed,
        );
    }

    /// Add given constraint to all live bindings.
    pub(super) fn record_narrowing_constraint(
        &mut self,
        narrowing_constraints: &mut NarrowingConstraintsBuilder,
        constraint: ScopedNarrowingConstraint,
    ) {
        self.bindings
            .record_narrowing_constraint(narrowing_constraints, constraint);
    }

    /// Add the given constraint to live bindings that were also present at an earlier use.
    pub(super) fn record_narrowing_constraint_for_bindings_at_use(
        &mut self,
        narrowing_constraints: &mut NarrowingConstraintsBuilder,
        constraint: ScopedNarrowingConstraint,
        bindings_at_use: &Bindings,
    ) {
        for binding in &mut self.bindings.live_bindings {
            if bindings_at_use
                .iter()
                .any(|binding_at_use| binding_at_use.binding() == binding.binding())
            {
                binding.narrowing_constraint = narrowing_constraints
                    .add_and_constraint(binding.narrowing_constraint, constraint);
            }
        }
    }

    /// Add the given constraint to live bindings selected by definition ID.
    pub(super) fn record_narrowing_constraint_for_bindings(
        &mut self,
        narrowing_constraints: &mut NarrowingConstraintsBuilder,
        constraint: ScopedNarrowingConstraint,
        bindings: &[ScopedDefinitionId],
    ) {
        for binding in &mut self.bindings.live_bindings {
            if bindings.contains(&binding.binding()) {
                binding.narrowing_constraint = narrowing_constraints
                    .add_and_constraint(binding.narrowing_constraint, constraint);
            }
        }
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

    /// Record an import that may contribute qualifiers independently of a declared type.
    pub(super) fn record_imported_qualifier(
        &mut self,
        declaration_id: ScopedDefinitionId,
        reachability_constraint: ScopedReachabilityConstraintId,
    ) {
        self.declarations.record_imported_qualifier(
            declaration_id,
            reachability_constraint,
            PreviousDefinitions::AreShadowed,
        );
    }

    pub(super) fn clear_imported_qualifiers(&mut self) {
        self.declarations.clear_imported_qualifiers();
    }

    /// Merge another [`PlaceState`] into this one.
    pub(super) fn merge(
        &mut self,
        b: PlaceState,
        narrowing_constraints: &mut NarrowingConstraintsBuilder,
        reachability_constraints: &mut ReachabilityConstraintsBuilder,
    ) {
        self.bindings
            .merge(b.bindings, narrowing_constraints, reachability_constraints);
        self.declarations
            .merge(b.declarations, reachability_constraints);
    }

    pub(super) fn bindings(&self) -> &Bindings {
        &self.bindings
    }

    pub(super) fn declarations(&self) -> &Declarations {
        &self.declarations
    }

    pub(super) fn into_parts(self) -> (Bindings, Declarations) {
        (self.bindings, self.declarations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_index::Idx;

    use crate::predicate::ScopedPredicateId;

    #[track_caller]
    fn assert_bindings(place: &PlaceState, expected: &[(u32, ScopedNarrowingConstraint)]) {
        let actual: Vec<(u32, ScopedNarrowingConstraint)> = place
            .bindings()
            .iter()
            .map(|live_binding| {
                (
                    live_binding.binding().as_u32(),
                    live_binding.narrowing_constraint,
                )
            })
            .collect();
        assert_eq!(actual, expected);
    }

    #[track_caller]
    fn assert_declarations(place: &PlaceState, expected: &[&str]) {
        let actual = place
            .declarations()
            .iter()
            .map(|live_declaration| {
                let declaration = live_declaration.declaration();
                if declaration == ScopedDefinitionId::UNBOUND {
                    "undeclared".into()
                } else if live_declaration.is_imported_qualifier() {
                    format!("{} (imported qualifier)", declaration.as_u32())
                } else {
                    declaration.as_u32().to_string()
                }
            })
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
            FutureDefinitions::ShadowThisOne,
        );

        assert_bindings(&sym, &[(1, ScopedNarrowingConstraint::ALWAYS_TRUE)]);
    }

    #[test]
    fn future_definitions_can_opt_out_of_shadowing() {
        let mut sym = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        sym.record_binding(
            ScopedDefinitionId::from_u32(1),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
            false,
            true,
            PreviousDefinitions::AreKept,
            FutureDefinitions::DontShadowThisOne,
        );
        sym.record_binding(
            ScopedDefinitionId::from_u32(2),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
            false,
            true,
            PreviousDefinitions::AreShadowed,
            FutureDefinitions::ShadowThisOne,
        );

        assert_bindings(
            &sym,
            &[
                (1, ScopedNarrowingConstraint::ALWAYS_TRUE),
                (2, ScopedNarrowingConstraint::ALWAYS_TRUE),
            ],
        );

        sym.record_binding(
            ScopedDefinitionId::from_u32(3),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
            false,
            true,
            PreviousDefinitions::AreShadowed,
            FutureDefinitions::ShadowThisOne,
        );

        assert_bindings(
            &sym,
            &[
                (1, ScopedNarrowingConstraint::ALWAYS_TRUE),
                (3, ScopedNarrowingConstraint::ALWAYS_TRUE),
            ],
        );
    }

    #[test]
    fn record_constraint() {
        let mut narrowing_constraints = NarrowingConstraintsBuilder::default();
        let mut sym = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        sym.record_binding(
            ScopedDefinitionId::from_u32(1),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
            false,
            true,
            PreviousDefinitions::AreShadowed,
            FutureDefinitions::ShadowThisOne,
        );
        let atom = narrowing_constraints.add_atom(ScopedPredicateId::new(0));
        sym.record_narrowing_constraint(&mut narrowing_constraints, atom);

        assert_bindings(&sym, &[(1, atom)]);
    }

    #[test]
    fn merge() {
        let mut narrowing_constraints = NarrowingConstraintsBuilder::default();
        let mut reachability_constraints = ReachabilityConstraintsBuilder::default();

        // merging the same definition with the same constraint keeps the constraint
        let mut sym1a = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        sym1a.record_binding(
            ScopedDefinitionId::from_u32(1),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
            false,
            true,
            PreviousDefinitions::AreShadowed,
            FutureDefinitions::ShadowThisOne,
        );
        let atom0 = narrowing_constraints.add_atom(ScopedPredicateId::new(0));
        sym1a.record_narrowing_constraint(&mut narrowing_constraints, atom0);

        let mut sym1b = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        sym1b.record_binding(
            ScopedDefinitionId::from_u32(1),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
            false,
            true,
            PreviousDefinitions::AreShadowed,
            FutureDefinitions::ShadowThisOne,
        );
        sym1b.record_narrowing_constraint(&mut narrowing_constraints, atom0);

        sym1a.merge(
            sym1b,
            &mut narrowing_constraints,
            &mut reachability_constraints,
        );
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
            FutureDefinitions::ShadowThisOne,
        );
        let atom1 = narrowing_constraints.add_atom(ScopedPredicateId::new(1));
        sym2a.record_narrowing_constraint(&mut narrowing_constraints, atom1);

        let mut sym1b = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        sym1b.record_binding(
            ScopedDefinitionId::from_u32(2),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
            false,
            true,
            PreviousDefinitions::AreShadowed,
            FutureDefinitions::ShadowThisOne,
        );
        let atom2 = narrowing_constraints.add_atom(ScopedPredicateId::new(2));
        sym1b.record_narrowing_constraint(&mut narrowing_constraints, atom2);

        sym2a.merge(
            sym1b,
            &mut narrowing_constraints,
            &mut reachability_constraints,
        );
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
            FutureDefinitions::ShadowThisOne,
        );
        let atom3 = narrowing_constraints.add_atom(ScopedPredicateId::new(3));
        sym3a.record_narrowing_constraint(&mut narrowing_constraints, atom3);

        let sym2b = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);

        sym3a.merge(
            sym2b,
            &mut narrowing_constraints,
            &mut reachability_constraints,
        );
        let sym3 = sym3a;
        let bindings: Vec<_> = sym3
            .bindings()
            .iter()
            .map(|b| (b.binding().as_u32(), b.narrowing_constraint))
            .collect();
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].0, 0); // unbound
        assert_eq!(bindings[1].0, 3);
        assert_eq!(bindings[1].1, atom3);

        // merging different definitions keeps them each with their existing constraints
        sym1.merge(
            sym3,
            &mut narrowing_constraints,
            &mut reachability_constraints,
        );
        let sym = sym1;
        let bindings: Vec<_> = sym
            .bindings()
            .iter()
            .map(|b| (b.binding().as_u32(), b.narrowing_constraint))
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
    fn imported_qualifier_preserves_existing_declaration() {
        let mut sym = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        sym.record_declaration(
            ScopedDefinitionId::from_u32(1),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
        );
        sym.record_imported_qualifier(
            ScopedDefinitionId::from_u32(2),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
        );

        assert_declarations(&sym, &["1", "2 (imported qualifier)"]);

        sym.record_imported_qualifier(
            ScopedDefinitionId::from_u32(3),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
        );

        assert_declarations(&sym, &["1", "3 (imported qualifier)"]);

        sym.clear_imported_qualifiers();

        assert_declarations(&sym, &["1"]);
    }

    #[test]
    fn imported_qualifier_merge_preserves_alternative_declaration() {
        let mut narrowing_constraints = NarrowingConstraintsBuilder::default();
        let mut reachability_constraints = ReachabilityConstraintsBuilder::default();
        let mut imported = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        imported.record_imported_qualifier(
            ScopedDefinitionId::from_u32(1),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
        );

        let mut declared = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        declared.record_declaration(
            ScopedDefinitionId::from_u32(2),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
        );

        imported.merge(
            declared,
            &mut narrowing_constraints,
            &mut reachability_constraints,
        );

        assert_declarations(&imported, &["undeclared", "1 (imported qualifier)", "2"]);
    }

    #[test]
    fn record_declaration_merge() {
        let mut narrowing_constraints = NarrowingConstraintsBuilder::default();
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

        sym.merge(
            sym2,
            &mut narrowing_constraints,
            &mut reachability_constraints,
        );

        assert_declarations(&sym, &["1", "2"]);
    }

    #[test]
    fn record_declaration_merge_partial_undeclared() {
        let mut narrowing_constraints = NarrowingConstraintsBuilder::default();
        let mut reachability_constraints = ReachabilityConstraintsBuilder::default();
        let mut sym = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);
        sym.record_declaration(
            ScopedDefinitionId::from_u32(1),
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
        );

        let sym2 = PlaceState::undefined(ScopedReachabilityConstraintId::ALWAYS_TRUE);

        sym.merge(
            sym2,
            &mut narrowing_constraints,
            &mut reachability_constraints,
        );

        assert_declarations(&sym, &["undeclared", "1"]);
    }
}
