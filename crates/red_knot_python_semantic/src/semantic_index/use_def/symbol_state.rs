//! Track live bindings per symbol, applicable constraints per binding, and live declarations.
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
//! The data structures used here ([`BitSet`] and [`smallvec::SmallVec`]) optimize for keeping all
//! data inline (avoiding lots of scattered allocations) in small-to-medium cases, and falling back
//! to heap allocation to be able to scale to arbitrary numbers of live bindings and constraints
//! when needed.
//!
//! Tracking live declarations is simpler, since constraints are not involved, but otherwise very
//! similar to tracking live bindings.

use itertools::{EitherOrBoth, Itertools};
use ruff_index::newtype_index;
use smallvec::SmallVec;

use crate::semantic_index::use_def::bitset::{BitSet, BitSetIterator};
use crate::semantic_index::use_def::VisibilityConstraints;

/// A newtype-index for a definition in a particular scope.
#[newtype_index]
pub(super) struct ScopedDefinitionId;

impl ScopedDefinitionId {
    /// A special ID that is used to describe an implicit start-of-scope state. When
    /// we see that this definition is live, we know that the symbol is (possibly)
    /// unbound or undeclared at a given usage site.
    /// When creating a use-def-map builder, we always add an empty `None` definition
    /// at index 0, so this ID is always present.
    pub(super) const UNBOUND: ScopedDefinitionId = ScopedDefinitionId::from_u32(0);
}

/// A newtype-index for a constraint expression in a particular scope.
#[newtype_index]
pub(crate) struct ScopedConstraintId;

/// Can reference this * 64 total definitions inline; more will fall back to the heap.
const INLINE_BINDING_BLOCKS: usize = 3;

/// A [`BitSet`] of [`ScopedDefinitionId`], representing live bindings of a symbol in a scope.
type Bindings = BitSet<INLINE_BINDING_BLOCKS>;
type BindingsIterator<'a> = BitSetIterator<'a, INLINE_BINDING_BLOCKS>;

/// Can reference this * 64 total declarations inline; more will fall back to the heap.
const INLINE_DECLARATION_BLOCKS: usize = 3;

/// A [`BitSet`] of [`ScopedDefinitionId`], representing live declarations of a symbol in a scope.
type Declarations = BitSet<INLINE_DECLARATION_BLOCKS>;
type DeclarationsIterator<'a> = BitSetIterator<'a, INLINE_DECLARATION_BLOCKS>;

/// Can reference this * 64 total constraints inline; more will fall back to the heap.
const INLINE_CONSTRAINT_BLOCKS: usize = 2;

/// Can keep inline this many live bindings per symbol at a given time; more will go to heap.
const INLINE_BINDINGS_PER_SYMBOL: usize = 4;

/// Which constraints apply to a given binding?
type Constraints = BitSet<INLINE_CONSTRAINT_BLOCKS>;

type InlineConstraintArray = [Constraints; INLINE_BINDINGS_PER_SYMBOL];

/// One [`BitSet`] of applicable [`ScopedConstraintId`]s per live binding.
type ConstraintsPerBinding = SmallVec<InlineConstraintArray>;

/// Iterate over all constraints for a single binding.
type ConstraintsIterator<'a> = std::slice::Iter<'a, Constraints>;

/// A newtype-index for a visibility constraint in a particular scope.
#[newtype_index]
pub(crate) struct ScopedVisibilityConstraintId;

impl ScopedVisibilityConstraintId {
    /// A special ID that is used for an "always true" / "always visible" constraint.
    /// When we create a new [`VisibilityConstraints`] object, this constraint is always
    /// present at index 0.
    pub(crate) const ALWAYS_TRUE: ScopedVisibilityConstraintId =
        ScopedVisibilityConstraintId::from_u32(0);
}

const INLINE_VISIBILITY_CONSTRAINTS: usize = 4;
type InlineVisibilityConstraintsArray =
    [ScopedVisibilityConstraintId; INLINE_VISIBILITY_CONSTRAINTS];

/// One [`ScopedVisibilityConstraintId`] per live declaration.
type VisibilityConstraintPerDeclaration = SmallVec<InlineVisibilityConstraintsArray>;

