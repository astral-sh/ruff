//! Build a map from each use of a symbol to the definitions visible from that use.
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
//! The other case we need to handle is when a symbol is referenced from a different scope (the
//! most obvious example of this is an import). We call this "public" use of a symbol. So the other
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
//! visible at the end of the scope.
//!
//! The data structure we build to answer these two questions is the `UseDefMap`. It has a
//! `definitions_by_use` vector indexed by [`ScopedUseId`] and a `public_definitions` map
//! indexed by [`ScopedSymbolId`]. The values in each are the visible definition of a symbol at
//! that use, or at the end of the scope.
//!
//! Rather than have multiple definitions, we use a Phi definition at control flow join points to
//! merge the visible definition in each path. This means at any given point we always have exactly
//! one definition for a symbol. (This is analogous to static-single-assignment, or SSA, form, and
//! in fact we use the algorithm from [Simple and efficient construction of static single
//! assignment form](https://dl.acm.org/doi/10.1007/978-3-642-37051-9_6) here.)
use crate::semantic_index::ast_ids::ScopedUseId;
use crate::semantic_index::definition::{Definition, DefinitionKind, ScopedPhiId};
use crate::semantic_index::symbol::{FileScopeId, ScopedSymbolId};
use crate::Db;
use ruff_db::files::File;
use ruff_index::{newtype_index, IndexVec};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::{smallvec, SmallVec};

/// Number of basic block predecessors we store inline.
const PREDECESSORS: usize = 2;

/// Input operands (definitions) for a Phi definition. None means not defined.
// TODO would like to use SmallVec here but can't due to lifetime invariance issue.
type PhiOperands<'db> = Vec<Option<Definition<'db>>>;

/// Definition for each use of a name.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UseDefMap<'db> {
    // TODO store constraints with definitions for type narrowing
    /// Definition that reaches each [`ScopedUseId`].
    definitions_by_use: IndexVec<ScopedUseId, Option<Definition<'db>>>,

    /// Definition of each symbol visible at end of scope.
    ///
    /// Sparse, because it only includes symbols defined in the scope.
    public_definitions: FxHashMap<ScopedSymbolId, Definition<'db>>,

    /// Operands for each Phi definition in this scope.
    phi_operands: IndexVec<ScopedPhiId, PhiOperands<'db>>,
}

impl<'db> UseDefMap<'db> {
    /// Return the dominating definition for a given use of a name; None means not-defined.
    pub(crate) fn definition_for_use(&self, use_id: ScopedUseId) -> Option<Definition<'db>> {
        self.definitions_by_use[use_id]
    }

    /// Return the definition visible at end of scope for a symbol.
    ///
    /// Return None if the symbol is never defined in the scope.
    pub(crate) fn public_definition(&self, symbol_id: ScopedSymbolId) -> Option<Definition<'db>> {
        self.public_definitions.get(&symbol_id).copied()
    }

    /// Return the operands for a Phi in this scope; a None means not-defined.
    pub(crate) fn phi_operands<'s>(&'s self, phi_id: ScopedPhiId) -> &'s [Option<Definition<'db>>] {
        self.phi_operands[phi_id].as_slice()
    }
}

type PredecessorBlocks = SmallVec<[BasicBlockId; PREDECESSORS]>;

/// A basic block is a linear region of code (no branches.)
#[newtype_index]
pub(super) struct BasicBlockId;

