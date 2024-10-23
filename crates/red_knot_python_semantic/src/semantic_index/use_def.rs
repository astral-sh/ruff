//! First, some terminology:
//!
//! * A "binding" gives a new value to a variable. This includes many different Python statements
//!   (assignment statements of course, but also imports, `def` and `class` statements, `as`
//!   clauses in `with` and `except` statements, match patterns, and others) and even one
//!   expression kind (named expressions). It notably does not include annotated assignment
//!   statements without a right-hand side value; these do not assign any new value to the
//!   variable. We consider function parameters to be bindings as well, since (from the perspective
//!   of the function's internal scope), a function parameter begins the scope bound to a value.
//!
//! * A "declaration" establishes an upper bound type for the values that a variable may be
//!   permitted to take on. Annotated assignment statements (with or without an RHS value) are
//!   declarations; annotated function parameters are also declarations. We consider `def` and
//!   `class` statements to also be declarations, so as to prohibit accidentally shadowing them.
//!
//! Annotated assignments with a right-hand side, and annotated function parameters, are both
//! bindings and declarations.
//!
//! We use [`Definition`] as the universal term (and Salsa tracked struct) encompassing both
//! bindings and declarations. (This sacrifices a bit of type safety in exchange for improved
//! performance via fewer Salsa tracked structs and queries, since most declarations -- typed
//! parameters and annotated assignments with RHS -- are both bindings and declarations.)
//!
//! At any given use of a variable, we can ask about both its "declared type" and its "inferred
//! type". These may be different, but the inferred type must always be assignable to the declared
//! type; that is, the declared type is always wider, and the inferred type may be more precise. If
//! we see an invalid assignment, we emit a diagnostic and abandon our inferred type, deferring to
//! the declared type (this allows an explicit annotation to override bad inference, without a
//! cast), maintaining the invariant.
//!
//! The **inferred type** represents the most precise type we believe encompasses all possible
//! values for the variable at a given use. It is based on a union of the bindings which can reach
//! that use through some control flow path, and the narrowing constraints that control flow must
//! have passed through between the binding and the use. For example, in this code:
//!
//! ```python
//! x = 1 if flag else None
//! if x is not None:
//!     use(x)
//! ```
//!
//! For the use of `x` on the third line, the inferred type should be `Literal[1]`. This is based
//! on the binding on the first line, which assigns the type `Literal[1] | None`, and the narrowing
//! constraint on the second line, which rules out the type `None`, since control flow must pass
//! through this constraint to reach the use in question.
//!
//! The **declared type** represents the code author's declaration (usually through a type
//! annotation) that a given variable should not be assigned any type outside the declared type. In
//! our model, declared types are also control-flow-sensitive; we allow the code author to
//! explicitly re-declare the same variable with a different type. So for a given binding of a
//! variable, we will want to ask which declarations of that variable can reach that binding, in
//! order to determine whether the binding is permitted, or should be a type error. For example:
//!
//! ```python
//! from pathlib import Path
//! def f(path: str):
//!     path: Path = Path(path)
//! ```
//!
//! In this function, the initial declared type of `path` is `str`, meaning that the assignment
//! `path = Path(path)` would be a type error, since it assigns to `path` a value whose type is not
//! assignable to `str`. This is the purpose of declared types: they prevent accidental assignment
//! of the wrong type to a variable.
//!
//! But in some cases it is useful to "shadow" or "re-declare" a variable with a new type, and we
//! permit this, as long as it is done with an explicit re-annotation. So `path: Path =
//! Path(path)`, with the explicit `: Path` annotation, is permitted.
//!
//! The general rule is that whatever declaration(s) can reach a given binding determine the
//! validity of that binding. If there is a path in which the symbol is not declared, that is a
//! declaration of `Unknown`. If multiple declarations can reach a binding, we union them, but by
//! default we also issue a type error, since this implicit union of declared types may hide an
//! error.
//!
//! To support type inference, we build a map from each use of a symbol to the bindings live at
//! that use, and the type narrowing constraints that apply to each binding.
//!
//! Let's take this code sample:
//!
//! ```python
//! x = 1
//! x = 2
//! y = x
//! if flag:
//!     x = 3
//! else:
//!     x = 4
//! z = x
//! ```
//!
//! In this snippet, we have four bindings of `x` (the statements assigning `1`, `2`, `3`, and `4`
//! to it), and two uses of `x` (the `y = x` and `z = x` assignments). The first binding of `x`
//! does not reach any use, because it's immediately replaced by the second binding, before any use
//! happens. (A linter could thus flag the statement `x = 1` as likely superfluous.)
//!
//! The first use of `x` has one live binding: the assignment `x = 2`.
//!
//! Things get a bit more complex when we have branches. We will definitely take either the `if` or
//! the `else` branch. Thus, the second use of `x` has two live bindings: `x = 3` and `x = 4`. The
//! `x = 2` assignment is no longer visible, because it must be replaced by either `x = 3` or `x =
//! 4`, no matter which branch was taken. We don't know which branch was taken, so we must consider
//! both bindings as live, which means eventually we would (in type inference) look at these two
//! bindings and infer a type of `Literal[3, 4]` -- the union of `Literal[3]` and `Literal[4]` --
//! for the second use of `x`.
//!
//! So that's one question our use-def map needs to answer: given a specific use of a symbol, which
//! binding(s) can reach that use. In [`AstIds`](crate::semantic_index::ast_ids::AstIds) we number
//! all uses (that means a `Name` node with `Load` context) so we have a `ScopedUseId` to
//! efficiently represent each use.
//!
//! We also need to know, for a given definition of a symbol, what type narrowing constraints apply
//! to it. For instance, in this code sample:
//!
//! ```python
//! x = 1 if flag else None
//! if x is not None:
//!     use(x)
//! ```
//!
//! At the use of `x`, the live binding of `x` is `1 if flag else None`, which would infer as the
//! type `Literal[1] | None`. But the constraint `x is not None` dominates this use, which means we
//! can rule out the possibility that `x` is `None` here, which should give us the type
//! `Literal[1]` for this use.
//!
//! For declared types, we need to be able to answer the question "given a binding to a symbol,
//! which declarations of that symbol can reach the binding?" This allows us to emit a diagnostic
//! if the binding is attempting to bind a value of a type that is not assignable to the declared
//! type for that symbol, at that point in control flow.
//!
//! We also need to know, given a declaration of a symbol, what the inferred type of that symbol is
//! at that point. This allows us to emit a diagnostic in a case like `x = "foo"; x: int`. The
//! binding `x = "foo"` occurs before the declaration `x: int`, so according to our
//! control-flow-sensitive interpretation of declarations, the assignment is not an error. But the
//! declaration is an error, since it would violate the "inferred type must be assignable to
//! declared type" rule.
//!
//! Another case we need to handle is when a symbol is referenced from a different scope (for
//! example, an import or a nonlocal reference). We call this "public" use of a symbol. For public
//! use of a symbol, we prefer the declared type, if there are any declarations of that symbol; if
//! not, we fall back to the inferred type. So we also need to know which declarations and bindings
//! can reach the end of the scope.
//!
//! Technically, public use of a symbol could occur from any point in control flow of the scope
//! where the symbol is defined (via inline imports and import cycles, in the case of an import, or
//! via a function call partway through the local scope that ends up using a symbol from the scope
//! via a global or nonlocal reference.) But modeling this fully accurately requires whole-program
//! analysis that isn't tractable for an efficient analysis, since it means a given symbol could
//! have a different type every place it's referenced throughout the program, depending on the
//! shape of arbitrarily-sized call/import graphs. So we follow other Python type checkers in
//! making the simplifying assumption that usually the scope will finish execution before its
//! symbols are made visible to other scopes; for instance, most imports will import from a
//! complete module, not a partially-executed module. (We may want to get a little smarter than
//! this in the future for some closures, but for now this is where we start.)
//!
//! The data structure we build to answer these questions is the `UseDefMap`. It has a
//! `bindings_by_use` vector of [`SymbolBindings`] indexed by [`ScopedUseId`], a
//! `declarations_by_binding` vector of [`SymbolDeclarations`] indexed by [`ScopedDefinitionId`], a
//! `bindings_by_declaration` vector of [`SymbolBindings`] indexed by [`ScopedDefinitionId`], and
//! `public_bindings` and `public_definitions` vectors indexed by [`ScopedSymbolId`]. The values in
//! each of these vectors are (in principle) a list of live bindings at that use/definition, or at
//! the end of the scope for that symbol, with a list of the dominating constraints for each
//! binding.
//!
//! In order to avoid vectors-of-vectors-of-vectors and all the allocations that would entail, we
//! don't actually store these "list of visible definitions" as a vector of [`Definition`].
//! Instead, [`SymbolBindings`] and [`SymbolDeclarations`] are structs which use bit-sets to track
//! definitions (and constraints, in the case of bindings) in terms of [`ScopedDefinitionId`] and
//! [`ScopedConstraintId`], which are indices into the `all_definitions` and `all_constraints`
//! indexvecs in the [`UseDefMap`].
//!
//! There is another special kind of possible "definition" for a symbol: there might be a path from
//! the scope entry to a given use in which the symbol is never bound.
//!
//! The simplest way to model "unbound" would be as a "binding" itself: the initial "binding" for
//! each symbol in a scope. But actually modeling it this way would unnecessarily increase the
//! number of [`Definition`]s that Salsa must track. Since "unbound" is special in that all symbols
//! share it, and it doesn't have any additional per-symbol state, and constraints are irrelevant
//! to it, we can represent it more efficiently: we use the `may_be_unbound` boolean on the
//! [`SymbolBindings`] struct. If this flag is `true` for a use of a symbol, it means the symbol
//! has a path to the use in which it is never bound. If this flag is `false`, it means we've
//! eliminated the possibility of unbound: every control flow path to the use includes a binding
//! for this symbol.
//!
//! To build a [`UseDefMap`], the [`UseDefMapBuilder`] is notified of each new use, definition, and
//! constraint as they are encountered by the
//! [`SemanticIndexBuilder`](crate::semantic_index::builder::SemanticIndexBuilder) AST visit. For
//! each symbol, the builder tracks the `SymbolState` (`SymbolBindings` and `SymbolDeclarations`)
//! for that symbol. When we hit a use or definition of a symbol, we record the necessary parts of
//! the current state for that symbol that we need for that use or definition. When we reach the
//! end of the scope, it records the state for each symbol as the public definitions of that
//! symbol.
//!
//! Let's walk through the above example. Initially we record for `x` that it has no bindings, and
//! may be unbound. When we see `x = 1`, we record that as the sole live binding of `x`, and flip
//! `may_be_unbound` to `false`. Then we see `x = 2`, and we replace `x = 1` as the sole live
//! binding of `x`. When we get to `y = x`, we record that the live bindings for that use of `x`
//! are just the `x = 2` definition.
//!
//! Then we hit the `if` branch. We visit the `test` node (`flag` in this case), since that will
//! happen regardless. Then we take a pre-branch snapshot of the current state for all symbols,
//! which we'll need later. Then we record `flag` as a possible constraint on the current binding
//! (`x = 2`), and go ahead and visit the `if` body. When we see `x = 3`, it replaces `x = 2`
//! (constrained by `flag`) as the sole live binding of `x`. At the end of the `if` body, we take
//! another snapshot of the current symbol state; we'll call this the post-if-body snapshot.
//!
//! Now we need to visit the `else` clause. The conditions when entering the `else` clause should
//! be the pre-if conditions; if we are entering the `else` clause, we know that the `if` test
//! failed and we didn't execute the `if` body. So we first reset the builder to the pre-if state,
//! using the snapshot we took previously (meaning we now have `x = 2` as the sole binding for `x`
//! again), then visit the `else` clause, where `x = 4` replaces `x = 2` as the sole live binding
//! of `x`.
//!
//! Now we reach the end of the if/else, and want to visit the following code. The state here needs
//! to reflect that we might have gone through the `if` branch, or we might have gone through the
//! `else` branch, and we don't know which. So we need to "merge" our current builder state
//! (reflecting the end-of-else state, with `x = 4` as the only live binding) with our post-if-body
//! snapshot (which has `x = 3` as the only live binding). The result of this merge is that we now
//! have two live bindings of `x`: `x = 3` and `x = 4`.
//!
//! The [`UseDefMapBuilder`] itself just exposes methods for taking a snapshot, resetting to a
//! snapshot, and merging a snapshot into the current state. The logic using these methods lives in
//! [`SemanticIndexBuilder`](crate::semantic_index::builder::SemanticIndexBuilder), e.g. where it
//! visits a `StmtIf` node.
use self::symbol_state::{
    BindingIdWithConstraintsIterator, ConstraintIdIterator, DeclarationIdIterator,
    ScopedConstraintId, ScopedDefinitionId, SymbolBindings, SymbolDeclarations, SymbolState,
};
use crate::semantic_index::ast_ids::ScopedUseId;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::symbol::ScopedSymbolId;
use ruff_index::IndexVec;
use rustc_hash::FxHashMap;

