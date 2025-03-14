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
//! The data structures use `IndexVec` arenas to store all data compactly and contiguously, while
//! supporting very cheap clones.
//!
//! Tracking live declarations is simpler, since constraints are not involved, but otherwise very
//! similar to tracking live bindings.

use itertools::{EitherOrBoth, Itertools};
use ruff_index::newtype_index;
use smallvec::{smallvec, SmallVec};

use crate::semantic_index::narrowing_constraints::{
    NarrowingConstraintsBuilder, ScopedNarrowingConstraint, ScopedNarrowingConstraintPredicate,
};
use crate::semantic_index::visibility_constraints::{
    ScopedVisibilityConstraintId, VisibilityConstraintsBuilder,
};

/// A newtype-index for a definition in a particular scope.
#[newtype_index]
#[derive(Ord, PartialOrd)]
pub(super) struct ScopedDefinitionId;

impl ScopedDefinitionId {
    /// A special ID that is used to describe an implicit start-of-scope state. When
    /// we see that this definition is live, we know that the symbol is (possibly)
    /// unbound or undeclared at a given usage site.
    /// When creating a use-def-map builder, we always add an empty `None` definition
    /// at index 0, so this ID is always present.
    pub(super) const UNBOUND: ScopedDefinitionId = ScopedDefinitionId::from_u32(0);
}

/// Can keep inline this many live bindings or declarations per symbol at a given time; more will
/// go to heap.
const INLINE_DEFINITIONS_PER_SYMBOL: usize = 4;

/// Live declarations for a single symbol at some point in control flow, with their
/// corresponding visibility constraints.
#[derive(Clone, Debug, Default, PartialEq, Eq, salsa::Update)]
pub(super) struct SymbolDeclarations {
    /// A list of live declarations for this symbol, sorted by their `ScopedDefinitionId`
    live_declarations: SmallVec<[LiveDeclaration; INLINE_DEFINITIONS_PER_SYMBOL]>,
}

/// One of the live declarations for a single symbol at some point in control flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LiveDeclaration {
    pub(super) declaration: ScopedDefinitionId,
    pub(super) visibility_constraint: ScopedVisibilityConstraintId,
}

pub(super) type LiveDeclarationsIterator<'a> = std::slice::Iter<'a, LiveDeclaration>;

impl SymbolDeclarations {
    fn undeclared(scope_start_visibility: ScopedVisibilityConstraintId) -> Self {
        let initial_declaration = LiveDeclaration {
            declaration: ScopedDefinitionId::UNBOUND,
            visibility_constraint: scope_start_visibility,
        };
        Self {
            live_declarations: smallvec![initial_declaration],
        }
    }

    /// Record a newly-encountered declaration for this symbol.
    fn record_declaration(&mut self, declaration: ScopedDefinitionId) {
        // The new declaration replaces all previous live declaration in this path.
        self.live_declarations.clear();
        self.live_declarations.push(LiveDeclaration {
            declaration,
            visibility_constraint: ScopedVisibilityConstraintId::ALWAYS_TRUE,
        });
    }

    /// Add given visibility constraint to all live declarations.
    pub(super) fn record_visibility_constraint(
        &mut self,
        visibility_constraints: &mut VisibilityConstraintsBuilder,
        constraint: ScopedVisibilityConstraintId,
    ) {
        for declaration in &mut self.live_declarations {
            declaration.visibility_constraint = visibility_constraints
                .add_and_constraint(declaration.visibility_constraint, constraint);
        }
    }

    /// Return an iterator over live declarations for this symbol.
    pub(super) fn iter(&self) -> LiveDeclarationsIterator<'_> {
        self.live_declarations.iter()
    }

    /// Iterate over the IDs of each currently live declaration for this symbol
    fn iter_declarations(&self) -> impl Iterator<Item = ScopedDefinitionId> + '_ {
        self.iter().map(|lb| lb.declaration)
    }

    fn simplify_visibility_constraints(&mut self, other: SymbolDeclarations) {
        // If the set of live declarations hasn't changed, don't simplify.
        if self.live_declarations.len() != other.live_declarations.len()
            || !self.iter_declarations().eq(other.iter_declarations())
        {
            return;
        }

        for (declaration, other_declaration) in self
            .live_declarations
            .iter_mut()
            .zip(other.live_declarations)
        {
            declaration.visibility_constraint = other_declaration.visibility_constraint;
        }
    }

    fn merge(&mut self, b: Self, visibility_constraints: &mut VisibilityConstraintsBuilder) {
        let a = std::mem::take(self);

        // Invariant: merge_join_by consumes the two iterators in sorted order, which ensures that
        // the merged `live_declarations` vec remains sorted. If a definition is found in both `a`
        // and `b`, we compose the constraints from the two paths in an appropriate way
        // (intersection for narrowing constraints; ternary OR for visibility constraints). If a
        // definition is found in only one path, it is used as-is.
        let a = a.live_declarations.into_iter();
        let b = b.live_declarations.into_iter();
        for zipped in a.merge_join_by(b, |a, b| a.declaration.cmp(&b.declaration)) {
            match zipped {
                EitherOrBoth::Both(a, b) => {
                    let visibility_constraint = visibility_constraints
                        .add_or_constraint(a.visibility_constraint, b.visibility_constraint);
                    self.live_declarations.push(LiveDeclaration {
                        declaration: a.declaration,
                        visibility_constraint,
                    });
                }

                EitherOrBoth::Left(declaration) | EitherOrBoth::Right(declaration) => {
                    self.live_declarations.push(declaration);
                }
            }
        }
    }
}

