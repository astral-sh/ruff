use ruff_python_ast::{AnyNodeRef, NodeKind};
use ruff_text_size::{Ranged, TextRange};
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::ptr::NonNull;

/// Used as key into the [`MultiMap`](super::MultiMap) storing the comments per node by
/// [`Comments`](super::Comments).
///
/// Implements equality and hashing based on the address of the [`AnyNodeRef`] to get fast and cheap
/// hashing/equality comparison.
#[derive(Copy, Clone)]
pub(super) struct NodeRefEqualityKey {
    pointer: NonNull<()>,
    kind: NodeKind,
    range: TextRange,
}

impl NodeRefEqualityKey {
    /// Creates a key for a node reference.
    pub(super) fn from_ref(node: AnyNodeRef<'_>) -> Self {
        Self {
            pointer: node.as_ptr(),
            kind: node.kind(),
            range: node.range(),
        }
    }

    pub(super) const fn kind(&self) -> NodeKind {
        self.kind
    }

    pub(super) const fn range(&self) -> TextRange {
        self.range
    }
}

impl Debug for NodeRefEqualityKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeRefEqualityKey")
            .field("kind", &self.kind)
            .field("range", &self.range)
            .finish_non_exhaustive()
    }
}

impl PartialEq for NodeRefEqualityKey {
    fn eq(&self, other: &Self) -> bool {
        self.pointer == other.pointer && self.kind == other.kind
    }
}

impl Eq for NodeRefEqualityKey {}

impl Hash for NodeRefEqualityKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.pointer.hash(state);
        self.kind.hash(state);
    }
}

impl From<AnyNodeRef<'_>> for NodeRefEqualityKey {
    fn from(value: AnyNodeRef<'_>) -> Self {
        NodeRefEqualityKey::from_ref(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::comments::node_key::NodeRefEqualityKey;
    use ruff_python_ast::AnyNodeRef;
    use ruff_python_ast::AtomicNodeIndex;
    use ruff_python_ast::StmtContinue;
    use ruff_text_size::TextRange;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn hash(key: NodeRefEqualityKey) -> u64 {
        let mut h = DefaultHasher::default();
        key.hash(&mut h);
        h.finish()
    }

    #[test]
    fn equality() {
        let continue_statement = StmtContinue {
            range: TextRange::default(),
            node_index: AtomicNodeIndex::NONE,
        };

        let ref_a = NodeRefEqualityKey::from_ref(AnyNodeRef::from(&continue_statement));
        let ref_b = NodeRefEqualityKey::from_ref(AnyNodeRef::from(&continue_statement));

        assert_eq!(ref_a, ref_b);
        assert_eq!(hash(ref_a), hash(ref_b));
    }

    #[test]
    fn inequality() {
        let continue_statement = StmtContinue {
            range: TextRange::default(),
            node_index: AtomicNodeIndex::NONE,
        };

        let boxed = Box::new(continue_statement.clone());

        let ref_a = NodeRefEqualityKey::from_ref(AnyNodeRef::from(&continue_statement));
        let ref_b = NodeRefEqualityKey::from_ref(AnyNodeRef::from(boxed.as_ref()));

        assert_ne!(ref_a, ref_b);
        assert_ne!(hash(ref_a), hash(ref_b));
    }
}
