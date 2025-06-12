use std::sync::atomic::{AtomicU32, Ordering};

/// An AST node that has an index.
pub trait HasNodeIndex {
    /// Returns the [`NodeIndex`] for this node.
    fn node_index(&self) -> &NodeIndex;
}

impl<T> HasNodeIndex for &T
where
    T: HasNodeIndex,
{
    fn node_index(&self) -> &NodeIndex {
        T::node_index(*self)
    }
}

/// A unique index for a node within an AST.
///
/// This type is interiorly mutable to allow assigning node indices
/// on-demand after parsing.
#[derive(Default)]
pub struct NodeIndex(AtomicU32);

impl NodeIndex {
    // Returns a placeholder `NodeIndex`.
    pub fn dummy() -> NodeIndex {
        NodeIndex(AtomicU32::from(u32::MAX))
    }

    pub fn store(&self, value: u32) {
        self.0.store(value, Ordering::Relaxed);
    }

    pub fn as_u32(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }
}

impl From<u32> for NodeIndex {
    fn from(value: u32) -> Self {
        NodeIndex(AtomicU32::from(value))
    }
}

impl std::fmt::Debug for NodeIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if *self == NodeIndex::dummy() {
            f.debug_tuple("NodeIndex").field(&"_").finish()
        } else {
            f.debug_tuple("NodeIndex").field(&self.0).finish()
        }
    }
}

impl std::hash::Hash for NodeIndex {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_u32().hash(state);
    }
}

impl PartialOrd for NodeIndex {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NodeIndex {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_u32().cmp(&other.as_u32())
    }
}

impl Eq for NodeIndex {}

impl PartialEq for NodeIndex {
    fn eq(&self, other: &Self) -> bool {
        self.as_u32() == other.as_u32()
    }
}

impl Clone for NodeIndex {
    fn clone(&self) -> Self {
        Self(AtomicU32::from(self.0.load(Ordering::Relaxed)))
    }
}
