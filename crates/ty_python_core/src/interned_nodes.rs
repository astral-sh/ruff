//! Data structures for interning interior nodes.
//!
//! Each graph needs stable IDs for its nodes and a way to reuse an existing ID when the same
//! node is constructed again. The indexed vector owns the nodes; a hash table of IDs provides
//! lookup without storing another copy of each node.

use std::hash::{BuildHasher, Hash};
use std::ops::Index;

use hashbrown::{HashTable, hash_table::Entry};
use ruff_index::{Idx, IndexSlice, IndexVec};
use rustc_hash::FxBuildHasher;

/// An indexed collection of distinct nodes with a reverse table of their IDs.
///
/// The vector owns each node at the index given by its stable ID. The hash table stores only
/// IDs, hashing and comparing them by reading the corresponding nodes from the vector. Both
/// collections are private, and [`Self::intern`] updates them together: every table entry
/// refers to a node in the vector, and an equal node reuses that node's ID. Once the graph is
/// built, [`Self::into_nodes`] returns the node arena and drops the table.
#[derive(Debug)]
pub(crate) struct InternedNodes<I: Idx, N> {
    nodes: IndexVec<I, N>,
    ids: HashTable<I>,
}

impl<I: Idx, N> Default for InternedNodes<I, N> {
    fn default() -> Self {
        Self {
            nodes: IndexVec::default(),
            ids: HashTable::default(),
        }
    }
}

impl<I: Idx, N> InternedNodes<I, N> {
    /// Consume the reverse lookup table and return the arena in its original ID order.
    pub(crate) fn into_nodes(self) -> IndexVec<I, N> {
        self.nodes
    }

    pub(crate) fn as_slice(&self) -> &IndexSlice<I, N> {
        &self.nodes
    }

    pub(crate) const fn len(&self) -> usize {
        self.nodes.raw.len()
    }
}

impl<I: Idx, N: Eq + Hash> InternedNodes<I, N> {
    pub(crate) fn find(&self, node: &N) -> Option<I> {
        self.ids
            .find(FxBuildHasher.hash_one(node), |id| self.nodes[*id].eq(node))
            .copied()
    }

    /// Returns the node ID and whether the node was newly inserted.
    pub(crate) fn intern(&mut self, node: N) -> (I, bool) {
        let nodes = &mut self.nodes;
        match self.ids.entry(
            FxBuildHasher.hash_one(&node),
            |id| nodes[*id].eq(&node),
            |id| FxBuildHasher.hash_one(&nodes[*id]),
        ) {
            Entry::Occupied(entry) => (*entry.get(), false),
            Entry::Vacant(entry) => {
                let id = nodes.push(node);
                entry.insert(id);
                (id, true)
            }
        }
    }
}

impl<I: Idx, N> Index<I> for InternedNodes<I, N> {
    type Output = N;

    fn index(&self, id: I) -> &N {
        &self.nodes[id]
    }
}
