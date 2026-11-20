use ruff_python_ast::AstNode as _;
use ruff_python_ast::SyntaxNode;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};

/// Used as key into the [`MultiMap`](super::MultiMap) storing the comments per node by
/// [`Comments`](super::Comments).
///
/// Keys identify rows in the tree being formatted. Different occurrences of equal syntax can
/// have different comments.
#[derive(Copy, Clone)]
pub(super) struct NodeRefEqualityKey<'a> {
    node: SyntaxNode<'a>,
}

impl<'a> NodeRefEqualityKey<'a> {
    /// Creates a key for a node reference.
    pub(super) const fn from_ref(node: SyntaxNode<'a>) -> Self {
        Self { node }
    }

    /// Returns the underlying node.
    pub(super) fn node(&self) -> SyntaxNode<'_> {
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
        self.node.is_same_node(other.node)
    }
}

impl Eq for NodeRefEqualityKey<'_> {}

impl Hash for NodeRefEqualityKey<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node.node_id().hash(state);
    }
}

impl<'a> From<SyntaxNode<'a>> for NodeRefEqualityKey<'a> {
    fn from(value: SyntaxNode<'a>) -> Self {
        NodeRefEqualityKey::from_ref(value)
    }
}

#[cfg(test)]
mod tests {
    use super::NodeRefEqualityKey;
    use ruff_python_ast::AstNode as _;
    use ruff_python_parser::parse_module;
    use std::hash::{DefaultHasher, Hash, Hasher};

    fn hash(key: NodeRefEqualityKey<'_>) -> u64 {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn equality() {
        let parsed = parse_module("pass").unwrap();
        let a = NodeRefEqualityKey::from_ref(parsed.suite().at(0).syntax_node());
        let b = NodeRefEqualityKey::from_ref(parsed.suite().at(0).syntax_node());
        assert_eq!(a, b);
        assert_eq!(hash(a), hash(b));
    }

    #[test]
    fn inequality() {
        let parsed = parse_module("pass\npass").unwrap();
        let other = parse_module("pass").unwrap();
        let a = NodeRefEqualityKey::from_ref(parsed.suite().at(0).syntax_node());
        let b = NodeRefEqualityKey::from_ref(parsed.suite().at(1).syntax_node());
        let c = NodeRefEqualityKey::from_ref(other.suite().at(0).syntax_node());
        assert_ne!(a, b);
        assert_ne!(a, c);
    }
}
