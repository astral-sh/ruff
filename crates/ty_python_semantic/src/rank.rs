//! A boxed bit slice that supports a constant-time `rank` operation.

use bitvec::prelude::{BitBox, Msb0};

/// A boxed bit slice that supports a constant-time `rank` operation.
///
/// This can be used to "shrink" a large vector, where you only need to keep certain elements, and
/// you want to continue to use the index in the large vector to identify each element.
///
/// First you create a new smaller vector, keeping only the elements of the large vector that you
/// care about. Now you need a way to translate an index into the large vector (which no longer
/// exists) into the corresponding index into the smaller vector. To do that, you create a bit
/// slice, containing a bit for every element of the original large vector. Each bit in the bit
/// slice indicates whether that element of the large vector was kept in the smaller vector. And
/// the `rank` of the bit gives us the index of the element in the smaller vector.
///
/// However, the naive implementation of `rank` is O(n) in the size of the bit slice. To address
/// that, we use a standard trick: we divide the bit slice into 64-bit chunks, and when
/// constructing the bit slice, precalculate the rank of the first bit in each chunk. Then, to
/// calculate the rank of an arbitrary bit, we first grab the precalculated rank of the chunk that
/// bit belongs to, and add the rank of the bit within its (fixed-sized) chunk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RankBitBox {
    bits: BitBox<u64, Msb0>,
    chunk_ranks: Box<[u32]>,
}

// This must match the `Store` type of the `bits` field above (which is currently `u64`).
const CHUNK_SIZE: usize = 64;

impl RankBitBox {
    pub(crate) fn from_bits(iter: impl Iterator<Item = bool>) -> Self {
        let bits: BitBox<u64, Msb0> = iter.collect();
        let chunk_ranks = bits
            .as_raw_slice()
            .iter()
            .scan(0u32, |rank, chunk| {
                let result = *rank;
                *rank += chunk.count_ones();
                Some(result)
            })
            .collect();
        Self { bits, chunk_ranks }
    }

    pub(crate) fn get_bit(&self, index: usize) -> Option<bool> {
        self.bits.get(index).map(|bit| *bit)
    }

    /// Returns the number of bits _before_ (and not including) the given index that are set.
    pub(crate) fn rank(&self, index: usize) -> u32 {
        let chunk_index = index / CHUNK_SIZE;
        let index_within_chunk = index % CHUNK_SIZE;
        let chunk_rank = self.chunk_ranks[chunk_index];
        if index_within_chunk == 0 {
            return chunk_rank;
        }

        // To calculate the rank within the bit's chunk, we zero out the requested bit and every
        // bit to the right, then count the number of 1s remaining (i.e., to the left of the
        // requested bit).
        let chunk = self.bits.as_raw_slice()[chunk_index];
        let chunk_mask = 0xffff_ffff_ffff_ffff_u64 << (64 - index_within_chunk);
        let rank_within_chunk = (chunk & chunk_mask).count_ones();
        chunk_rank + rank_within_chunk
    }
}

impl get_size2::GetSize for RankBitBox {
    fn get_heap_size(&self) -> usize {
        self.bits.as_raw_slice().get_heap_size() + self.chunk_ranks.get_heap_size()
    }
}