pub(super) struct UseDefMapBuilder<'db> {
    db: &'db dyn Db,
    file: File,
    file_scope: FileScopeId,

    /// Predecessor blocks for each basic block.
    ///
    /// Entry block has none, all other blocks have at least one, blocks that join control flow can
    /// have two or more.
    predecessors: IndexVec<BasicBlockId, PredecessorBlocks>,

    /// The definition of each symbol which dominates each basic block.
    ///
    /// No entry means "lazily unfilled"; we haven't had to query for it yet, and we may never have
    /// to, if the symbol isn't used in this block or any successor block.
    ///
    /// Each block has an [`FxHashMap`] of symbols instead of an [`IndexVec`] because it is lazy
    /// and potentially sparse; it will only include a definition for a symbol that is actually
    /// used in that block or a successor. An [`IndexVec`] would have to be eagerly filled with
    /// placeholders.
    definitions_per_block:
        IndexVec<BasicBlockId, FxHashMap<ScopedSymbolId, Option<Definition<'db>>>>,

    /// Incomplete Phi definitions in each block.
    ///
    /// An incomplete Phi is used when we don't know, while processing a block's body, what new
    /// predecessors it may later gain (that is, backward jumps.)
    ///
    /// Sparse, because relative few blocks (just loop headers) will have any incomplete Phis.
    incomplete_phis: FxHashMap<BasicBlockId, Vec<Definition<'db>>>,

    /// Operands for each Phi definition in this scope.
    phi_operands: IndexVec<ScopedPhiId, PhiOperands<'db>>,

    /// Are this block's predecessors fully populated?
    ///
    /// If not, it isn't safe to recurse to predecessors yet; we might miss a predecessor block.
    sealed_blocks: IndexVec<BasicBlockId, bool>,

    /// Definition for each so-far-recorded use.
    definitions_by_use: IndexVec<ScopedUseId, Option<Definition<'db>>>,

    /// All symbols defined in this scope.
    defined_symbols: FxHashSet<ScopedSymbolId>,
}

impl<'db> UseDefMapBuilder<'db> {
    pub(super) fn new(db: &'db dyn Db, file: File, file_scope: FileScopeId) -> Self {
        let mut new = Self {
            db,
            file,
            file_scope,
            predecessors: IndexVec::new(),
            definitions_per_block: IndexVec::new(),
            incomplete_phis: FxHashMap::default(),
            sealed_blocks: IndexVec::new(),
            definitions_by_use: IndexVec::new(),
            phi_operands: IndexVec::new(),
            defined_symbols: FxHashSet::default(),
        };

        // create the entry basic block
        new.predecessors.push(PredecessorBlocks::default());
        new.definitions_per_block.push(FxHashMap::default());
        new.sealed_blocks.push(true);

        new
    }

    /// Record a definition for a symbol.
    pub(super) fn record_definition(
        &mut self,
        symbol_id: ScopedSymbolId,
        definition: Definition<'db>,
    ) {
        self.memoize(self.current_block_id(), symbol_id, Some(definition));
        self.defined_symbols.insert(symbol_id);
    }

    /// Record a use of a symbol.
    pub(super) fn record_use(&mut self, symbol_id: ScopedSymbolId, use_id: ScopedUseId) {
        let definition_id = self.lookup(symbol_id);
        let new_use = self.definitions_by_use.push(definition_id);
        debug_assert_eq!(use_id, new_use);
    }

    /// Get the id of the current basic block.
    pub(super) fn current_block_id(&self) -> BasicBlockId {
        BasicBlockId::from(self.definitions_per_block.len() - 1)
    }

    /// Push a new basic block, with given block as predecessor.
    pub(super) fn new_block_from(&mut self, block_id: BasicBlockId, sealed: bool) {
        self.new_block_with_predecessors(smallvec![block_id], sealed);
    }

    /// Push a new basic block, with current block as predecessor; return the current block's ID.
    pub(super) fn next_block(&mut self, sealed: bool) -> BasicBlockId {
        let current_block_id = self.current_block_id();
        self.new_block_from(current_block_id, sealed);
        current_block_id
    }

    /// Add a predecessor to the current block.
    pub(super) fn merge_block(&mut self, new_predecessor: BasicBlockId) {
        let block_id = self.current_block_id();
        debug_assert!(!self.sealed_blocks[block_id]);
        self.predecessors[block_id].push(new_predecessor);
    }

    /// Add predecessors to the current block.
    pub(super) fn merge_blocks(&mut self, new_predecessors: Vec<BasicBlockId>) {
        let block_id = self.current_block_id();
        debug_assert!(!self.sealed_blocks[block_id]);
        self.predecessors[block_id].extend(new_predecessors);
    }

    /// Mark the current block as sealed; it cannot have any more predecessors added.
    pub(super) fn seal_current_block(&mut self) {
        self.seal_block(self.current_block_id());
    }

    /// Mark a block as sealed; it cannot have any more predecessors added.
    pub(super) fn seal_block(&mut self, block_id: BasicBlockId) {
        debug_assert!(!self.sealed_blocks[block_id]);
        if let Some(phis) = self.incomplete_phis.get(&block_id) {
            for phi in phis.clone() {
                self.add_phi_operands(block_id, phi);
            }
            self.incomplete_phis.remove(&block_id);
        }
        self.sealed_blocks[block_id] = true;
    }

