//! Build a map from each use of a symbol to the definitions visible from that use, and the
//! type-narrowing constraints that apply to each definition.
//!
//! Let's take this code sample:
//!
//! ```python
//! x = 1
//! x = 2
//! y = x
//! if y is not None:
//!     x = 3
//! else:
//!     x = 4
//! z = x
//! ```
//!
//! In this snippet, we have four definitions of `x` (the statements assigning `1`, `2`, `3`,
//! and `4` to it), and two uses of `x` (the `y = x` and `z = x` assignments). The first
//! [`Definition`] of `x` is never visible to any use, because it's immediately replaced by the
//! second definition, before any use happens. (A linter could thus flag the statement `x = 1`
//! as likely superfluous.)
//!
//! The first use of `x` has one definition visible to it: the assignment `x = 2`.
//!
//! Things get a bit more complex when we have branches. We will definitely take either the `if` or
//! the `else` branch. Thus, the second use of `x` has two definitions visible to it: `x = 3` and
//! `x = 4`. The `x = 2` definition is no longer visible, because it must be replaced by either `x
//! = 3` or `x = 4`, no matter which branch was taken. We don't know which branch was taken, so we
//! must consider both definitions as visible, which means eventually we would (in type inference)
//! look at these two definitions and infer a type of `Literal[3, 4]` -- the union of `Literal[3]`
//! and `Literal[4]` -- for the second use of `x`.
//!
//! So that's one question our use-def map needs to answer: given a specific use of a symbol, which
//! definition(s) is/are visible from that use. In
//! [`AstIds`](crate::semantic_index::ast_ids::AstIds) we number all uses (that means a `Name` node
//! with `Load` context) so we have a `ScopedUseId` to efficiently represent each use.
//!
//! Another case we need to handle is when a symbol is referenced from a different scope (the most
//! obvious example of this is an import). We call this "public" use of a symbol. So the other
//! question we need to be able to answer is, what are the publicly-visible definitions of each
//! symbol?
//!
//! Technically, public use of a symbol could also occur from any point in control flow of the
//! scope where the symbol is defined (via inline imports and import cycles, in the case of an
//! import, or via a function call partway through the local scope that ends up using a symbol from
//! the scope via a global or nonlocal reference.) But modeling this fully accurately requires
//! whole-program analysis that isn't tractable for an efficient incremental compiler, since it
//! means a given symbol could have a different type every place it's referenced throughout the
//! program, depending on the shape of arbitrarily-sized call/import graphs. So we follow other
//! Python type-checkers in making the simplifying assumption that usually the scope will finish
//! execution before its symbols are made visible to other scopes; for instance, most imports will
//! import from a complete module, not a partially-executed module. (We may want to get a little
//! smarter than this in the future, in particular for closures, but for now this is where we
//! start.)
//!
//! So this means that the publicly-visible definitions of a symbol are the definitions still
//! visible at the end of the scope; effectively we have an implicit "use" of every symbol at the
//! end of the scope.
//!
//! We also need to know, for a given definition of a symbol, what type-narrowing constraints apply
//! to it. For instance, in this code sample:
//!
//! ```python
//! x = 1 if flag else None
//! if x is not None:
//!     y = x
//! ```
//!
//! At the use of `x` in `y = x`, the visible definition of `x` is `1 if flag else None`, which
//! would infer as the type `Literal[1] | None`. But the constraint `x is not None` dominates this
//! use, which means we can rule out the possibility that `x` is `None` here, which should give us
//! the type `Literal[1]` for this use.
//!
//! The data structure we build to answer these questions is the `UseDefMap`. It has a
//! `definitions_by_use` vector indexed by [`ScopedUseId`] and a `public_definitions` vector
//! indexed by [`ScopedSymbolId`]. The values in each of these vectors are (in principle) a list of
//! visible definitions at that use, or at the end of the scope for that symbol, with a list of the
//! dominating constraints for each of those definitions.
//!
//! In order to avoid vectors-of-vectors-of-vectors and all the allocations that would entail, we
//! don't actually store these "list of visible definitions" as a vector of [`Definition`].
//! Instead, the values in `definitions_by_use` and `public_definitions` are a [`SymbolState`]
//! struct which uses bit-sets to track definitions and constraints in terms of
//! [`ScopedDefinitionId`] and [`ScopedConstraintId`], which are indices into the `all_definitions`
//! and `all_constraints` indexvecs in the [`UseDefMap`].
//!
//! There is another special kind of possible "definition" for a symbol: there might be a path from
//! the scope entry to a given use in which the symbol is never bound.
//!
//! The simplest way to model "unbound" would be as an actual [`Definition`] itself: the initial
//! visible [`Definition`] for each symbol in a scope. But actually modeling it this way would
//! unnecessarily increase the number of [`Definition`] that Salsa must track. Since "unbound" is a
//! special definition in that all symbols share it, and it doesn't have any additional per-symbol
//! state, and constraints are irrelevant to it, we can represent it more efficiently: we use the
//! `may_be_unbound` boolean on the [`SymbolState`] struct. If this flag is `true`, it means the
//! symbol/use really has one additional visible "definition", which is the unbound state. If this
//! flag is `false`, it means we've eliminated the possibility of unbound: every path we've
//! followed includes a definition for this symbol.
//!
//! To build a [`UseDefMap`], the [`UseDefMapBuilder`] is notified of each new use, definition, and
//! constraint as they are encountered by the
//! [`SemanticIndexBuilder`](crate::semantic_index::builder::SemanticIndexBuilder) AST visit. For
//! each symbol, the builder tracks the `SymbolState` for that symbol. When we hit a use of a
//! symbol, it records the current state for that symbol for that use. When we reach the end of the
//! scope, it records the state for each symbol as the public definitions of that symbol.
//!
//! Let's walk through the above example. Initially we record for `x` that it has no visible
//! definitions, and may be unbound. When we see `x = 1`, we record that as the sole visible
//! definition of `x`, and flip `may_be_unbound` to `false`. Then we see `x = 2`, and it replaces
//! `x = 1` as the sole visible definition of `x`. When we get to `y = x`, we record that the
//! visible definitions for that use of `x` are just the `x = 2` definition.
//!
//! Then we hit the `if` branch. We visit the `test` node (`flag` in this case), since that will
//! happen regardless. Then we take a pre-branch snapshot of the currently visible definitions for
//! all symbols, which we'll need later. Then we record `flag` as a possible constraint on the
//! currently visible definition (`x = 2`), and go ahead and visit the `if` body. When we see `x =
//! 3`, it replaces `x = 2` (constrained by `flag`) as the sole visible definition of `x`. At the
//! end of the `if` body, we take another snapshot of the currently-visible definitions; we'll call
//! this the post-if-body snapshot.
//!
//! Now we need to visit the `else` clause. The conditions when entering the `else` clause should
//! be the pre-if conditions; if we are entering the `else` clause, we know that the `if` test
//! failed and we didn't execute the `if` body. So we first reset the builder to the pre-if state,
//! using the snapshot we took previously (meaning we now have `x = 2` as the sole visible
//! definition for `x` again), then visit the `else` clause, where `x = 4` replaces `x = 2` as the
//! sole visible definition of `x`.
//!
//! Now we reach the end of the if/else, and want to visit the following code. The state here needs
//! to reflect that we might have gone through the `if` branch, or we might have gone through the
//! `else` branch, and we don't know which. So we need to "merge" our current builder state
//! (reflecting the end-of-else state, with `x = 4` as the only visible definition) with our
//! post-if-body snapshot (which has `x = 3` as the only visible definition). The result of this
//! merge is that we now have two visible definitions of `x`: `x = 3` and `x = 4`.
//!
//! The [`UseDefMapBuilder`] itself just exposes methods for taking a snapshot, resetting to a
//! snapshot, and merging a snapshot into the current state. The logic using these methods lives in
//! [`SemanticIndexBuilder`](crate::semantic_index::builder::SemanticIndexBuilder), e.g. where it
//! visits a `StmtIf` node.
//!
//! (In the future we may have some other questions we want to answer as well, such as "is this
//! definition used?", which will require tracking a bit more info in our map, e.g. a "used" bit
//! for each [`Definition`] which is flipped to true when we record that definition for a use.)
use self::symbol_state::{
    ConstraintIdIterator, DefinitionIdWithConstraintsIterator, ScopedConstraintId,
    ScopedDefinitionId, SymbolState,
};
use crate::semantic_index::ast_ids::ScopedUseId;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::expression::Expression;
use crate::semantic_index::symbol::ScopedSymbolId;
use ruff_index::IndexVec;

