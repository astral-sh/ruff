use std::sync::atomic::{AtomicU32, Ordering};

/// An AST node that has an index.
pub trait HasNodeIndex {
    /// Returns the [`AtomicNodeIndex`] for this node.
    fn node_index(&self) -> &AtomicNodeIndex;
}

impl<T> HasNodeIndex for &T
where
    T: HasNodeIndex,
{
    fn node_index(&self) -> &AtomicNodeIndex {
        T::node_index(*self)
    }
}

/// A unique index for a node within an AST.
///
/// This type is interiorly mutable to allow assigning node indices
/// on-demand after parsing.
#[derive(Default)]
pub struct AtomicNodeIndex(AtomicU32);

impl AtomicNodeIndex {
    /// Returns a placeholder `AtomicNodeIndex`.
    pub fn dummy() -> AtomicNodeIndex {
        AtomicNodeIndex(AtomicU32::from(u32::MAX))
    }

    /// Load the current value of the `AtomicNodeIndex`.
    pub fn load(&self) -> NodeIndex {
        NodeIndex(self.0.load(Ordering::Relaxed))
    }

    /// Set the value of the `AtomicNodeIndex`.
    pub fn set(&self, value: u32) {
        self.0.store(value, Ordering::Relaxed);
    }
}

/// A unique index for a node within an AST.
#[derive(PartialEq, Eq, Debug, PartialOrd, Ord, Clone, Copy, Hash)]
pub struct NodeIndex(u32);

impl NodeIndex {
    pub fn as_usize(self) -> usize {
        self.0 as _
    }
}

impl From<u32> for AtomicNodeIndex {
    fn from(value: u32) -> Self {
        AtomicNodeIndex(AtomicU32::from(value))
    }
}

impl std::fmt::Debug for AtomicNodeIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if *self == AtomicNodeIndex::dummy() {
            f.debug_tuple("AtomicNodeIndex").finish_non_exhaustive()
        } else {
            f.debug_tuple("AtomicNodeIndex").field(&self.0).finish()
        }
    }
}

impl std::hash::Hash for AtomicNodeIndex {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.load().hash(state);
    }
}

impl PartialOrd for AtomicNodeIndex {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AtomicNodeIndex {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.load().cmp(&other.load())
    }
}

impl Eq for AtomicNodeIndex {}

impl PartialEq for AtomicNodeIndex {
    fn eq(&self, other: &Self) -> bool {
        self.load() == other.load()
    }
}

impl Clone for AtomicNodeIndex {
    fn clone(&self) -> Self {
        Self(AtomicU32::from(self.0.load(Ordering::Relaxed)))
    }
}