/// One [`ScopedVisibilityConstraintId`] per live binding.
type VisibilityConstraintPerBinding = SmallVec<InlineVisibilityConstraintsArray>;

/// Iterator over the visibility constraints for all live bindings/declarations.
type VisibilityConstraintsIterator<'a> = std::slice::Iter<'a, ScopedVisibilityConstraintId>;

/// Live declarations for a single symbol at some point in control flow, with their
/// corresponding visibility constraints.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct SymbolDeclarations {
    /// [`BitSet`]: which declarations (as [`ScopedDefinitionId`]) can reach the current location?
    ///
    /// Invariant: Because this is a `BitSet`, it can be viewed as a _sorted_ set of definition
    /// IDs. The `visibility_constraints` field stores constraints for each definition. Therefore
    /// those fields must always have the same `len()` as `live_declarations`, and the elements
    /// must appear in the same order.  Effectively, this means that elements must always be added
    /// in sorted order, or via a binary search that determines the correct place to insert new
    /// constraints.
    pub(crate) live_declarations: Declarations,

    /// For each live declaration, which visibility constraint applies to it?
    pub(crate) visibility_constraints: VisibilityConstraintPerDeclaration,
}

impl SymbolDeclarations {
    fn undeclared(scope_start_visibility: ScopedVisibilityConstraintId) -> Self {
        Self {
            live_declarations: Declarations::with(0),
            visibility_constraints: VisibilityConstraintPerDeclaration::from_iter([
                scope_start_visibility,
            ]),
        }
    }

    /// Record a newly-encountered declaration for this symbol.
    fn record_declaration(&mut self, declaration_id: ScopedDefinitionId) {
        self.live_declarations = Declarations::with(declaration_id.into());

        self.visibility_constraints = VisibilityConstraintPerDeclaration::with_capacity(1);
        self.visibility_constraints
            .push(ScopedVisibilityConstraintId::ALWAYS_TRUE);
    }

    /// Add given visibility constraint to all live declarations.
    pub(super) fn record_visibility_constraint(
        &mut self,
        visibility_constraints: &mut VisibilityConstraints,
        constraint: ScopedVisibilityConstraintId,
    ) {
        for existing in &mut self.visibility_constraints {
            *existing = visibility_constraints.add_and_constraint(*existing, constraint);
        }
    }

    /// Return an iterator over live declarations for this symbol.
    pub(super) fn iter(&self) -> DeclarationIdIterator {
        DeclarationIdIterator {
            declarations: self.live_declarations.iter(),
            visibility_constraints: self.visibility_constraints.iter(),
        }
    }

    fn merge(&mut self, b: Self, visibility_constraints: &mut VisibilityConstraints) {
        let a = std::mem::take(self);
        self.live_declarations = a.live_declarations.clone();
        self.live_declarations.union(&b.live_declarations);

        // Invariant: These zips are well-formed since we maintain an invariant that all of our
        // fields are sets/vecs with the same length.
        let a = (a.live_declarations.iter()).zip(a.visibility_constraints);
        let b = (b.live_declarations.iter()).zip(b.visibility_constraints);

        // Invariant: merge_join_by consumes the two iterators in sorted order, which ensures that
        // the definition IDs and constraints line up correctly in the merged result. If a
        // definition is found in both `a` and `b`, we compose the constraints from the two paths
        // in an appropriate way (intersection for narrowing constraints; ternary OR for visibility
        // constraints). If a definition is found in only one path, it is used as-is.
        for zipped in a.merge_join_by(b, |(a_decl, _), (b_decl, _)| a_decl.cmp(b_decl)) {
            match zipped {
                EitherOrBoth::Both((_, a_vis_constraint), (_, b_vis_constraint)) => {
                    let vis_constraint = visibility_constraints
                        .add_or_constraint(a_vis_constraint, b_vis_constraint);
                    self.visibility_constraints.push(vis_constraint);
                }

                EitherOrBoth::Left((_, vis_constraint))
                | EitherOrBoth::Right((_, vis_constraint)) => {
                    self.visibility_constraints.push(vis_constraint);
                }
            }
        }
    }
}

