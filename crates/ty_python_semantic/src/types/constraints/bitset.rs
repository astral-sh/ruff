//! A simple bitset implementation, optimized for a small number of available bits.

use std::ops::BitOrAssign;

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) enum BitSet {
    Inline(usize),
    Heap(Vec<usize>),
}

const CHUNK_SIZE: usize = usize::BITS as usize;

impl BitSet {
    pub(super) fn insert(&mut self, index: usize) {
        // Fast path
        if let BitSet::Inline(chunk) = self
            && index < CHUNK_SIZE
        {
            let bit_mask_within_chunk = 1 << index;
            *chunk |= bit_mask_within_chunk;
            return;
        }

        let chunks_needed = (index + 1).div_ceil(CHUNK_SIZE);
        let chunk_index = index / CHUNK_SIZE;
        let bit_index_within_chunk = index % CHUNK_SIZE;
        let bit_mask_within_chunk = 1 << bit_index_within_chunk;

        match self {
            BitSet::Heap(chunks) => {
                if chunks.len() < chunks_needed {
                    chunks.resize(chunks_needed, 0);
                }
                chunks[chunk_index] |= bit_mask_within_chunk;
            }

            BitSet::Inline(chunk) => {
                let mut chunks = vec![0; chunks_needed];
                chunks[0] = *chunk;
                chunks[chunk_index] |= bit_mask_within_chunk;
                *self = BitSet::Heap(chunks);
            }
        }
    }

    fn iter_chunks(&self) -> impl Iterator<Item = usize> + '_ {
        let chunks = match self {
            BitSet::Inline(chunk) => std::slice::from_ref(chunk),
            BitSet::Heap(chunks) => chunks.as_slice(),
        };
        chunks.iter().copied()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        // Iterate through all of the chunks
        let mut next_chunk_start = 0;
        self.iter_chunks().flat_map(move |mut chunk| {
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
                Some(chunk_start + index)
            })
        })
    }

    pub(super) fn overlaps_with(&self, other: &Self) -> bool {
        std::iter::zip(self.iter_chunks(), other.iter_chunks()).any(|(lhs, rhs)| (lhs & rhs) != 0)
    }
}

impl Default for BitSet {
    fn default() -> Self {
        BitSet::Inline(0)
    }
}

impl BitOrAssign<&Self> for BitSet {
    fn bitor_assign(&mut self, rhs: &Self) {
        match (self, rhs) {
            (BitSet::Inline(lhs), BitSet::Inline(rhs)) => {
                *lhs |= *rhs;
            }
            (BitSet::Inline(lhs), BitSet::Heap(rhs)) => {
                *lhs |= rhs[0];
            }
            (BitSet::Heap(lhs), BitSet::Inline(rhs)) => {
                lhs[0] |= *rhs;
            }
            (BitSet::Heap(lhs), BitSet::Heap(rhs)) => {
                if lhs.len() < rhs.len() {
                    lhs.resize(rhs.len(), 0);
                }
                for (lhs, rhs) in std::iter::zip(lhs, rhs) {
                    *lhs |= *rhs;
                }
            }
        }
    }
}

impl BitOrAssign<Option<&Self>> for BitSet {
    fn bitor_assign(&mut self, rhs: Option<&Self>) {
        if let Some(rhs) = rhs {
            *self |= rhs;
        }
    }
}