use super::constraint::Constraint;

mod bitset;
mod symbol_state;

/// Applicable definitions and constraints for every use of a name.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UseDefMap<'db> {
    /// Array of [`Definition`] in this scope.
    all_definitions: IndexVec<ScopedDefinitionId, Definition<'db>>,

    /// Array of [`Constraint`] in this scope.
    all_constraints: IndexVec<ScopedConstraintId, Constraint<'db>>,

    /// [`SymbolBindings`] reaching a [`ScopedUseId`].
    bindings_by_use: IndexVec<ScopedUseId, SymbolBindings>,

    /// [`SymbolBindings`] or [`SymbolDeclarations`] reaching a given [`Definition`].
    ///
    /// If the definition is a binding (only) -- `x = 1` for example -- then we need
    /// [`SymbolDeclarations`] to know whether this binding is permitted by the live declarations.
    ///
    /// If the definition is a declaration (only) -- `x: int` for example -- then we need
    /// [`SymbolBindings`] to know whether this declaration is consistent with the previously
    /// inferred type.
    ///
    /// If the definition is both a declaration and a binding -- `x: int = 1` for example -- then
    /// we don't actually need anything here, all we'll need to validate is that our own RHS is a
    /// valid assignment to our own annotation.
    definitions_by_definition: FxHashMap<Definition<'db>, SymbolDefinitions>,

    /// [`SymbolState`] visible at end of scope for each symbol.
    public_symbols: IndexVec<ScopedSymbolId, SymbolState>,
}