/// Live bindings for a single symbol at some point in control flow. Each live binding comes
/// with a set of narrowing constraints and a visibility constraint.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct SymbolBindings {
    /// [`BitSet`]: which bindings (as [`ScopedDefinitionId`]) can reach the current location?
    ///
    /// Invariant: Because this is a `BitSet`, it can be viewed as a _sorted_ set of definition
    /// IDs. The `constraints` and `visibility_constraints` field stores constraints for each
    /// definition. Therefore those fields must always have the same `len()` as
    /// `live_bindings`, and the elements must appear in the same order.  Effectively, this means
    /// that elements must always be added in sorted order, or via a binary search that determines
    /// the correct place to insert new constraints.
    live_bindings: Bindings,

    /// For each live binding, which [`ScopedConstraintId`] apply?
    ///
    /// This is a [`smallvec::SmallVec`] which should always have one [`BitSet`] of constraints per
    /// binding in `live_bindings`.
    constraints: ConstraintsPerBinding,

    /// For each live binding, which visibility constraint applies to it?
    visibility_constraints: VisibilityConstraintPerBinding,
}

impl SymbolBindings {
    fn unbound(scope_start_visibility: ScopedVisibilityConstraintId) -> Self {
        Self {
            live_bindings: Bindings::with(ScopedDefinitionId::UNBOUND.as_u32()),
            constraints: ConstraintsPerBinding::from_iter([Constraints::default()]),
            visibility_constraints: VisibilityConstraintPerBinding::from_iter([
                scope_start_visibility,
            ]),
        }
    }

    /// Record a newly-encountered binding for this symbol.
    pub(super) fn record_binding(&mut self, binding_id: ScopedDefinitionId) {
        // The new binding replaces all previous live bindings in this path, and has no
        // constraints.
        self.live_bindings = Bindings::with(binding_id.into());
        self.constraints = ConstraintsPerBinding::with_capacity(1);
        self.constraints.push(Constraints::default());

        self.visibility_constraints = VisibilityConstraintPerBinding::with_capacity(1);
        self.visibility_constraints
            .push(ScopedVisibilityConstraintId::ALWAYS_TRUE);
    }

    /// Add given constraint to all live bindings.
    pub(super) fn record_constraint(&mut self, constraint_id: ScopedConstraintId) {
        for bitset in &mut self.constraints {
            bitset.insert(constraint_id.into());
        }
    }

    /// Add given visibility constraint to all live bindings.
    pub(super) fn record_visibility_constraint(
        &mut self,
        visibility_constraints: &mut VisibilityConstraints,
        constraint: ScopedVisibilityConstraintId,
    ) {
        for existing in &mut self.visibility_constraints {
            *existing = visibility_constraints.add_and_constraint(*existing, constraint);
        }
    }

    /// Iterate over currently live bindings for this symbol
    pub(super) fn iter(&self) -> BindingIdWithConstraintsIterator {
        BindingIdWithConstraintsIterator {
            definitions: self.live_bindings.iter(),
            constraints: self.constraints.iter(),
            visibility_constraints: self.visibility_constraints.iter(),
        }
    }

