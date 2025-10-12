use std::num::NonZeroU32;
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
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct NodeIndex(NonZeroU32);

impl NodeIndex {
    /// A placeholder `NodeIndex`.
    pub const NONE: NodeIndex = NodeIndex(NonZeroU32::new(NodeIndex::_NONE).unwrap());

    // Note that the index `u32::MAX` is reserved for the `NonZeroU32` niche, and
    // this placeholder also reserves the second highest index.
    const _NONE: u32 = u32::MAX - 1;

    /// Returns the index as a `u32`. or `None` for `NodeIndex::NONE`.
    pub fn as_u32(self) -> Option<u32> {
        if self == NodeIndex::NONE {
            None
        } else {
            Some(self.0.get() - 1)
        }
    }
}

impl From<u32> for NodeIndex {
    fn from(value: u32) -> Self {
        match NonZeroU32::new(value + 1).map(NodeIndex) {
            None | Some(NodeIndex::NONE) => panic!("exceeded maximum `NodeIndex`"),
            Some(index) => index,
        }
    }
}

impl std::fmt::Debug for NodeIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if *self == Self::NONE {
            f.debug_tuple("NodeIndex(None)").finish()
        } else {
            f.debug_tuple("NodeIndex").field(&self.0).finish()
        }
    }
}

/// A unique index for a node within an AST.
///
/// This type is interiorly mutable to allow assigning node indices
/// on-demand after parsing.
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct AtomicNodeIndex(AtomicU32);

#[allow(clippy::declare_interior_mutable_const)]
impl AtomicNodeIndex {
    /// A placeholder `AtomicNodeIndex`.
    pub const NONE: AtomicNodeIndex = AtomicNodeIndex(AtomicU32::new(NodeIndex::_NONE));

    /// Load the current value of the `AtomicNodeIndex`.
    pub fn load(&self) -> NodeIndex {
        let index = NonZeroU32::new(self.0.load(Ordering::Relaxed))
            .expect("value stored was a valid `NodeIndex`");

        NodeIndex(index)
    }

    /// Set the value of the `AtomicNodeIndex`.
    pub fn set(&self, index: NodeIndex) {
        self.0.store(index.0.get(), Ordering::Relaxed);
    }
}

impl Default for AtomicNodeIndex {
    fn default() -> Self {
        Self::NONE
    }
}

impl std::fmt::Debug for AtomicNodeIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.load(), f)
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

#[cfg(test)]
mod tests {
    use super::{AtomicNodeIndex, NodeIndex};

    #[test]
    fn test_node_index() {
        let index = AtomicNodeIndex::NONE;

        assert_eq!(index.load(), NodeIndex::NONE);
        assert_eq!(format!("{index:?}"), "NodeIndex(None)");

        index.set(NodeIndex::from(1));
        assert_eq!(index.load(), NodeIndex::from(1));
        assert_eq!(index.load().as_u32(), Some(1));

        let index = NodeIndex::from(0);
        assert_eq!(index.as_u32(), Some(0));

        let index = NodeIndex::NONE;
        assert_eq!(index.as_u32(), None);
    }
}