/// Live bindings for a single symbol at some point in control flow. Each live binding comes
/// with a set of narrowing constraints and a visibility constraint.
#[derive(Clone, Debug, Default, PartialEq, Eq, salsa::Update)]
pub(super) struct SymbolBindings {
    /// A list of live bindings for this symbol, sorted by their `ScopedDefinitionId`
    live_bindings: SmallVec<[LiveBinding; INLINE_DEFINITIONS_PER_SYMBOL]>,
}

/// One of the live bindings for a single symbol at some point in control flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LiveBinding {
    pub(super) binding: ScopedDefinitionId,
    pub(super) narrowing_constraint: ScopedNarrowingConstraint,
    pub(super) visibility_constraint: ScopedVisibilityConstraintId,
}

pub(super) type LiveBindingsIterator<'a> = std::slice::Iter<'a, LiveBinding>;

impl SymbolBindings {
    fn unbound(scope_start_visibility: ScopedVisibilityConstraintId) -> Self {
        let initial_binding = LiveBinding {
            binding: ScopedDefinitionId::UNBOUND,
            narrowing_constraint: ScopedNarrowingConstraint::empty(),
            visibility_constraint: scope_start_visibility,
        };
        Self {
            live_bindings: smallvec![initial_binding],
        }
    }

    /// Record a newly-encountered binding for this symbol.
    pub(super) fn record_binding(
        &mut self,
        binding: ScopedDefinitionId,
        visibility_constraint: ScopedVisibilityConstraintId,
    ) {
        // The new binding replaces all previous live bindings in this path, and has no
        // constraints.
        self.live_bindings.clear();
        self.live_bindings.push(LiveBinding {
            binding,
            narrowing_constraint: ScopedNarrowingConstraint::empty(),
            visibility_constraint,
        });
    }

    /// Add given constraint to all live bindings.
    pub(super) fn record_narrowing_constraint(
        &mut self,
        narrowing_constraints: &mut NarrowingConstraintsBuilder,
        predicate: ScopedNarrowingConstraintPredicate,
    ) {
        for binding in &mut self.live_bindings {
            binding.narrowing_constraint = narrowing_constraints
                .add_predicate_to_constraint(binding.narrowing_constraint, predicate);
        }
    }

    /// Add given visibility constraint to all live bindings.
    pub(super) fn record_visibility_constraint(
        &mut self,
        visibility_constraints: &mut VisibilityConstraintsBuilder,
        constraint: ScopedVisibilityConstraintId,
    ) {
        for binding in &mut self.live_bindings {
            binding.visibility_constraint = visibility_constraints
                .add_and_constraint(binding.visibility_constraint, constraint);
        }
    }

