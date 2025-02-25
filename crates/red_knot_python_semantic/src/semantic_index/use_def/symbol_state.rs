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

use ruff_index::list::{ListBuilder, ListReverseIterator, ListStorage};
use ruff_index::newtype_index;

use crate::semantic_index::narrowing_constraints::{
    NarrowingConstraintsBuilder, ScopedNarrowingConstraintClause, ScopedNarrowingConstraintId,
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

/// Live declarations for a single symbol at some point in control flow, with their
/// corresponding visibility constraints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, salsa::Update)]
pub(super) struct SymbolDeclarations {
    /// A list of live declarations for this symbol, sorted by their `ScopedDefinitionId`
    live_declarations: Option<LiveDeclarationsId>,
}

/// One of the live declarations for a single symbol at some point in control flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LiveDeclaration {
    pub(super) visibility_constraint: ScopedVisibilityConstraintId,
}

pub(super) type LiveDeclarationsIterator<'a> =
    ListReverseIterator<'a, LiveDeclarationsId, ScopedDefinitionId, LiveDeclaration>;

impl SymbolDeclarations {
    fn undeclared(
        symbol_states: &mut SymbolStatesBuilder,
        scope_start_visibility: ScopedVisibilityConstraintId,
    ) -> Self {
        let live_declarations = symbol_states
            .declarations
            .entry(None, ScopedDefinitionId::UNBOUND)
            .replace(LiveDeclaration {
                visibility_constraint: scope_start_visibility,
            });
        Self { live_declarations }
    }

    /// Record a newly-encountered declaration for this symbol.
    fn record_declaration(
        &mut self,
        symbol_states: &mut SymbolStatesBuilder,
        declaration: ScopedDefinitionId,
    ) {
        // The new declaration replaces all previous live declaration in this path.
        self.live_declarations =
            symbol_states
                .declarations
                .entry(None, declaration)
                .replace(LiveDeclaration {
                    visibility_constraint: ScopedVisibilityConstraintId::ALWAYS_TRUE,
                });
    }

    /// Add given visibility constraint to all live declarations.
    pub(super) fn record_visibility_constraint(
        &mut self,
        symbol_states: &mut SymbolStatesBuilder,
        visibility_constraints: &mut VisibilityConstraintsBuilder,
        constraint: ScopedVisibilityConstraintId,
    ) {
        self.live_declarations =
            symbol_states
                .declarations
                .map(self.live_declarations, |declaration| LiveDeclaration {
                    visibility_constraint: visibility_constraints
                        .add_and_constraint(declaration.visibility_constraint, constraint),
                });
    }

    /// Return an iterator over live declarations for this symbol.
    pub(super) fn iter(self, symbol_states: &SymbolStatesStorage) -> LiveDeclarationsIterator<'_> {
        symbol_states
            .declarations
            .iter_reverse(self.live_declarations)
    }

    fn simplify_visibility_constraints(
        &mut self,
        symbol_states: &mut SymbolStatesBuilder,
        other: SymbolDeclarations,
    ) {
        // If the set of live declarations hasn't changed, don't simplify.
        let self_declarations = symbol_states
            .declarations
            .iter_reverse(self.live_declarations)
            .map(|(declaration, _)| *declaration);
        let other_declarations = symbol_states
            .declarations
            .iter_reverse(other.live_declarations)
            .map(|(declaration, _)| *declaration);
        if !self_declarations.eq(other_declarations) {
            return;
        }

        // LiveDeclarations only contains visibility_constraints, so we can do a simple copy to
        // reset them.
        self.live_declarations = other.live_declarations;
    }

    fn merge(
        &mut self,
        symbol_states: &mut SymbolStatesBuilder,
        visibility_constraints: &mut VisibilityConstraintsBuilder,
        other: Self,
    ) {
        self.live_declarations = symbol_states.declarations.union_with(
            self.live_declarations,
            other.live_declarations,
            |a, b| {
                let visibility_constraint = visibility_constraints
                    .add_or_constraint(a.visibility_constraint, b.visibility_constraint);
                LiveDeclaration {
                    visibility_constraint,
                }
            },
        );
    }
}

/// Live bindings for a single symbol at some point in control flow. Each live binding comes
/// with a set of narrowing constraints and a visibility constraint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, salsa::Update)]
pub(super) struct SymbolBindings {
    /// A list of live bindings for this symbol, sorted by their `ScopedDefinitionId`
    live_bindings: Option<LiveBindingsId>,
}

