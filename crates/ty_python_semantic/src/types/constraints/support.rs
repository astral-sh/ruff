//! Tracks the support of each constraint and interior node in a BDD.
//!
//! The support of a constraint is the set of typevars mentioned anywhere in the constraint
//! (either the subject, or anywhere in the lower or upper bound).
//!
//! The support of a node is the union of the supports of every constraint reachable from that
//! node.

use std::ops::BitOrAssign;

use crate::types::constraints::TypeVarId;

use ruff_index::newtype_index;
use smallvec::SmallVec;

#[newtype_index]
#[derive(get_size2::GetSize)]
pub(super) struct SupportId;

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct Support {
    chunks: SmallVec<[usize; 2]>,
    has_skipped_lazy_attributes: bool,
}

const CHUNK_SIZE: usize = usize::BITS as usize;

impl Support {
    /// Adds a typevar to this support.
    pub(super) fn insert(&mut self, typevar: TypeVarId) {
        let index = typevar.index();
        let chunks_needed = (index + 1).div_ceil(CHUNK_SIZE);
        if self.chunks.len() < chunks_needed {
            self.chunks.resize(chunks_needed, 0);
        }

        let chunk_index = index / CHUNK_SIZE;
        let bit_index_within_chunk = index % CHUNK_SIZE;
        let bit_mask_within_chunk = 1 << bit_index_within_chunk;
        self.chunks[chunk_index] |= bit_mask_within_chunk;
    }

    /// Returns an iterator of all of the typevars in this support.
    pub(super) fn iter(&self) -> impl Iterator<Item = TypeVarId> + '_ {
        // Iterate through all of the chunks
        let mut next_chunk_start = 0;
        self.chunks.iter().copied().flat_map(move |mut chunk| {
            // Figure out the starting index of this chunk
            let chunk_start = next_chunk_start;
            next_chunk_start += CHUNK_SIZE;

            // Iterate through the set bits in this chunk
            std::iter::from_fn(move || {
                // Find the lowest set bit, if there is one
                let index = chunk.trailing_zeros() as usize;
                if index == CHUNK_SIZE {
                    return None;
                }

                // Clear out the bit we just found.
                chunk ^= 1 << index;

                // And then return it, converted into a TypeVarId
                Some(TypeVarId::from_usize(chunk_start + index))
            })
        })
    }

    /// Returns whether this support contains any type variables in common with `other`.
    pub(super) fn overlaps_with(&self, other: &Self) -> bool {
        std::iter::zip(&self.chunks, &other.chunks).any(|(lhs, rhs)| (*lhs & *rhs) != 0)
    }

    /// Records that lazy type attributes may contain additional type variables.
    pub(super) fn mark_incomplete(&mut self) {
        self.has_skipped_lazy_attributes = true;
    }

    /// Returns whether all type attributes were inspected while collecting this support.
    pub(super) fn is_complete(&self) -> bool {
        !self.has_skipped_lazy_attributes
    }
}

impl BitOrAssign<&Self> for Support {
    fn bitor_assign(&mut self, rhs: &Self) {
        if self.chunks.len() < rhs.chunks.len() {
            self.chunks.resize(rhs.chunks.len(), 0);
        }
        for (lhs, rhs) in std::iter::zip(&mut self.chunks, &rhs.chunks) {
            *lhs |= *rhs;
        }
        self.has_skipped_lazy_attributes |= rhs.has_skipped_lazy_attributes;
    }
}

impl BitOrAssign<Option<&Self>> for Support {
    fn bitor_assign(&mut self, rhs: Option<&Self>) {
        if let Some(rhs) = rhs {
            *self |= rhs;
        }
    }
}
