/// An AST node that has an index.
pub trait HasNodeIndex {
    /// Returns the [`NodeIndex`] for this node.
    fn node_index(&self) -> NodeIndex;
}

/// A unique index for a node within an AST.
#[derive(Default, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct NodeIndex(u32);

impl NodeIndex {
    #[must_use]
    pub fn next(self) -> NodeIndex {
        NodeIndex(self.0 + 1)
    }

    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }
}

impl From<u32> for NodeIndex {
    fn from(value: u32) -> Self {
        NodeIndex(value)
    }
}