impl<'db> UseDefMap<'db> {
    pub(crate) fn bindings_at_use(
        &self,
        use_id: ScopedUseId,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        self.bindings_iterator(&self.bindings_by_use[use_id])
    }

    pub(crate) fn use_may_be_unbound(&self, use_id: ScopedUseId) -> bool {
        self.bindings_by_use[use_id].may_be_unbound()
    }

    pub(crate) fn public_bindings(
        &self,
        symbol: ScopedSymbolId,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        self.bindings_iterator(self.public_symbols[symbol].bindings())
    }

    pub(crate) fn public_may_be_unbound(&self, symbol: ScopedSymbolId) -> bool {
        self.public_symbols[symbol].may_be_unbound()
    }

    pub(crate) fn bindings_at_declaration(
        &self,
        declaration: Definition<'db>,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        if let SymbolDefinitions::Bindings(bindings) = &self.definitions_by_definition[&declaration]
        {
            self.bindings_iterator(bindings)
        } else {
            unreachable!("Declaration has non-Bindings in definitions_by_definition");
        }
    }

    pub(crate) fn declarations_at_binding(
        &self,
        binding: Definition<'db>,
    ) -> DeclarationsIterator<'_, 'db> {
        if let SymbolDefinitions::Declarations(declarations) =
            &self.definitions_by_definition[&binding]
        {
            self.declarations_iterator(declarations)
        } else {
            unreachable!("Binding has non-Declarations in definitions_by_definition");
        }
    }