    /// Iterate over currently live bindings for this symbol
    pub(super) fn iter(&self) -> LiveBindingsIterator<'_> {
        self.live_bindings.iter()
    }

    /// Iterate over the IDs of each currently live binding for this symbol
    fn iter_bindings(&self) -> impl Iterator<Item = ScopedDefinitionId> + '_ {
        self.iter().map(|lb| lb.binding)
    }

    fn simplify_visibility_constraints(&mut self, other: SymbolBindings) {
        // If the set of live bindings hasn't changed, don't simplify.
        if self.live_bindings.len() != other.live_bindings.len()
            || !self.iter_bindings().eq(other.iter_bindings())
        {
            return;
        }

        for (binding, other_binding) in self.live_bindings.iter_mut().zip(other.live_bindings) {
            binding.visibility_constraint = other_binding.visibility_constraint;
        }
    }

    fn merge(
        &mut self,
        b: Self,
        narrowing_constraints: &mut NarrowingConstraintsBuilder,
        visibility_constraints: &mut VisibilityConstraintsBuilder,
    ) {
        let a = std::mem::take(self);

        // Invariant: merge_join_by consumes the two iterators in sorted order, which ensures that
        // the merged `live_bindings` vec remains sorted. If a definition is found in both `a` and
        // `b`, we compose the constraints from the two paths in an appropriate way (intersection
        // for narrowing constraints; ternary OR for visibility constraints). If a definition is
        // found in only one path, it is used as-is.
        let a = a.live_bindings.into_iter();
        let b = b.live_bindings.into_iter();
        for zipped in a.merge_join_by(b, |a, b| a.binding.cmp(&b.binding)) {
            match zipped {
                EitherOrBoth::Both(a, b) => {
                    // If the same definition is visible through both paths, any constraint
                    // that applies on only one path is irrelevant to the resulting type from
                    // unioning the two paths, so we intersect the constraints.
                    let narrowing_constraint = narrowing_constraints
                        .intersect_constraints(a.narrowing_constraint, b.narrowing_constraint);

                    // For visibility constraints, we merge them using a ternary OR operation:
                    let visibility_constraint = visibility_constraints
                        .add_or_constraint(a.visibility_constraint, b.visibility_constraint);

                    self.live_bindings.push(LiveBinding {
                        binding: a.binding,
                        narrowing_constraint,
                        visibility_constraint,
                    });
                }

                EitherOrBoth::Left(binding) | EitherOrBoth::Right(binding) => {
                    self.live_bindings.push(binding);
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
    pub(super) fn record_binding(
        &mut self,
        binding_id: ScopedDefinitionId,
        visibility_constraint: ScopedVisibilityConstraintId,
    ) {
        debug_assert_ne!(binding_id, ScopedDefinitionId::UNBOUND);
        self.bindings
            .record_binding(binding_id, visibility_constraint);
    }

    /// Add given constraint to all live bindings.
    pub(super) fn record_narrowing_constraint(
        &mut self,
        narrowing_constraints: &mut NarrowingConstraintsBuilder,
        constraint: ScopedNarrowingConstraintPredicate,
    ) {
        self.bindings
            .record_narrowing_constraint(narrowing_constraints, constraint);
    }

    /// Add given visibility constraint to all live bindings.
    pub(super) fn record_visibility_constraint(
        &mut self,
        visibility_constraints: &mut VisibilityConstraintsBuilder,
        constraint: ScopedVisibilityConstraintId,
    ) {
        self.bindings
            .record_visibility_constraint(visibility_constraints, constraint);
        self.declarations
            .record_visibility_constraint(visibility_constraints, constraint);
    }

    /// Simplifies this snapshot to have the same visibility constraints as a previous point in the
    /// control flow, but only if the set of live bindings or declarations for this symbol hasn't
    /// changed.
    pub(super) fn simplify_visibility_constraints(&mut self, snapshot_state: SymbolState) {
        self.bindings
            .simplify_visibility_constraints(snapshot_state.bindings);
        self.declarations
            .simplify_visibility_constraints(snapshot_state.declarations);
    }

    /// Record a newly-encountered declaration of this symbol.
    pub(super) fn record_declaration(&mut self, declaration_id: ScopedDefinitionId) {
        self.declarations.record_declaration(declaration_id);
    }

    /// Merge another [`SymbolState`] into this one.
    pub(super) fn merge(
        &mut self,
        b: SymbolState,
        narrowing_constraints: &mut NarrowingConstraintsBuilder,
        visibility_constraints: &mut VisibilityConstraintsBuilder,
    ) {
        self.bindings
            .merge(b.bindings, narrowing_constraints, visibility_constraints);
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::semantic_index::predicate::ScopedPredicateId;

    #[track_caller]
    fn assert_bindings(
        narrowing_constraints: &NarrowingConstraintsBuilder,
        symbol: &SymbolState,
        expected: &[&str],
    ) {
        let actual = symbol
            .bindings()
            .iter()
            .map(|live_binding| {
                let def_id = live_binding.binding;
                let def = if def_id == ScopedDefinitionId::UNBOUND {
                    "unbound".into()
                } else {
                    def_id.as_u32().to_string()
                };
                let predicates = narrowing_constraints
                    .iter_predicates(live_binding.narrowing_constraint)
                    .map(|idx| idx.as_u32().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{def}<{predicates}>")
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
                |LiveDeclaration {
                     declaration,
                     visibility_constraint: _,
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
        let narrowing_constraints = NarrowingConstraintsBuilder::default();
        let sym = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);

        assert_bindings(&narrowing_constraints, &sym, &["unbound<>"]);
    }

    #[test]
    fn with() {
        let narrowing_constraints = NarrowingConstraintsBuilder::default();
        let mut sym = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym.record_binding(
            ScopedDefinitionId::from_u32(1),
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );

        assert_bindings(&narrowing_constraints, &sym, &["1<>"]);
    }

    #[test]
    fn record_constraint() {
        let mut narrowing_constraints = NarrowingConstraintsBuilder::default();
        let mut sym = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym.record_binding(
            ScopedDefinitionId::from_u32(1),
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        let predicate = ScopedPredicateId::from_u32(0).into();
        sym.record_narrowing_constraint(&mut narrowing_constraints, predicate);

        assert_bindings(&narrowing_constraints, &sym, &["1<0>"]);
    }

    #[test]
    fn merge() {
        let mut narrowing_constraints = NarrowingConstraintsBuilder::default();
        let mut visibility_constraints = VisibilityConstraintsBuilder::default();

        // merging the same definition with the same constraint keeps the constraint
        let mut sym1a = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym1a.record_binding(
            ScopedDefinitionId::from_u32(1),
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        let predicate = ScopedPredicateId::from_u32(0).into();
        sym1a.record_narrowing_constraint(&mut narrowing_constraints, predicate);

        let mut sym1b = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym1b.record_binding(
            ScopedDefinitionId::from_u32(1),
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        let predicate = ScopedPredicateId::from_u32(0).into();
        sym1b.record_narrowing_constraint(&mut narrowing_constraints, predicate);

        sym1a.merge(
            sym1b,
            &mut narrowing_constraints,
            &mut visibility_constraints,
        );
        let mut sym1 = sym1a;
        assert_bindings(&narrowing_constraints, &sym1, &["1<0>"]);

        // merging the same definition with differing constraints drops all constraints
        let mut sym2a = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym2a.record_binding(
            ScopedDefinitionId::from_u32(2),
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        let predicate = ScopedPredicateId::from_u32(1).into();
        sym2a.record_narrowing_constraint(&mut narrowing_constraints, predicate);

        let mut sym1b = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym1b.record_binding(
            ScopedDefinitionId::from_u32(2),
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        let predicate = ScopedPredicateId::from_u32(2).into();
        sym1b.record_narrowing_constraint(&mut narrowing_constraints, predicate);

        sym2a.merge(
            sym1b,
            &mut narrowing_constraints,
            &mut visibility_constraints,
        );
        let sym2 = sym2a;
        assert_bindings(&narrowing_constraints, &sym2, &["2<>"]);

        // merging a constrained definition with unbound keeps both
        let mut sym3a = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym3a.record_binding(
            ScopedDefinitionId::from_u32(3),
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        let predicate = ScopedPredicateId::from_u32(3).into();
        sym3a.record_narrowing_constraint(&mut narrowing_constraints, predicate);

        let sym2b = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);

        sym3a.merge(
            sym2b,
            &mut narrowing_constraints,
            &mut visibility_constraints,
        );
        let sym3 = sym3a;
        assert_bindings(&narrowing_constraints, &sym3, &["unbound<>", "3<3>"]);

        // merging different definitions keeps them each with their existing constraints
        sym1.merge(
            sym3,
            &mut narrowing_constraints,
            &mut visibility_constraints,
        );
        let sym = sym1;
        assert_bindings(&narrowing_constraints, &sym, &["unbound<>", "1<0>", "3<3>"]);
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
        let mut narrowing_constraints = NarrowingConstraintsBuilder::default();
        let mut visibility_constraints = VisibilityConstraintsBuilder::default();
        let mut sym = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym.record_declaration(ScopedDefinitionId::from_u32(1));

        let mut sym2 = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym2.record_declaration(ScopedDefinitionId::from_u32(2));

        sym.merge(
            sym2,
            &mut narrowing_constraints,
            &mut visibility_constraints,
        );

        assert_declarations(&sym, &["1", "2"]);
    }

    #[test]
    fn record_declaration_merge_partial_undeclared() {
        let mut narrowing_constraints = NarrowingConstraintsBuilder::default();
        let mut visibility_constraints = VisibilityConstraintsBuilder::default();
        let mut sym = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);
        sym.record_declaration(ScopedDefinitionId::from_u32(1));

        let sym2 = SymbolState::undefined(ScopedVisibilityConstraintId::ALWAYS_TRUE);

        sym.merge(
            sym2,
            &mut narrowing_constraints,
            &mut visibility_constraints,
        );

        assert_declarations(&sym, &["undeclared", "1"]);
    }
}