    fn merge(&mut self, mut b: Self, visibility_constraints: &mut VisibilityConstraints) {
        let mut a = std::mem::take(self);
        self.live_bindings = a.live_bindings.clone();
        self.live_bindings.union(&b.live_bindings);

        // Invariant: These zips are well-formed since we maintain an invariant that all of our
        // fields are sets/vecs with the same length.
        //
        // Performance: We iterate over the `constraints` smallvecs via mut reference, because the
        // individual elements are `BitSet`s (currently 24 bytes in size), and we don't want to
        // move them by value multiple times during iteration. By iterating by reference, we only
        // have to copy single pointers around.  In the loop below, the `std::mem::take` calls
        // specify precisely where we want to move them into the merged `constraints` smallvec.
        //
        // We don't need a similar optimization for `visibility_constraints`, since those elements
        // are 32-bit IndexVec IDs, and so are already cheap to move/copy.
        let a = (a.live_bindings.iter())
            .zip(a.constraints.iter_mut())
            .zip(a.visibility_constraints);
        let b = (b.live_bindings.iter())
            .zip(b.constraints.iter_mut())
            .zip(b.visibility_constraints);

        // Invariant: merge_join_by consumes the two iterators in sorted order, which ensures that
        // the definition IDs and constraints line up correctly in the merged result. If a
        // definition is found in both `a` and `b`, we compose the constraints from the two paths
        // in an appropriate way (intersection for narrowing constraints; ternary OR for visibility
        // constraints). If a definition is found in only one path, it is used as-is.
        for zipped in a.merge_join_by(b, |((a_def, _), _), ((b_def, _), _)| a_def.cmp(b_def)) {
            match zipped {
                EitherOrBoth::Both(
                    ((_, a_constraints), a_vis_constraint),
                    ((_, b_constraints), b_vis_constraint),
                ) => {
                    // If the same definition is visible through both paths, any constraint
                    // that applies on only one path is irrelevant to the resulting type from
                    // unioning the two paths, so we intersect the constraints.
                    let constraints = a_constraints;
                    constraints.intersect(b_constraints);
                    self.constraints.push(std::mem::take(constraints));

                    // For visibility constraints, we merge them using a ternary OR operation:
                    let vis_constraint = visibility_constraints
                        .add_or_constraint(a_vis_constraint, b_vis_constraint);
                    self.visibility_constraints.push(vis_constraint);
                }

                EitherOrBoth::Left(((_, constraints), vis_constraint))
                | EitherOrBoth::Right(((_, constraints), vis_constraint)) => {
                    self.constraints.push(std::mem::take(constraints));
                    self.visibility_constraints.push(vis_constraint);
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SymbolState {
    declarations: SymbolDeclarations,
    bindings: SymbolBindings,
}

impl SymbolState {
    /// Return a new [`SymbolState`] representing an unbound, undeclared symbol.
    pub(super) fn undefined(scope_start_visibility: ScopedVisibilityConstraintId) -> Self {
        Self {
            declarations: SymbolDeclarations::undeclared(scope_start_visibility),
            bindings: SymbolBindings::unbound(scope_start_visibility),
        }
    }

    /// Record a newly-encountered binding for this symbol.
    pub(super) fn record_binding(&mut self, binding_id: ScopedDefinitionId) {
        debug_assert_ne!(binding_id, ScopedDefinitionId::UNBOUND);
        self.bindings.record_binding(binding_id);
    }

    /// Add given constraint to all live bindings.
    pub(super) fn record_constraint(&mut self, constraint_id: ScopedConstraintId) {
        self.bindings.record_constraint(constraint_id);
    }

    /// Add given visibility constraint to all live bindings.
    pub(super) fn record_visibility_constraint(
        &mut self,
        visibility_constraints: &mut VisibilityConstraints,
        constraint: ScopedVisibilityConstraintId,
    ) {
        self.bindings
            .record_visibility_constraint(visibility_constraints, constraint);
        self.declarations
            .record_visibility_constraint(visibility_constraints, constraint);
    }

    pub(super) fn simplify_visibility_constraints(&mut self, snapshot_state: SymbolState) {
        if self.bindings.live_bindings == snapshot_state.bindings.live_bindings {
            self.bindings.visibility_constraints = snapshot_state.bindings.visibility_constraints;
        }
        if self.declarations.live_declarations == snapshot_state.declarations.live_declarations {
            self.declarations.visibility_constraints =
                snapshot_state.declarations.visibility_constraints;
        }
    }

    /// Record a newly-encountered declaration of this symbol.
    pub(super) fn record_declaration(&mut self, declaration_id: ScopedDefinitionId) {
        self.declarations.record_declaration(declaration_id);
    }

    /// Merge another [`SymbolState`] into this one.
    pub(super) fn merge(
        &mut self,
        b: SymbolState,
        visibility_constraints: &mut VisibilityConstraints,
    ) {
        self.bindings.merge(b.bindings, visibility_constraints);
        self.declarations
            .merge(b.declarations, visibility_constraints);
    }

    pub(super) fn bindings(&self) -> &SymbolBindings {
        &self.bindings
    }

    pub(super) fn declarations(&self) -> &SymbolDeclarations {
        &self.declarations
    }
}

/// A single binding (as [`ScopedDefinitionId`]) with an iterator of its applicable
/// narrowing constraints ([`ScopedConstraintId`]) and a corresponding visibility
/// visibility constraint ([`ScopedVisibilityConstraintId`]).
#[derive(Debug)]
pub(super) struct BindingIdWithConstraints<'map> {
    pub(super) definition: ScopedDefinitionId,
    pub(super) constraint_ids: ConstraintIdIterator<'map>,
    pub(super) visibility_constraint: ScopedVisibilityConstraintId,
}

#[derive(Debug)]
pub(super) struct BindingIdWithConstraintsIterator<'map> {
    definitions: BindingsIterator<'map>,
    constraints: ConstraintsIterator<'map>,
    visibility_constraints: VisibilityConstraintsIterator<'map>,
}

impl<'map> Iterator for BindingIdWithConstraintsIterator<'map> {
    type Item = BindingIdWithConstraints<'map>;

    fn next(&mut self) -> Option<Self::Item> {
        match (
            self.definitions.next(),
            self.constraints.next(),
            self.visibility_constraints.next(),
        ) {
            (None, None, None) => None,
            (Some(def), Some(constraints), Some(visibility_constraint_id)) => {
                Some(BindingIdWithConstraints {
                    definition: ScopedDefinitionId::from_u32(def),
                    constraint_ids: ConstraintIdIterator {
                        wrapped: constraints.iter(),
                    },
                    visibility_constraint: *visibility_constraint_id,
                })
            }
            // SAFETY: see above.
            _ => unreachable!("definitions and constraints length mismatch"),
        }
    }
}

impl std::iter::FusedIterator for BindingIdWithConstraintsIterator<'_> {}

#[derive(Debug)]
pub(super) struct ConstraintIdIterator<'a> {
    wrapped: BitSetIterator<'a, INLINE_CONSTRAINT_BLOCKS>,
}

impl Iterator for ConstraintIdIterator<'_> {
    type Item = ScopedConstraintId;

    fn next(&mut self) -> Option<Self::Item> {
        self.wrapped.next().map(ScopedConstraintId::from_u32)
    }
}

impl std::iter::FusedIterator for ConstraintIdIterator<'_> {}

/// A single declaration (as [`ScopedDefinitionId`]) with a corresponding visibility
/// visibility constraint ([`ScopedVisibilityConstraintId`]).
#[derive(Debug)]
pub(super) struct DeclarationIdWithConstraint {
    pub(super) definition: ScopedDefinitionId,
    pub(super) visibility_constraint: ScopedVisibilityConstraintId,
}

pub(super) struct DeclarationIdIterator<'map> {
    pub(crate) declarations: DeclarationsIterator<'map>,
    pub(crate) visibility_constraints: VisibilityConstraintsIterator<'map>,
}