    pub(crate) fn public_declarations(
        &self,
        symbol: ScopedSymbolId,
    ) -> DeclarationsIterator<'_, 'db> {
        let declarations = self.public_symbols[symbol].declarations();
        self.declarations_iterator(declarations)
    }

    pub(crate) fn has_public_declarations(&self, symbol: ScopedSymbolId) -> bool {
        !self.public_symbols[symbol].declarations().is_empty()
    }

    fn bindings_iterator<'a>(
        &'a self,
        bindings: &'a SymbolBindings,
    ) -> BindingWithConstraintsIterator<'a, 'db> {
        BindingWithConstraintsIterator {
            all_definitions: &self.all_definitions,
            all_constraints: &self.all_constraints,
            inner: bindings.iter(),
        }
    }

    fn declarations_iterator<'a>(
        &'a self,
        declarations: &'a SymbolDeclarations,
    ) -> DeclarationsIterator<'a, 'db> {
        DeclarationsIterator {
            all_definitions: &self.all_definitions,
            inner: declarations.iter(),
            may_be_undeclared: declarations.may_be_undeclared(),
        }
    }
}

/// Either live bindings or live declarations for a symbol.
#[derive(Debug, PartialEq, Eq)]
enum SymbolDefinitions {
    Bindings(SymbolBindings),
    Declarations(SymbolDeclarations),
}

