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
        self.chunks
            .iter()
            .copied()
            .enumerate()
            .flat_map(|(chunk_index, mut chunk)| {
                let mut bit_index = chunk_index * CHUNK_SIZE;
                std::iter::from_fn(move || {
                    while chunk != 0 {
                        let lowest_bit = chunk & 1;
                        chunk >>= 1;
                        bit_index += 1;
                        if lowest_bit != 0 {
                            return Some(TypeVarId::from_usize(bit_index - 1));
                        }
                    }
                    None
                })
            })
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
    }
}

impl BitOrAssign<Option<&Self>> for Support {
    fn bitor_assign(&mut self, rhs: Option<&Self>) {
        if let Some(rhs) = rhs {
            *self |= rhs;
        }
    }
}