mod bitset;
mod symbol_state;

/// Applicable definitions and constraints for every use of a name.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UseDefMap<'db> {
    /// Array of [`Definition`] in this scope.
    all_definitions: IndexVec<ScopedDefinitionId, Definition<'db>>,

    /// Array of constraints (as [`Expression`]) in this scope.
    all_constraints: IndexVec<ScopedConstraintId, Expression<'db>>,

    /// [`SymbolState`] visible at a [`ScopedUseId`].
    definitions_by_use: IndexVec<ScopedUseId, SymbolState>,

    /// [`SymbolState`] visible at end of scope for each symbol.
    public_definitions: IndexVec<ScopedSymbolId, SymbolState>,
}

impl<'db> UseDefMap<'db> {
    pub(crate) fn use_definitions(
        &self,
        use_id: ScopedUseId,
    ) -> DefinitionWithConstraintsIterator<'_, 'db> {
        DefinitionWithConstraintsIterator {
            all_definitions: &self.all_definitions,
            all_constraints: &self.all_constraints,
            inner: self.definitions_by_use[use_id].visible_definitions(),
        }
    }

    pub(crate) fn use_may_be_unbound(&self, use_id: ScopedUseId) -> bool {
        self.definitions_by_use[use_id].may_be_unbound()
    }