#[derive(Debug)]
pub(crate) struct BindingWithConstraintsIterator<'map, 'db> {
    all_definitions: &'map IndexVec<ScopedDefinitionId, Definition<'db>>,
    all_constraints: &'map IndexVec<ScopedConstraintId, Constraint<'db>>,
    inner: BindingIdWithConstraintsIterator<'map>,
}

impl<'map, 'db> Iterator for BindingWithConstraintsIterator<'map, 'db> {
    type Item = BindingWithConstraints<'map, 'db>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|def_id_with_constraints| BindingWithConstraints {
                binding: self.all_definitions[def_id_with_constraints.definition],
                constraints: ConstraintsIterator {
                    all_constraints: self.all_constraints,
                    constraint_ids: def_id_with_constraints.constraint_ids,
                },
            })
    }
}

impl std::iter::FusedIterator for BindingWithConstraintsIterator<'_, '_> {}

pub(crate) struct BindingWithConstraints<'map, 'db> {
    pub(crate) binding: Definition<'db>,
    pub(crate) constraints: ConstraintsIterator<'map, 'db>,
}

pub(crate) struct ConstraintsIterator<'map, 'db> {
    all_constraints: &'map IndexVec<ScopedConstraintId, Constraint<'db>>,
    constraint_ids: ConstraintIdIterator<'map>,
}

impl<'map, 'db> Iterator for ConstraintsIterator<'map, 'db> {
    type Item = Constraint<'db>;

    fn next(&mut self) -> Option<Self::Item> {
        self.constraint_ids
            .next()
            .map(|constraint_id| self.all_constraints[constraint_id])
    }
}

impl std::iter::FusedIterator for ConstraintsIterator<'_, '_> {}

pub(crate) struct DeclarationsIterator<'map, 'db> {
    all_definitions: &'map IndexVec<ScopedDefinitionId, Definition<'db>>,
    inner: DeclarationIdIterator<'map>,
    may_be_undeclared: bool,
}

impl DeclarationsIterator<'_, '_> {
    pub(crate) fn may_be_undeclared(&self) -> bool {
        self.may_be_undeclared
    }
}

impl<'map, 'db> Iterator for DeclarationsIterator<'map, 'db> {
    type Item = Definition<'db>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|def_id| self.all_definitions[def_id])
    }
}

impl std::iter::FusedIterator for DeclarationsIterator<'_, '_> {}

/// A snapshot of the definitions and constraints state at a particular point in control flow.
#[derive(Clone, Debug)]
pub(super) struct FlowSnapshot {
    symbol_states: IndexVec<ScopedSymbolId, SymbolState>,
}

#[derive(Debug, Default)]
pub(super) struct UseDefMapBuilder<'db> {
    /// Append-only array of [`Definition`].
    all_definitions: IndexVec<ScopedDefinitionId, Definition<'db>>,

    /// Append-only array of [`Constraint`].
    all_constraints: IndexVec<ScopedConstraintId, Constraint<'db>>,

    /// Live bindings at each so-far-recorded use.
    bindings_by_use: IndexVec<ScopedUseId, SymbolBindings>,

    /// Live bindings or declarations for each so-far-recorded definition.
    definitions_by_definition: FxHashMap<Definition<'db>, SymbolDefinitions>,

    /// Currently live bindings and declarations for each symbol.
    symbol_states: IndexVec<ScopedSymbolId, SymbolState>,
}

impl<'db> UseDefMapBuilder<'db> {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn add_symbol(&mut self, symbol: ScopedSymbolId) {
        let new_symbol = self.symbol_states.push(SymbolState::undefined());
        debug_assert_eq!(symbol, new_symbol);
    }

    pub(super) fn record_binding(&mut self, symbol: ScopedSymbolId, binding: Definition<'db>) {
        let def_id = self.all_definitions.push(binding);
        let symbol_state = &mut self.symbol_states[symbol];
        self.definitions_by_definition.insert(
            binding,
            SymbolDefinitions::Declarations(symbol_state.declarations().clone()),
        );
        symbol_state.record_binding(def_id);
    }

