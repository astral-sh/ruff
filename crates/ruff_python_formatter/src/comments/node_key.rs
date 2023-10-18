use ruff_python_ast::AnyNodeRef;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};

/// Used as key into the [`MultiMap`] storing the comments per node by [`Comments`].
///
/// Implements equality and hashing based on the address of the [`AnyNodeRef`] to get fast and cheap
/// hashing/equality comparison.
#[derive(Copy, Clone)]
pub(super) struct NodeRefEqualityKey<'a> {
    node: AnyNodeRef<'a>,
}

impl<'a> NodeRefEqualityKey<'a> {
    /// Creates a key for a node reference.
    pub(super) const fn from_ref(node: AnyNodeRef<'a>) -> Self {
        Self { node }
    }

    /// Returns the underlying node.
    pub(super) fn node(&self) -> AnyNodeRef {
        self.node
    }
}

impl Debug for NodeRefEqualityKey<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.node.fmt(f)
    }
}

impl PartialEq for NodeRefEqualityKey<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.node.ptr_eq(other.node)
    }
}

impl Eq for NodeRefEqualityKey<'_> {}

impl Hash for NodeRefEqualityKey<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node.as_ptr().hash(state);
    }
}

impl<'a> From<AnyNodeRef<'a>> for NodeRefEqualityKey<'a> {
    fn from(value: AnyNodeRef<'a>) -> Self {
        NodeRefEqualityKey::from_ref(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::comments::node_key::NodeRefEqualityKey;
    use ruff_python_ast::AnyNodeRef;
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
        };

        let boxed = Box::new(continue_statement.clone());

        let ref_a = NodeRefEqualityKey::from_ref(AnyNodeRef::from(&continue_statement));
        let ref_b = NodeRefEqualityKey::from_ref(AnyNodeRef::from(boxed.as_ref()));

        assert_ne!(ref_a, ref_b);
        assert_ne!(hash(ref_a), hash(ref_b));
    }
}