    pub(super) fn finish(mut self) -> UseDefMap<'db> {
        debug_assert!(self.incomplete_phis.is_empty());
        debug_assert!(self.sealed_blocks.iter().all(|&b| b));
        self.definitions_by_use.shrink_to_fit();
        self.phi_operands.shrink_to_fit();

        let mut public_definitions: FxHashMap<ScopedSymbolId, Definition<'db>> =
            FxHashMap::default();

        for symbol_id in self.defined_symbols.clone() {
            // SAFETY: We are only looking up defined symbols here, can't get None.
            public_definitions.insert(symbol_id, self.lookup(symbol_id).unwrap());
        }

        UseDefMap {
            definitions_by_use: self.definitions_by_use,
            public_definitions,
            phi_operands: self.phi_operands,
        }
    }

    /// Push a new basic block (with given predecessors) and return its ID.
    fn new_block_with_predecessors(
        &mut self,
        predecessors: PredecessorBlocks,
        sealed: bool,
    ) -> BasicBlockId {
        let new_block_id = self.predecessors.push(predecessors);
        self.definitions_per_block.push(FxHashMap::default());
        self.sealed_blocks.push(sealed);

        new_block_id
    }

    /// Look up the dominating definition for a symbol in the current block.
    ///
    /// If there isn't a local definition, recursively look up the symbol in predecessor blocks,
    /// memoizing the found symbol in each block.
    fn lookup(&mut self, symbol_id: ScopedSymbolId) -> Option<Definition<'db>> {
        self.lookup_impl(self.current_block_id(), symbol_id)
    }

    fn lookup_impl(
        &mut self,
        block_id: BasicBlockId,
        symbol_id: ScopedSymbolId,
    ) -> Option<Definition<'db>> {
        if let Some(local) = self.definitions_per_block[block_id].get(&symbol_id) {
            return *local;
        }
        if !self.sealed_blocks[block_id] {
            // we may still be missing predecessors; insert an incomplete Phi.
            let definition = self.create_incomplete_phi(block_id, symbol_id);
            self.incomplete_phis
                .entry(block_id)
                .or_default()
                .push(definition);
            return Some(definition);
        }
        match self.predecessors[block_id].as_slice() {
            // entry block, no definition found: return None
            [] => None,
            // single predecessor, recurse
            &[single_predecessor_id] => {
                let definition = self.lookup_impl(single_predecessor_id, symbol_id);
                self.memoize(block_id, symbol_id, definition);
                definition
            }
            // multiple predecessors: create and memoize an incomplete Phi to break cycles, then
            // recurse into predecessors and fill the Phi operands.
            _ => {
                let phi = self.create_incomplete_phi(block_id, symbol_id);
                self.add_phi_operands(block_id, phi);
                Some(phi)
            }
        }
    }

    /// Recurse into predecessors to add operands for an incomplete Phi.
    fn add_phi_operands(&mut self, block_id: BasicBlockId, phi: Definition<'db>) {
        let predecessors: PredecessorBlocks = self.predecessors[block_id].clone();
        let operands: PhiOperands = predecessors
            .iter()
            .map(|pred_id| self.lookup_impl(*pred_id, phi.symbol(self.db)))
            .collect();
        let DefinitionKind::Phi(phi_id) = phi.kind(self.db) else {
            unreachable!("add_phi_operands called with non-Phi");
        };
        self.phi_operands[*phi_id] = operands;
    }

    /// Remember a given definition for a given symbol in the given block.
    fn memoize(
        &mut self,
        block_id: BasicBlockId,
        symbol_id: ScopedSymbolId,
        definition_id: Option<Definition<'db>>,
    ) {
        self.definitions_per_block[block_id].insert(symbol_id, definition_id);
    }

    /// Create an incomplete Phi for the given block and symbol, memoize it, and return its ID.
    fn create_incomplete_phi(
        &mut self,
        block_id: BasicBlockId,
        symbol_id: ScopedSymbolId,
    ) -> Definition<'db> {
        let phi_id = self.phi_operands.push(vec![]);
        let definition = Definition::new(
            self.db,
            self.file,
            self.file_scope,
            symbol_id,
            DefinitionKind::Phi(phi_id),
            countme::Count::default(),
        );
        self.memoize(block_id, symbol_id, Some(definition));
        definition
    }
}