    pub(super) fn record_constraint(&mut self, constraint: Constraint<'db>) {
        let constraint_id = self.all_constraints.push(constraint);
        for state in &mut self.symbol_states {
            state.record_constraint(constraint_id);
        }
    }

    pub(super) fn record_declaration(
        &mut self,
        symbol: ScopedSymbolId,
        declaration: Definition<'db>,
    ) {
        let def_id = self.all_definitions.push(declaration);
        let symbol_state = &mut self.symbol_states[symbol];
        self.definitions_by_definition.insert(
            declaration,
            SymbolDefinitions::Bindings(symbol_state.bindings().clone()),
        );
        symbol_state.record_declaration(def_id);
    }

    pub(super) fn record_declaration_and_binding(
        &mut self,
        symbol: ScopedSymbolId,
        definition: Definition<'db>,
    ) {
        // We don't need to store anything in self.definitions_by_definition.
        let def_id = self.all_definitions.push(definition);
        let symbol_state = &mut self.symbol_states[symbol];
        symbol_state.record_declaration(def_id);
        symbol_state.record_binding(def_id);
    }

    pub(super) fn record_use(&mut self, symbol: ScopedSymbolId, use_id: ScopedUseId) {
        // We have a use of a symbol; clone the current bindings for that symbol, and record them
        // as the live bindings for this use.
        let new_use = self
            .bindings_by_use
            .push(self.symbol_states[symbol].bindings().clone());
        debug_assert_eq!(use_id, new_use);
    }

    /// Take a snapshot of the current visible-symbols state.
    pub(super) fn snapshot(&self) -> FlowSnapshot {
        FlowSnapshot {
            symbol_states: self.symbol_states.clone(),
        }
    }

    /// Restore the current builder symbols state to the given snapshot.
    pub(super) fn restore(&mut self, snapshot: FlowSnapshot) {
        // We never remove symbols from `symbol_states` (it's an IndexVec, and the symbol
        // IDs must line up), so the current number of known symbols must always be equal to or
        // greater than the number of known symbols in a previously-taken snapshot.
        let num_symbols = self.symbol_states.len();
        debug_assert!(num_symbols >= snapshot.symbol_states.len());

        // Restore the current visible-definitions state to the given snapshot.
        self.symbol_states = snapshot.symbol_states;

        // If the snapshot we are restoring is missing some symbols we've recorded since, we need
        // to fill them in so the symbol IDs continue to line up. Since they don't exist in the
        // snapshot, the correct state to fill them in with is "undefined".
        self.symbol_states
            .resize(num_symbols, SymbolState::undefined());
    }

    /// Merge the given snapshot into the current state, reflecting that we might have taken either
    /// path to get here. The new state for each symbol should include definitions from both the
    /// prior state and the snapshot.
    pub(super) fn merge(&mut self, snapshot: FlowSnapshot) {
        // We never remove symbols from `symbol_states` (it's an IndexVec, and the symbol
        // IDs must line up), so the current number of known symbols must always be equal to or
        // greater than the number of known symbols in a previously-taken snapshot.
        debug_assert!(self.symbol_states.len() >= snapshot.symbol_states.len());

        let mut snapshot_definitions_iter = snapshot.symbol_states.into_iter();
        for current in &mut self.symbol_states {
            if let Some(snapshot) = snapshot_definitions_iter.next() {
                current.merge(snapshot);
            } else {
                // Symbol not present in snapshot, so it's unbound/undeclared from that path.
                current.set_may_be_unbound();
                current.set_may_be_undeclared();
            }
        }
    }

    pub(super) fn finish(mut self) -> UseDefMap<'db> {
        self.all_definitions.shrink_to_fit();
        self.all_constraints.shrink_to_fit();
        self.symbol_states.shrink_to_fit();
        self.bindings_by_use.shrink_to_fit();
        self.definitions_by_definition.shrink_to_fit();

        UseDefMap {
            all_definitions: self.all_definitions,
            all_constraints: self.all_constraints,
            bindings_by_use: self.bindings_by_use,
            public_symbols: self.symbol_states,
            definitions_by_definition: self.definitions_by_definition,
        }
    }
}
