use crate::semantic_index::ast_ids::ScopedUseId;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::symbol::ScopedSymbolId;
use ruff_index::IndexVec;
use std::ops::Range;

/// All definitions that can reach a given use of a name.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UseDefMap<'db> {
    // TODO store constraints with definitions for type narrowing
    all_definitions: Vec<Definition<'db>>,

    /// Definitions that can reach a [`ScopedUseId`].
    definitions_by_use: IndexVec<ScopedUseId, Definitions>,

    /// Definitions of a symbol visible to other scopes.
    public_definitions: IndexVec<ScopedSymbolId, Definitions>,
}

impl<'db> UseDefMap<'db> {
    pub(crate) fn use_definitions(&self, use_id: ScopedUseId) -> &[Definition<'db>] {
        &self.all_definitions[self.definitions_by_use[use_id].definitions.clone()]
    }

    pub(crate) fn use_may_be_unbound(&self, use_id: ScopedUseId) -> bool {
        self.definitions_by_use[use_id].may_be_unbound
    }

    pub(crate) fn public_definitions(&self, symbol: ScopedSymbolId) -> &[Definition<'db>] {
        &self.all_definitions[self.public_definitions[symbol].definitions.clone()]
    }

    pub(crate) fn public_may_be_unbound(&self, symbol: ScopedSymbolId) -> bool {
        self.public_definitions[symbol].may_be_unbound
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Definitions {
    definitions: Range<usize>,
    may_be_unbound: bool,
}

impl Default for Definitions {
    fn default() -> Self {
        Self {
            definitions: 0..0,
            may_be_unbound: true,
        }
    }
}

#[derive(Debug)]
pub(super) struct FlowSnapshot {
    definitions_by_symbol: IndexVec<ScopedSymbolId, Definitions>,
}

pub(super) struct UseDefMapBuilder<'db> {
    all_definitions: Vec<Definition<'db>>,

    definitions_by_use: IndexVec<ScopedUseId, Definitions>,

    // builder state: currently visible definitions for each symbol
    definitions_by_symbol: IndexVec<ScopedSymbolId, Definitions>,
}

impl<'db> UseDefMapBuilder<'db> {
    pub(super) fn new() -> Self {
        Self {
            all_definitions: Vec::new(),
            definitions_by_use: IndexVec::new(),
            definitions_by_symbol: IndexVec::new(),
        }
    }

    pub(super) fn add_symbol(&mut self, symbol: ScopedSymbolId) {
        let new_symbol = self.definitions_by_symbol.push(Definitions::default());
        debug_assert_eq!(symbol, new_symbol);
    }

    pub(super) fn record_def(&mut self, symbol: ScopedSymbolId, definition: Definition<'db>) {
        let def_idx = self.all_definitions.len();
        self.all_definitions.push(definition);
        self.definitions_by_symbol[symbol] = Definitions {
            #[allow(clippy::range_plus_one)]
            definitions: def_idx..(def_idx + 1),
            may_be_unbound: false,
        };
    }

    pub(super) fn record_use(&mut self, symbol: ScopedSymbolId, use_id: ScopedUseId) {
        let new_use = self
            .definitions_by_use
            .push(self.definitions_by_symbol[symbol].clone());
        debug_assert_eq!(use_id, new_use);
    }

    pub(super) fn snapshot(&self) -> FlowSnapshot {
        FlowSnapshot {
            definitions_by_symbol: self.definitions_by_symbol.clone(),
        }
    }

    pub(super) fn set(&mut self, state: &FlowSnapshot) {
        let num_symbols = self.definitions_by_symbol.len();
        self.definitions_by_symbol = state.definitions_by_symbol.clone();
        self.definitions_by_symbol
            .resize(num_symbols, Definitions::default());
    }

    pub(super) fn merge(&mut self, state: &FlowSnapshot) {
        for (symbol_id, to_merge) in state.definitions_by_symbol.iter_enumerated() {
            let current = self.definitions_by_symbol.get_mut(symbol_id).unwrap();
            // if the symbol can be unbound in either predecessor, it can be unbound
            current.may_be_unbound |= to_merge.may_be_unbound;
            // merge the definition ranges
            if current.definitions == to_merge.definitions {
                // ranges already identical, nothing to do!
            } else if current.definitions.end == to_merge.definitions.start {
                // ranges adjacent (current first), just merge them
                current.definitions = (current.definitions.start)..(to_merge.definitions.end);
            } else if current.definitions.start == to_merge.definitions.end {
                // ranges adjacent (to_merge first), just merge them
                current.definitions = (to_merge.definitions.start)..(current.definitions.end);
            } else if current.definitions.end == self.all_definitions.len() {
                // ranges not adjacent but current is at end, copy only to_merge
                self.all_definitions
                    .extend_from_within(to_merge.definitions.clone());
                current.definitions.end = self.all_definitions.len();
            } else if to_merge.definitions.end == self.all_definitions.len() {
                // ranges not adjacent but to_merge is at end, copy only current
                self.all_definitions
                    .extend_from_within(current.definitions.clone());
                current.definitions.start = to_merge.definitions.start;
                current.definitions.end = self.all_definitions.len();
            } else {
                // ranges not adjacent and neither at end, must copy both
                let start = self.all_definitions.len();
                self.all_definitions
                    .extend_from_within(current.definitions.clone());
                self.all_definitions
                    .extend_from_within(to_merge.definitions.clone());
                current.definitions.start = start;
                current.definitions.end = self.all_definitions.len();
            }
        }
    }

    pub(super) fn finish(mut self) -> UseDefMap<'db> {
        self.all_definitions.shrink_to_fit();
        self.definitions_by_symbol.shrink_to_fit();
        self.definitions_by_use.shrink_to_fit();

        UseDefMap {
            all_definitions: self.all_definitions,
            definitions_by_use: self.definitions_by_use,
            public_definitions: self.definitions_by_symbol,
        }
    }
}