    pub(crate) fn public_definitions(
        &self,
        symbol: ScopedSymbolId,
    ) -> DefinitionWithConstraintsIterator<'_, 'db> {
        DefinitionWithConstraintsIterator {
            all_definitions: &self.all_definitions,
            all_constraints: &self.all_constraints,
            inner: self.public_definitions[symbol].visible_definitions(),
        }
    }

    pub(crate) fn public_may_be_unbound(&self, symbol: ScopedSymbolId) -> bool {
        self.public_definitions[symbol].may_be_unbound()
    }
}

#[derive(Debug)]
pub(crate) struct DefinitionWithConstraintsIterator<'map, 'db> {
    all_definitions: &'map IndexVec<ScopedDefinitionId, Definition<'db>>,
    all_constraints: &'map IndexVec<ScopedConstraintId, Expression<'db>>,
    inner: DefinitionIdWithConstraintsIterator<'map>,
}

impl<'map, 'db> Iterator for DefinitionWithConstraintsIterator<'map, 'db> {
    type Item = DefinitionWithConstraints<'map, 'db>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|def_id_with_constraints| DefinitionWithConstraints {
                definition: self.all_definitions[def_id_with_constraints.definition],
                constraints: ConstraintsIterator {
                    all_constraints: self.all_constraints,
                    constraint_ids: def_id_with_constraints.constraint_ids,
                },
            })
    }
}

impl std::iter::FusedIterator for DefinitionWithConstraintsIterator<'_, '_> {}

pub(crate) struct DefinitionWithConstraints<'map, 'db> {
    pub(crate) definition: Definition<'db>,
    pub(crate) constraints: ConstraintsIterator<'map, 'db>,
}

pub(crate) struct ConstraintsIterator<'map, 'db> {
    all_constraints: &'map IndexVec<ScopedConstraintId, Expression<'db>>,
    constraint_ids: ConstraintIdIterator<'map>,
}

impl<'map, 'db> Iterator for ConstraintsIterator<'map, 'db> {
    type Item = Expression<'db>;

    fn next(&mut self) -> Option<Self::Item> {
        self.constraint_ids
            .next()
            .map(|constraint_id| self.all_constraints[constraint_id])
    }
}

impl std::iter::FusedIterator for ConstraintsIterator<'_, '_> {}

/// A snapshot of the definitions and constraints state at a particular point in control flow.
#[derive(Clone, Debug)]
pub(super) struct FlowSnapshot {
    definitions_by_symbol: IndexVec<ScopedSymbolId, SymbolState>,
}

#[derive(Debug, Default)]
pub(super) struct UseDefMapBuilder<'db> {
    /// Append-only array of [`Definition`]; None is unbound.
    all_definitions: IndexVec<ScopedDefinitionId, Definition<'db>>,

    /// Append-only array of constraints (as [`Expression`]).
    all_constraints: IndexVec<ScopedConstraintId, Expression<'db>>,

    /// Visible definitions at each so-far-recorded use.
    definitions_by_use: IndexVec<ScopedUseId, SymbolState>,

    /// Currently visible definitions for each symbol.
    definitions_by_symbol: IndexVec<ScopedSymbolId, SymbolState>,
}

