use ruff_python_ast::{HasNodeIndex, NodeIndex, sub_ast_level};

use crate::ast_node_ref::AstNodeRef;
use crate::frozen::FrozenMap;
use crate::rank::RankBitBox;

/// Compact key for a node for use in a hash map.
#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, salsa::Update, get_size2::GetSize,
)]
pub struct NodeKey(NodeIndex);

impl NodeKey {
    pub fn from_node<N>(node: N) -> Self
    where
        N: HasNodeIndex,
    {
        NodeKey(node.node_index().load())
    }

    pub fn from_node_ref<T>(node_ref: &AstNodeRef<T>) -> Self {
        NodeKey(node_ref.index())
    }

    pub(crate) fn index(self) -> NodeIndex {
        self.0
    }
}

/// Compact immutable map keyed by AST node index.
///
/// Top-level node indices are dense, so storing every key repeats information already encoded by
/// its position in the AST. A bitmap records which nodes have values, while prefix ranks map a set
/// bit to its packed value. String annotations use a separate index space and remain in a sorted
/// fallback map because their indices are intentionally sparse.
#[derive(Debug, Eq, PartialEq, salsa::Update, get_size2::GetSize)]
pub(crate) struct NodeIndexMap<V> {
    root_presence: RankBitBox,
    root_values: Box<[V]>,
    sub_ast: FrozenMap<u32, V>,
}

impl<V> NodeIndexMap<V> {
    pub(crate) fn from_entries(entries: impl IntoIterator<Item = (NodeIndex, V)>) -> Self {
        let mut root = Vec::new();
        let mut sub_ast = Vec::new();

        for (index, value) in entries {
            let raw_index = index
                .as_u32()
                .expect("semantic index keys should have assigned node indices");
            if sub_ast_level(raw_index) == 0 {
                root.push((raw_index, value));
            } else {
                sub_ast.push((raw_index, value));
            }
        }

        root.sort_unstable_by_key(|(index, _)| *index);
        debug_assert!(
            root.windows(2).all(|entries| entries[0].0 != entries[1].0),
            "node index map keys must be unique",
        );

        let root_len = root.last().map_or(0, |(index, _)| *index as usize + 1);
        let mut root_presence = RankBitBox::bits_with_capacity(root_len);
        for (index, _) in &root {
            root_presence.set(*index as usize, true);
        }

        Self {
            root_presence: RankBitBox::from_bits(root_presence),
            root_values: root.into_iter().map(|(_, value)| value).collect(),
            sub_ast: FrozenMap::from_entries(sub_ast),
        }
    }

    pub(crate) fn get(&self, index: NodeIndex) -> Option<&V> {
        let raw_index = index.as_u32()?;
        if sub_ast_level(raw_index) != 0 {
            return self.sub_ast.get(&raw_index);
        }

        let index = raw_index as usize;
        if !self.root_presence.get_bit(index)? {
            return None;
        }

        self.root_values
            .get(self.root_presence.rank(index) as usize)
    }

    pub(crate) fn contains_key(&self, index: NodeIndex) -> bool {
        self.get(index).is_some()
    }
}

impl<V> std::ops::Index<NodeIndex> for NodeIndexMap<V> {
    type Output = V;

    #[track_caller]
    fn index(&self, index: NodeIndex) -> &Self::Output {
        self.get(index).expect("key not found")
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_ast::NodeIndex;

    use super::NodeIndexMap;

    #[test]
    fn node_index_map_supports_root_and_sub_ast_indices() {
        let root = NodeIndex::from(2);
        let missing_root = NodeIndex::from(3);
        let sub_ast = NodeIndex::from(1 << 30);
        let map = NodeIndexMap::from_entries([(root, "root"), (sub_ast, "sub-AST")]);

        assert_eq!(map.get(root), Some(&"root"));
        assert_eq!(map.get(missing_root), None);
        assert_eq!(map.get(sub_ast), Some(&"sub-AST"));
    }
}