/// One of the live bindings for a single symbol at some point in control flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LiveBinding {
    pub(super) narrowing_constraint: Option<ScopedNarrowingConstraintId>,
    pub(super) visibility_constraint: ScopedVisibilityConstraintId,
}

pub(super) type LiveBindingsIterator<'a> =
    ListReverseIterator<'a, LiveBindingsId, ScopedDefinitionId, LiveBinding>;

impl SymbolBindings {
    fn unbound(
        symbol_states: &mut SymbolStatesBuilder,
        scope_start_visibility: ScopedVisibilityConstraintId,
    ) -> Self {
        let live_bindings = symbol_states
            .bindings
            .entry(None, ScopedDefinitionId::UNBOUND)
            .replace(LiveBinding {
                narrowing_constraint: None,
                visibility_constraint: scope_start_visibility,
            });
        Self { live_bindings }
    }

    /// Record a newly-encountered binding for this symbol.
    pub(super) fn record_binding(
        &mut self,
        symbol_states: &mut SymbolStatesBuilder,
        binding: ScopedDefinitionId,
        visibility_constraint: ScopedVisibilityConstraintId,
    ) {
        // The new binding replaces all previous live bindings in this path, and has no
        // constraints.
        self.live_bindings = symbol_states
            .bindings
            .entry(None, binding)
            .replace(LiveBinding {
                narrowing_constraint: None,
                visibility_constraint,
            });
    }

    /// Add given constraint to all live bindings.
    pub(super) fn record_constraint(
        &mut self,
        symbol_states: &mut SymbolStatesBuilder,
        narrowing_constraints: &mut NarrowingConstraintsBuilder,
        constraint: ScopedNarrowingConstraintClause,
    ) {
        self.live_bindings =
            symbol_states
                .bindings
                .map(self.live_bindings, |binding| LiveBinding {
                    narrowing_constraint: narrowing_constraints
                        .add(binding.narrowing_constraint, constraint),
                    visibility_constraint: binding.visibility_constraint,
                });
    }

    /// Add given visibility constraint to all live bindings.
    pub(super) fn record_visibility_constraint(
        &mut self,
        symbol_states: &mut SymbolStatesBuilder,
        visibility_constraints: &mut VisibilityConstraintsBuilder,
        constraint: ScopedVisibilityConstraintId,
    ) {
        self.live_bindings =
            symbol_states
                .bindings
                .map(self.live_bindings, |binding| LiveBinding {
                    narrowing_constraint: binding.narrowing_constraint,
                    visibility_constraint: visibility_constraints
                        .add_and_constraint(binding.visibility_constraint, constraint),
                });
    }

