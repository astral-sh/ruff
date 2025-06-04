/// An AST node that has an index.
pub trait HasNodeIndex {
    /// Returns the [`NodeIndex`] for this node.
    fn node_index(&self) -> NodeIndex;
}

/// A unique index for a node within an AST.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct NodeIndex(u32);

impl NodeIndex {
    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}

impl From<u32> for NodeIndex {
    fn from(value: u32) -> Self {
        NodeIndex(value)
    }
}

impl std::fmt::Debug for NodeIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