impl Iterator for DeclarationIdIterator<'_> {
    type Item = DeclarationIdWithConstraint;

    fn next(&mut self) -> Option<Self::Item> {
        match (self.declarations.next(), self.visibility_constraints.next()) {
            (None, None) => None,
            (Some(declaration), Some(&visibility_constraint)) => {
                Some(DeclarationIdWithConstraint {
                    definition: ScopedDefinitionId::from_u32(declaration),
                    visibility_constraint,
                })
            }
            // SAFETY: see above.
            _ => unreachable!("declarations and visibility_constraints length mismatch"),
        }
    }
}

impl std::iter::FusedIterator for DeclarationIdIterator<'_> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn assert_bindings(symbol: &SymbolState, expected: &[&str]) {
        let actual = symbol
            .bindings()
            .iter()
            .map(|def_id_with_constraints| {
                let def_id = def_id_with_constraints.definition;
                let def = if def_id == ScopedDefinitionId::UNBOUND {
                    "unbound".into()
                } else {
                    def_id.as_u32().to_string()
                };
                let constraints = def_id_with_constraints
                    .constraint_ids
                    .map(ScopedConstraintId::as_u32)
                    .map(|idx| idx.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{def}<{constraints}>")
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    #[track_caller]
    pub(crate) fn assert_declarations(symbol: &SymbolState, expected: &[&str]) {
        let actual = symbol
            .declarations()
            .iter()
            .map(
                |DeclarationIdWithConstraint {
                     definition,
                     visibility_constraint: _,
                 }| {
                    if definition == ScopedDefinitionId::UNBOUND {
                        "undeclared".into()
                    } else {
                        definition.as_u32().to_string()
                    }
                },
            )
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    #[test]
    fn unbound() {
        let sym = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);

        assert_bindings(&sym, &["unbound<>"]);
    }

    #[test]
    fn with() {
        let mut sym = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym.record_binding(ScopedDefinitionId::from_u32(1));

        assert_bindings(&sym, &["1<>"]);
    }

    #[test]
    fn record_constraint() {
        let mut sym = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym.record_binding(ScopedDefinitionId::from_u32(1));
        sym.record_constraint(ScopedConstraintId::from_u32(0));

        assert_bindings(&sym, &["1<0>"]);
    }

    #[test]
    fn merge() {
        let mut visibility_constraints = VisibilityConstraints::default();

        // merging the same definition with the same constraint keeps the constraint
        let mut sym1a = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym1a.record_binding(ScopedDefinitionId::from_u32(1));
        sym1a.record_constraint(ScopedConstraintId::from_u32(0));

        let mut sym1b = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym1b.record_binding(ScopedDefinitionId::from_u32(1));
        sym1b.record_constraint(ScopedConstraintId::from_u32(0));

        sym1a.merge(sym1b, &mut visibility_constraints);
        let mut sym1 = sym1a;
        assert_bindings(&sym1, &["1<0>"]);

        // merging the same definition with differing constraints drops all constraints
        let mut sym2a = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym2a.record_binding(ScopedDefinitionId::from_u32(2));
        sym2a.record_constraint(ScopedConstraintId::from_u32(1));

        let mut sym1b = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym1b.record_binding(ScopedDefinitionId::from_u32(2));
        sym1b.record_constraint(ScopedConstraintId::from_u32(2));

        sym2a.merge(sym1b, &mut visibility_constraints);
        let sym2 = sym2a;
        assert_bindings(&sym2, &["2<>"]);

        // merging a constrained definition with unbound keeps both
        let mut sym3a = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym3a.record_binding(ScopedDefinitionId::from_u32(3));
        sym3a.record_constraint(ScopedConstraintId::from_u32(3));

        let sym2b = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);

        sym3a.merge(sym2b, &mut visibility_constraints);
        let sym3 = sym3a;
        assert_bindings(&sym3, &["unbound<>", "3<3>"]);

        // merging different definitions keeps them each with their existing constraints
        sym1.merge(sym3, &mut visibility_constraints);
        let sym = sym1;
        assert_bindings(&sym, &["unbound<>", "1<0>", "3<3>"]);
    }

    #[test]
    fn no_declaration() {
        let sym = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);

        assert_declarations(&sym, &["undeclared"]);
    }

    #[test]
    fn record_declaration() {
        let mut sym = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym.record_declaration(ScopedDefinitionId::from_u32(1));

        assert_declarations(&sym, &["1"]);
    }

    #[test]
    fn record_declaration_override() {
        let mut sym = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym.record_declaration(ScopedDefinitionId::from_u32(1));
        sym.record_declaration(ScopedDefinitionId::from_u32(2));

        assert_declarations(&sym, &["2"]);
    }

    #[test]
    fn record_declaration_merge() {
        let mut visibility_constraints = VisibilityConstraints::default();
        let mut sym = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym.record_declaration(ScopedDefinitionId::from_u32(1));

        let mut sym2 = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym2.record_declaration(ScopedDefinitionId::from_u32(2));

        sym.merge(sym2, &mut visibility_constraints);

        assert_declarations(&sym, &["1", "2"]);
    }

    #[test]
    fn record_declaration_merge_partial_undeclared() {
        let mut visibility_constraints = VisibilityConstraints::default();
        let mut sym = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym.record_declaration(ScopedDefinitionId::from_u32(1));

        let sym2 = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);

        sym.merge(sym2, &mut visibility_constraints);

        assert_declarations(&sym, &["undeclared", "1"]);
    }
}