    /// Iterate over currently live bindings for this symbol
    pub(super) fn iter(self, symbol_states: &SymbolStatesStorage) -> LiveBindingsIterator<'_> {
        symbol_states.bindings.iter_reverse(self.live_bindings)
    }

    fn simplify_visibility_constraints(
        &mut self,
        symbol_states: &mut SymbolStatesBuilder,
        other: SymbolBindings,
    ) {
        // If the set of live bindings hasn't changed, don't simplify.
        let self_bindings = symbol_states
            .bindings
            .iter_reverse(self.live_bindings)
            .map(|(binding, _)| *binding);
        let other_bindings = symbol_states
            .bindings
            .iter_reverse(other.live_bindings)
            .map(|(binding, _)| *binding);
        if !self_bindings.eq(other_bindings) {
            return;
        }

        // We can't just copy other.live_bindings, since the narrowing constraints might be
        // different.
        self.live_bindings = symbol_states.bindings.intersect_with(
            self.live_bindings,
            other.live_bindings,
            |binding, other_binding| LiveBinding {
                narrowing_constraint: binding.narrowing_constraint,
                visibility_constraint: other_binding.visibility_constraint,
            },
        );
    }

    fn merge(
        &mut self,
        symbol_states: &mut SymbolStatesBuilder,
        narrowing_constraints: &mut NarrowingConstraintsBuilder,
        visibility_constraints: &mut VisibilityConstraintsBuilder,
        other: Self,
    ) {
        self.live_bindings =
            symbol_states
                .bindings
                .union_with(self.live_bindings, other.live_bindings, |a, b| {
                    // If the same definition is visible through both paths, any constraint
                    // that applies on only one path is irrelevant to the resulting type from
                    // unioning the two paths, so we intersect the constraints.
                    let narrowing_constraint = narrowing_constraints
                        .intersect(a.narrowing_constraint, b.narrowing_constraint);

                    // For visibility constraints, we merge them using a ternary OR operation:
                    let visibility_constraint = visibility_constraints
                        .add_or_constraint(a.visibility_constraint, b.visibility_constraint);

                    LiveBinding {
                        narrowing_constraint,
                        visibility_constraint,
                    }
                });
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SymbolState {
    declarations: SymbolDeclarations,
    bindings: SymbolBindings,
}

impl SymbolState {
    /// Return a new [`SymbolState`] representing an unbound, undeclared symbol.
    pub(super) fn undefined(
        symbol_states: &mut SymbolStatesBuilder,
        scope_start_visibility: ScopedVisibilityConstraintId,
    ) -> Self {
        Self {
            declarations: SymbolDeclarations::undeclared(symbol_states, scope_start_visibility),
            bindings: SymbolBindings::unbound(symbol_states, scope_start_visibility),
        }
    }

    /// Record a newly-encountered binding for this symbol.
    pub(super) fn record_binding(
        &mut self,
        symbol_states: &mut SymbolStatesBuilder,
        binding_id: ScopedDefinitionId,
        visibility_constraint: ScopedVisibilityConstraintId,
    ) {
        debug_assert_ne!(binding_id, ScopedDefinitionId::UNBOUND);
        self.bindings
            .record_binding(symbol_states, binding_id, visibility_constraint);
    }

    /// Add given constraint to all live bindings.
    pub(super) fn record_constraint(
        &mut self,
        symbol_states: &mut SymbolStatesBuilder,
        narrowing_constraints: &mut NarrowingConstraintsBuilder,
        constraint: ScopedNarrowingConstraintClause,
    ) {
        self.bindings
            .record_constraint(symbol_states, narrowing_constraints, constraint);
    }

    /// Add given visibility constraint to all live bindings.
    pub(super) fn record_visibility_constraint(
        &mut self,
        symbol_states: &mut SymbolStatesBuilder,
        visibility_constraints: &mut VisibilityConstraintsBuilder,
        constraint: ScopedVisibilityConstraintId,
    ) {
        self.bindings.record_visibility_constraint(
            symbol_states,
            visibility_constraints,
            constraint,
        );
        self.declarations.record_visibility_constraint(
            symbol_states,
            visibility_constraints,
            constraint,
        );
    }

    /// Simplifies this snapshot to have the same visibility constraints as a previous point in the
    /// control flow, but only if the set of live bindings or declarations for this symbol hasn't
    /// changed.
    pub(super) fn simplify_visibility_constraints(
        &mut self,
        symbol_states: &mut SymbolStatesBuilder,
        snapshot_state: &SymbolState,
    ) {
        self.bindings
            .simplify_visibility_constraints(symbol_states, snapshot_state.bindings);
        self.declarations
            .simplify_visibility_constraints(symbol_states, snapshot_state.declarations);
    }

    /// Record a newly-encountered declaration of this symbol.
    pub(super) fn record_declaration(
        &mut self,
        symbol_states: &mut SymbolStatesBuilder,
        declaration_id: ScopedDefinitionId,
    ) {
        self.declarations
            .record_declaration(symbol_states, declaration_id);
    }

    /// Merge another [`SymbolState`] into this one.
    pub(super) fn merge(
        &mut self,
        symbol_states: &mut SymbolStatesBuilder,
        narrowing_constraints: &mut NarrowingConstraintsBuilder,
        visibility_constraints: &mut VisibilityConstraintsBuilder,
        other: &SymbolState,
    ) {
        self.bindings.merge(
            symbol_states,
            narrowing_constraints,
            visibility_constraints,
            other.bindings,
        );
        self.declarations
            .merge(symbol_states, visibility_constraints, other.declarations);
    }

    pub(super) fn bindings(&self) -> SymbolBindings {
        self.bindings
    }

    pub(super) fn declarations(&self) -> SymbolDeclarations {
        self.declarations
    }
}

// Arena storage
// -------------

#[newtype_index]
pub(super) struct LiveBindingsId;

#[newtype_index]
pub(super) struct LiveDeclarationsId;

#[derive(Debug, Eq, PartialEq)]
pub(super) struct SymbolStatesStorage {
    bindings: ListStorage<LiveBindingsId, ScopedDefinitionId, LiveBinding>,
    declarations: ListStorage<LiveDeclarationsId, ScopedDefinitionId, LiveDeclaration>,
}

#[derive(Debug, Default)]
pub(super) struct SymbolStatesBuilder {
    bindings: ListBuilder<LiveBindingsId, ScopedDefinitionId, LiveBinding>,
    declarations: ListBuilder<LiveDeclarationsId, ScopedDefinitionId, LiveDeclaration>,
}

impl SymbolStatesBuilder {
    pub(super) fn build(self) -> SymbolStatesStorage {
        SymbolStatesStorage {
            bindings: self.bindings.build(),
            declarations: self.declarations.build(),
        }
    }
}

// Tests
// -----

#[cfg(test)]
mod tests {
    use super::*;

    use crate::semantic_index::constraint::ScopedConstraintId;

    #[track_caller]
    fn assert_bindings(
        symbol_states: &SymbolStatesBuilder,
        narrowing_constraints: &NarrowingConstraintsBuilder,
        symbol: &SymbolState,
        expected: &[&str],
    ) {
        let actual = symbol_states
            .bindings
            .iter_reverse(symbol.bindings.live_bindings)
            .map(|(def_id, live_binding)| {
                let def = if *def_id == ScopedDefinitionId::UNBOUND {
                    "unbound".into()
                } else {
                    def_id.as_u32().to_string()
                };
                let constraints = narrowing_constraints
                    .iter_constraints(live_binding.narrowing_constraint)
                    .map(|idx| idx.as_u32().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{def}<{constraints}>")
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    #[track_caller]
    pub(crate) fn assert_declarations(
        symbol_states: &SymbolStatesBuilder,
        symbol: &SymbolState,
        expected: &[&str],
    ) {
        let actual = symbol_states
            .declarations
            .iter_reverse(symbol.declarations.live_declarations)
            .map(|(declaration, _)| {
                if *declaration == ScopedDefinitionId::UNBOUND {
                    "undeclared".into()
                } else {
                    declaration.as_u32().to_string()
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    #[test]
    fn unbound() {
        let mut symbol_states = SymbolStatesBuilder::default();
        let narrowing_constraints = NarrowingConstraintsBuilder::default();
        let sym = SymbolState::undefined(
            &mut symbol_states,
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );

        assert_bindings(&symbol_states, &narrowing_constraints, &sym, &["unbound<>"]);
    }

    #[test]
    fn with() {
        let mut symbol_states = SymbolStatesBuilder::default();
        let narrowing_constraints = NarrowingConstraintsBuilder::default();
        let mut sym = SymbolState::undefined(
            &mut symbol_states,
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        sym.record_binding(
            &mut symbol_states,
            ScopedDefinitionId::from_u32(1),
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );

        assert_bindings(&symbol_states, &narrowing_constraints, &sym, &["1<>"]);
    }

    #[test]
    fn record_constraint() {
        let mut symbol_states = SymbolStatesBuilder::default();
        let mut narrowing_constraints = NarrowingConstraintsBuilder::default();
        let mut sym = SymbolState::undefined(
            &mut symbol_states,
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        sym.record_binding(
            &mut symbol_states,
            ScopedDefinitionId::from_u32(1),
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        let constraint = ScopedConstraintId::from_u32(0).into();
        sym.record_constraint(&mut symbol_states, &mut narrowing_constraints, constraint);

        assert_bindings(&symbol_states, &narrowing_constraints, &sym, &["1<0>"]);
    }

    #[test]
    fn merge() {
        let mut symbol_states = SymbolStatesBuilder::default();
        let mut narrowing_constraints = NarrowingConstraintsBuilder::default();
        let mut visibility_constraints = VisibilityConstraintsBuilder::default();

        // merging the same definition with the same constraint keeps the constraint
        let mut sym1a = SymbolState::undefined(
            &mut symbol_states,
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        sym1a.record_binding(
            &mut symbol_states,
            ScopedDefinitionId::from_u32(1),
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        let constraint = ScopedConstraintId::from_u32(0).into();
        sym1a.record_constraint(&mut symbol_states, &mut narrowing_constraints, constraint);

        let mut sym1b = SymbolState::undefined(
            &mut symbol_states,
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        sym1b.record_binding(
            &mut symbol_states,
            ScopedDefinitionId::from_u32(1),
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        let constraint = ScopedConstraintId::from_u32(0).into();
        sym1b.record_constraint(&mut symbol_states, &mut narrowing_constraints, constraint);

        sym1a.merge(
            &mut symbol_states,
            &mut narrowing_constraints,
            &mut visibility_constraints,
            &sym1b,
        );
        let mut sym1 = sym1a;
        assert_bindings(&symbol_states, &narrowing_constraints, &sym1, &["1<0>"]);

        // merging the same definition with differing constraints drops all constraints
        let mut sym2a = SymbolState::undefined(
            &mut symbol_states,
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        sym2a.record_binding(
            &mut symbol_states,
            ScopedDefinitionId::from_u32(2),
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        let constraint = ScopedConstraintId::from_u32(1).into();
        sym2a.record_constraint(&mut symbol_states, &mut narrowing_constraints, constraint);

        let mut sym1b = SymbolState::undefined(
            &mut symbol_states,
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        sym1b.record_binding(
            &mut symbol_states,
            ScopedDefinitionId::from_u32(2),
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        let constraint = ScopedConstraintId::from_u32(2).into();
        sym1b.record_constraint(&mut symbol_states, &mut narrowing_constraints, constraint);

        sym2a.merge(
            &mut symbol_states,
            &mut narrowing_constraints,
            &mut visibility_constraints,
            &sym1b,
        );
        let sym2 = sym2a;
        assert_bindings(&symbol_states, &narrowing_constraints, &sym2, &["2<>"]);

        // merging a constrained definition with unbound keeps both
        let mut sym3a = SymbolState::undefined(
            &mut symbol_states,
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        sym3a.record_binding(
            &mut symbol_states,
            ScopedDefinitionId::from_u32(3),
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        let constraint = ScopedConstraintId::from_u32(3).into();
        sym3a.record_constraint(&mut symbol_states, &mut narrowing_constraints, constraint);

        let sym2b = SymbolState::undefined(
            &mut symbol_states,
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );

        sym3a.merge(
            &mut symbol_states,
            &mut narrowing_constraints,
            &mut visibility_constraints,
            &sym2b,
        );
        let sym3 = sym3a;
        assert_bindings(
            &symbol_states,
            &narrowing_constraints,
            &sym3,
            &["unbound<>", "3<3>"],
        );

        // merging different definitions keeps them each with their existing constraints
        sym1.merge(
            &mut symbol_states,
            &mut narrowing_constraints,
            &mut visibility_constraints,
            &sym3,
        );
        let sym = sym1;
        assert_bindings(
            &symbol_states,
            &narrowing_constraints,
            &sym,
            &["unbound<>", "1<0>", "3<3>"],
        );
    }

    #[test]
    fn no_declaration() {
        let mut symbol_states = SymbolStatesBuilder::default();
        let sym = SymbolState::undefined(
            &mut symbol_states,
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );

        assert_declarations(&symbol_states, &sym, &["undeclared"]);
    }

    #[test]
    fn record_declaration() {
        let mut symbol_states = SymbolStatesBuilder::default();
        let mut sym = SymbolState::undefined(
            &mut symbol_states,
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        sym.record_declaration(&mut symbol_states, ScopedDefinitionId::from_u32(1));

        assert_declarations(&symbol_states, &sym, &["1"]);
    }

    #[test]
    fn record_declaration_override() {
        let mut symbol_states = SymbolStatesBuilder::default();
        let mut sym = SymbolState::undefined(
            &mut symbol_states,
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        sym.record_declaration(&mut symbol_states, ScopedDefinitionId::from_u32(1));
        sym.record_declaration(&mut symbol_states, ScopedDefinitionId::from_u32(2));

        assert_declarations(&symbol_states, &sym, &["2"]);
    }

    #[test]
    fn record_declaration_merge() {
        let mut symbol_states = SymbolStatesBuilder::default();
        let mut narrowing_constraints = NarrowingConstraintsBuilder::default();
        let mut visibility_constraints = VisibilityConstraintsBuilder::default();
        let mut sym = SymbolState::undefined(
            &mut symbol_states,
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        sym.record_declaration(&mut symbol_states, ScopedDefinitionId::from_u32(1));

        let mut sym2 = SymbolState::undefined(
            &mut symbol_states,
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        sym2.record_declaration(&mut symbol_states, ScopedDefinitionId::from_u32(2));

        sym.merge(
            &mut symbol_states,
            &mut narrowing_constraints,
            &mut visibility_constraints,
            &sym2,
        );

        assert_declarations(&symbol_states, &sym, &["1", "2"]);
    }

    #[test]
    fn record_declaration_merge_partial_undeclared() {
        let mut symbol_states = SymbolStatesBuilder::default();
        let mut narrowing_constraints = NarrowingConstraintsBuilder::default();
        let mut visibility_constraints = VisibilityConstraintsBuilder::default();
        let mut sym = SymbolState::undefined(
            &mut symbol_states,
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );
        sym.record_declaration(&mut symbol_states, ScopedDefinitionId::from_u32(1));

        let sym2 = SymbolState::undefined(
            &mut symbol_states,
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );

        sym.merge(
            &mut symbol_states,
            &mut narrowing_constraints,
            &mut visibility_constraints,
            &sym2,
        );

        assert_declarations(&symbol_states, &sym, &["undeclared", "1"]);
    }
}
