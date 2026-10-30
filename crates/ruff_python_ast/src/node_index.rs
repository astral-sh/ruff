use std::num::NonZeroU32;

/// An AST node that has an index.
pub trait HasNodeIndex {
    /// Returns the source-order index of this node in its module.
    fn node_index(&self) -> NodeIndex;
}

impl<T> HasNodeIndex for &T
where
    T: HasNodeIndex,
{
    fn node_index(&self) -> NodeIndex {
        T::node_index(*self)
    }
}

/// A unique index for a node within an AST.
///
/// Our encoding of 32-bit AST node indices is as follows:
///
/// * `u32::MAX`     (1111...1) is reserved as a forbidden value (mapped to 0 for `NonZero`)
/// * `u32::MAX - 1` (1111...0) is reserved for `NodeIndex::NONE`
/// * The top two bits encode the sub-AST level:
///   * 00 is top-level AST
///   * 01 is sub-AST (string annotation)
///   * 10 is sub-sub-AST (string annotation in string annotation)
///   * 11 is forbidden (well, it only appears in the above reserved values)
/// * The remaining 30 bits are the real (sub)-AST node index
///
/// To get the first sub-index of a node's sub-AST we:
///
/// * increment the sub-AST level in the high-bits
/// * at level 1, multiply the real index by 256
/// * at level 2, multiply the real index by 8
///
/// The multiplication gives each node a reserved space of 256 nodes for its sub-AST
/// to work with ("should be enough for anybody"), and 8 nodes for a sub-sub-AST
/// (enough for an identifier and maybe some simple unions).
///
/// Here are some implications:
///
/// * We have 2^30 top-level AST nodes (1 billion)
/// * To have a string annotation, the parent node needs to be multiplied by 256 without
///   overflowing 30 bits, so string annotations cannot be used after 2^22 nodes (4 million),
///   which would be like, a million lines of code.
/// * To have a sub-string annotation, the top-level node needs to be multiplied
///   by 256 * 8, so sub-string annotations cannot be used after 2^19 nodes (500 thousand),
///   or about 100k lines of code.
///
/// This feels like a pretty reasonable compromise that will work well in practice,
/// although it creates some very wonky boundary conditions that will be very unpleasant
/// if someone runs into them.
///
/// That said, string annotations are in many regards "legacy" and so new code ideally
/// doesn't have to use them, and there's never a real reason to use sub-annotation
/// let-alone a sub-sub-annotation.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct NodeIndex(NonZeroU32);

impl NodeIndex {
    /// Placeholder for a node without an assigned index, such as a node created for a fix.
    pub const NONE: Self = Self(NonZeroU32::new(u32::MAX - 1).unwrap());

    /// Returns the index as a `u32`. or `None` for `NodeIndex::NONE`.
    pub fn as_u32(self) -> Option<u32> {
        if self == NodeIndex::NONE {
            None
        } else {
            Some(self.0.get() - 1)
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum NodeIndexError {
    NoParent,
    TooNested,
    ExhaustedSubIndices,
    ExhaustedSubSubIndices,
    OverflowedIndices,
    OverflowedSubIndices,
}

const MAX_LEVEL: u32 = 2;
const LEVEL_BITS: u32 = MAX_LEVEL.bit_width();
const LEVEL_SHIFT: u32 = 32 - LEVEL_BITS;
const LEVEL_MASK: u32 = ((LEVEL_BITS << 1) - 1) << LEVEL_SHIFT;
const SUB_NODES: u32 = 256;
const SUB_SUB_NODES: u32 = 8;
pub const MAX_REAL_INDEX: u32 = (1 << LEVEL_SHIFT) - 1;

/// sub-AST level is stored in the top two bits
pub fn sub_ast_level(index: u32) -> u32 {
    (index & LEVEL_MASK) >> LEVEL_SHIFT
}

/// Get the first and last index of the sub-AST of the input
pub fn sub_indices(index: u32) -> Result<(u32, u32), NodeIndexError> {
    let level = sub_ast_level(index);
    if level >= MAX_LEVEL {
        return Err(NodeIndexError::TooNested);
    }
    let next_level = (level + 1) << LEVEL_SHIFT;
    let without_level = index & !LEVEL_MASK;
    let (nodes_in_level, error_kind) = if level == 0 {
        (SUB_NODES, NodeIndexError::OverflowedIndices)
    } else if level == 1 {
        (SUB_SUB_NODES, NodeIndexError::OverflowedSubIndices)
    } else {
        unreachable!(
            "Someone made a mistake updating the encoding of node indices: {index:08X} had level {level}"
        );
    };

    // If this overflows the file has hundreds of thousands of lines of code,
    // but that *can* happen (we just can't support string annotations that deep)
    let sub_index_without_level = without_level
        .checked_mul(nodes_in_level)
        .ok_or(error_kind)?;
    if sub_index_without_level > MAX_REAL_INDEX {
        return Err(error_kind);
    }

    let first_index = sub_index_without_level | next_level;
    // Can't overflow by construction
    let last_index = first_index + nodes_in_level - 1;
    Ok((first_index, last_index))
}

impl Default for NodeIndex {
    fn default() -> Self {
        Self::NONE
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

#[cfg(test)]
mod tests {
    use super::NodeIndex;

    #[test]
    fn node_index() {
        assert_eq!(NodeIndex::default(), NodeIndex::NONE);
        assert_eq!(NodeIndex::NONE.as_u32(), None);
        assert_eq!(format!("{:?}", NodeIndex::NONE), "NodeIndex(None)");
        assert_eq!(NodeIndex::from(0).as_u32(), Some(0));
        assert_eq!(NodeIndex::from(1).as_u32(), Some(1));
    }
}