impl<'db> UseDefMapBuilder<'db> {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn add_symbol(&mut self, symbol: ScopedSymbolId) {
        let new_symbol = self.definitions_by_symbol.push(SymbolState::unbound());
        debug_assert_eq!(symbol, new_symbol);
    }

    pub(super) fn record_definition(
        &mut self,
        symbol: ScopedSymbolId,
        definition: Definition<'db>,
    ) {
        // We have a new definition of a symbol; this replaces any previous definitions in this
        // path.
        let def_id = self.all_definitions.push(definition);
        self.definitions_by_symbol[symbol] = SymbolState::with(def_id);
    }

    pub(super) fn record_constraint(&mut self, constraint: Expression<'db>) {
        let constraint_id = self.all_constraints.push(constraint);
        for definitions in &mut self.definitions_by_symbol {
            definitions.add_constraint(constraint_id);
        }
    }

    pub(super) fn record_use(&mut self, symbol: ScopedSymbolId, use_id: ScopedUseId) {
        // We have a use of a symbol; clone the currently visible definitions for that symbol, and
        // record them as the visible definitions for this use.
        let new_use = self
            .definitions_by_use
            .push(self.definitions_by_symbol[symbol].clone());
        debug_assert_eq!(use_id, new_use);
    }

    /// Take a snapshot of the current visible-symbols state.
    pub(super) fn snapshot(&self) -> FlowSnapshot {
        FlowSnapshot {
            definitions_by_symbol: self.definitions_by_symbol.clone(),
        }
    }

    /// Restore the current builder visible-definitions state to the given snapshot.
    pub(super) fn restore(&mut self, snapshot: FlowSnapshot) {
        // We never remove symbols from `definitions_by_symbol` (it's an IndexVec, and the symbol
        // IDs must line up), so the current number of known symbols must always be equal to or
        // greater than the number of known symbols in a previously-taken snapshot.
        let num_symbols = self.definitions_by_symbol.len();
        debug_assert!(num_symbols >= snapshot.definitions_by_symbol.len());

        // Restore the current visible-definitions state to the given snapshot.
        self.definitions_by_symbol = snapshot.definitions_by_symbol;

        // If the snapshot we are restoring is missing some symbols we've recorded since, we need
        // to fill them in so the symbol IDs continue to line up. Since they don't exist in the
        // snapshot, the correct state to fill them in with is "unbound".
        self.definitions_by_symbol
            .resize(num_symbols, SymbolState::unbound());
    }

    /// Merge the given snapshot into the current state, reflecting that we might have taken either
    /// path to get here. The new visible-definitions state for each symbol should include
    /// definitions from both the prior state and the snapshot.
    pub(super) fn merge(&mut self, snapshot: &FlowSnapshot) {
        // The tricky thing about merging two Ranges pointing into `all_definitions` is that if the
        // two Ranges aren't already adjacent in `all_definitions`, we will have to copy at least
        // one or the other of the ranges to the end of `all_definitions` so as to make them
        // adjacent. We can't ever move things around in `all_definitions` because previously
        // recorded uses may still have ranges pointing to any part of it; all we can do is append.
        // It's possible we may end up with some old entries in `all_definitions` that nobody is
        // pointing to, but that's OK.

        // We never remove symbols from `definitions_by_symbol` (it's an IndexVec, and the symbol
        // IDs must line up), so the current number of known symbols must always be equal to or
        // greater than the number of known symbols in a previously-taken snapshot.
        debug_assert!(self.definitions_by_symbol.len() >= snapshot.definitions_by_symbol.len());

        for (symbol_id, current) in self.definitions_by_symbol.iter_mut_enumerated() {
            if let Some(snapshot) = snapshot.definitions_by_symbol.get(symbol_id) {
                *current = SymbolState::merge(current, snapshot);
            } else {
                // Symbol not present in snapshot, so it's unbound from that path.
                current.add_unbound();
            }
        }
    }

    pub(super) fn finish(mut self) -> UseDefMap<'db> {
        self.all_definitions.shrink_to_fit();
        self.all_constraints.shrink_to_fit();
        self.definitions_by_symbol.shrink_to_fit();
        self.definitions_by_use.shrink_to_fit();

        UseDefMap {
            all_definitions: self.all_definitions,
            all_constraints: self.all_constraints,
            definitions_by_use: self.definitions_by_use,
            public_definitions: self.definitions_by_symbol,
        }
    }
}
