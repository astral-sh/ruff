use ruff_python_ast::{AnyNodeRef, Identifier, NodeKind};
use ruff_text_size::{Ranged, TextRange};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(super) enum Kind {
    Node(NodeKind),
    Identifier,
}

/// Compact key for a node for use in a hash map.
///
/// Compares two nodes by their kind and text range.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(super) struct NodeKey {
    kind: Kind,
    range: TextRange,
}

impl NodeKey {
    pub(super) fn from_node<'a, N>(node: N) -> Self
    where
        N: Into<AnyNodeRef<'a>>,
    {
        let node = node.into();
        NodeKey {
            kind: Kind::Node(node.kind()),
            range: node.range(),
        }
    }

    pub(super) fn from_identifier(identifier: &Identifier) -> Self {
        NodeKey {
            kind: Kind::Identifier,
            range: identifier.range(),
        }
    }
}
