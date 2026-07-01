use ruff_index::Idx;
use ruff_python_ast::{HasNodeIndex, NodeIndex};

use crate::ast_node_ref::AstNodeRef;
use crate::frozen::FrozenMap;

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

/// A node-index map whose values use 16 bits when every stored index fits.
#[derive(Debug, Eq, PartialEq, salsa::Update, get_size2::GetSize)]
pub(crate) struct NarrowNodeIndexMap<I>(NarrowNodeIndexStorage<I>);

#[derive(Debug, Eq, PartialEq, salsa::Update, get_size2::GetSize)]
enum NarrowNodeIndexStorage<I> {
    Narrow(Box<[NarrowNodeIndexEntry]>),
    Wide(FrozenMap<NodeKey, I>),
}

/// A packed node index and 16-bit index value.
///
/// The byte array keeps the entry's alignment at one; `(NodeKey, u16)` has two bytes of tail
/// padding and occupies eight bytes.
#[derive(Debug, Eq, PartialEq, salsa::Update, get_size2::GetSize)]
struct NarrowNodeIndexEntry([u8; 6]);

impl NarrowNodeIndexEntry {
    fn new(node_index: u32, value: u16) -> Self {
        let node_index = node_index.to_ne_bytes();
        let value = value.to_ne_bytes();
        Self([
            node_index[0],
            node_index[1],
            node_index[2],
            node_index[3],
            value[0],
            value[1],
        ])
    }

    fn node_index(&self) -> u32 {
        u32::from_ne_bytes([self.0[0], self.0[1], self.0[2], self.0[3]])
    }

    fn value(&self) -> u16 {
        u16::from_ne_bytes([self.0[4], self.0[5]])
    }
}

impl<I: Idx> NarrowNodeIndexMap<I> {
    pub(crate) fn from_entries(entries: impl IntoIterator<Item = (NodeIndex, I)>) -> Self {
        let entries = entries.into_iter().collect::<Vec<_>>();
        let narrow = entries
            .iter()
            .map(|(index, value)| {
                Some(NarrowNodeIndexEntry::new(
                    index.as_u32()?,
                    u16::try_from(value.index()).ok()?,
                ))
            })
            .collect::<Option<Vec<_>>>();

        if let Some(mut narrow) = narrow {
            narrow.sort_unstable_by_key(NarrowNodeIndexEntry::node_index);
            debug_assert!(
                narrow
                    .windows(2)
                    .all(|entries| entries[0].node_index() != entries[1].node_index()),
                "narrow node index map keys must be unique",
            );
            Self(NarrowNodeIndexStorage::Narrow(narrow.into_boxed_slice()))
        } else {
            Self(NarrowNodeIndexStorage::Wide(FrozenMap::from_entries(
                entries
                    .into_iter()
                    .map(|(index, value)| (NodeKey(index), value))
                    .collect(),
            )))
        }
    }

    pub(crate) fn get(&self, index: NodeIndex) -> Option<I> {
        match &self.0 {
            NarrowNodeIndexStorage::Narrow(entries) => {
                let index = index.as_u32()?;
                let entry = entries
                    .binary_search_by_key(&index, NarrowNodeIndexEntry::node_index)
                    .ok()
                    .and_then(|index| entries.get(index))?;
                Some(I::new(usize::from(entry.value())))
            }
            NarrowNodeIndexStorage::Wide(map) => map.get(&NodeKey(index)).copied(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::mem::{size_of, size_of_val};

    use ruff_index::newtype_index;
    use ruff_python_ast::NodeIndex;

    use super::{NarrowNodeIndexEntry, NarrowNodeIndexMap, NarrowNodeIndexStorage, NodeKey};
    use crate::frozen::FrozenMap;

    #[newtype_index]
    #[derive(get_size2::GetSize)]
    struct TestId;

    #[test]
    fn narrow_node_index_entry_is_packed() {
        assert_eq!(size_of::<NarrowNodeIndexEntry>(), 6);
    }

    #[test]
    fn narrow_node_index_map_uses_less_retained_memory() {
        let entries = (0..16)
            .map(|index| (NodeIndex::from(index), TestId::from_usize(index as usize)))
            .collect::<Vec<_>>();
        let narrow = NarrowNodeIndexMap::from_entries(entries.iter().copied());
        let wide = FrozenMap::from_entries(
            entries
                .into_iter()
                .map(|(index, value)| (NodeKey(index), value))
                .collect(),
        );

        let narrow_size = size_of_val(&narrow) + ruff_memory_usage::heap_size(&narrow);
        let wide_size = size_of_val(&wide) + ruff_memory_usage::heap_size(&wide);
        assert!(narrow_size < wide_size, "{narrow_size} >= {wide_size}");
    }

    #[test]
    fn narrow_node_index_map_looks_up_values_and_falls_back_for_wide_values() {
        let node = NodeIndex::from(1);
        let missing = NodeIndex::from(2);
        let other = NodeIndex::from(3);
        let narrow = NarrowNodeIndexMap::from_entries([
            (other, TestId::from_usize(7)),
            (node, TestId::from_usize(u16::MAX as usize)),
        ]);
        let wide =
            NarrowNodeIndexMap::from_entries([(node, TestId::from_usize(u16::MAX as usize + 1))]);

        assert!(matches!(narrow.0, NarrowNodeIndexStorage::Narrow(_)));
        assert!(matches!(wide.0, NarrowNodeIndexStorage::Wide(_)));
        assert_eq!(
            narrow.get(node),
            Some(TestId::from_usize(u16::MAX as usize))
        );
        assert_eq!(narrow.get(missing), None);
        assert_eq!(narrow.get(other), Some(TestId::from_usize(7)));
        assert_eq!(
            wide.get(node),
            Some(TestId::from_usize(u16::MAX as usize + 1))
        );
    }
}
