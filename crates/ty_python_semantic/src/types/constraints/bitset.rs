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

    #[cfg(test)]
    fn contains(&self, index: usize) -> bool {
        match self {
            // Fast path
            BitSet::Inline(chunk) => {
                let Ok(index) = u32::try_from(index) else {
                    return false;
                };
                let bit_mask_within_chunk = 1usize.checked_shl(index);
                bit_mask_within_chunk
                    .is_some_and(|bit_mask_within_chunk| (*chunk & bit_mask_within_chunk) != 0)
            }
            BitSet::Heap(chunks) => {
                let chunk_index = index / CHUNK_SIZE;
                chunks.get(chunk_index).is_some_and(|chunk| {
                    let bit_index_within_chunk = index % CHUNK_SIZE;
                    let bit_mask_within_chunk = 1 << bit_index_within_chunk;
                    (*chunk & bit_mask_within_chunk) != 0
                })
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
        match (self, other) {
            (BitSet::Inline(lhs), BitSet::Inline(rhs)) => (*lhs & *rhs) != 0,
            (BitSet::Inline(lhs), BitSet::Heap(rhs)) => (*lhs & rhs[0]) != 0,
            (BitSet::Heap(lhs), BitSet::Inline(rhs)) => (lhs[0] & *rhs) != 0,
            (BitSet::Heap(lhs), BitSet::Heap(rhs)) => {
                std::iter::zip(lhs, rhs).any(|(lhs, rhs)| (*lhs & *rhs) != 0)
            }
        }
    }
}

impl Default for BitSet {
    fn default() -> Self {
        BitSet::Inline(0)
    }
}

impl BitOrAssign<&Self> for BitSet {
    fn bitor_assign(&mut self, rhs: &Self) {
        match self {
            BitSet::Inline(lhs) => match rhs {
                BitSet::Inline(rhs) => {
                    *lhs |= *rhs;
                }
                BitSet::Heap(rhs) => {
                    let mut chunks = rhs.clone();
                    chunks[0] |= *lhs;
                    *self = BitSet::Heap(chunks);
                }
            },
            BitSet::Heap(lhs) => match rhs {
                BitSet::Inline(rhs) => {
                    lhs[0] |= *rhs;
                }
                BitSet::Heap(rhs) => {
                    if lhs.len() < rhs.len() {
                        lhs.resize(rhs.len(), 0);
                    }
                    for (lhs, rhs) in std::iter::zip(lhs, rhs) {
                        *lhs |= *rhs;
                    }
                }
            },
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::ops::BitOr;

    use itertools::Itertools;
    use quickcheck_macros::quickcheck;

    use super::BitSet;

    impl BitSet {
        fn from_elements(elements: &[u16]) -> BitSet {
            let mut bitset = BitSet::default();
            for element in elements {
                bitset.insert(*element as usize);
            }
            bitset
        }

        /// Returns whether two bitsets are _mathematically_ equal. (Our [`Eq`] impl is only used
        /// for salsa caching, and implements the cheaper test of _structural_ equality.)
        fn equals(a: &Self, b: &Self) -> bool {
            // This depends on iter returning the values in sorted order
            a.iter().eq(b.iter())
        }
    }

    impl BitOr<&BitSet> for &BitSet {
        type Output = BitSet;

        fn bitor(self, rhs: &BitSet) -> BitSet {
            let mut result = self.clone();
            result |= rhs;
            result
        }
    }

    #[quickcheck]
    #[expect(clippy::needless_pass_by_value)]
    fn iter_returns_all_elements(elements: Vec<u16>) -> bool {
        let expected: HashSet<_> = elements.iter().copied().map_into::<usize>().collect();
        let bitset = BitSet::from_elements(&elements);
        let actual: HashSet<_> = bitset.iter().collect();
        expected == actual
    }

    #[quickcheck]
    #[expect(clippy::needless_pass_by_value)]
    fn iter_contains_no_duplicates(elements: Vec<u16>) -> bool {
        let bitset = BitSet::from_elements(&elements);
        let as_vec: Vec<_> = bitset.iter().collect();
        let as_set: HashSet<_> = bitset.iter().collect();
        as_vec.len() == as_set.len()
    }

    #[quickcheck]
    #[expect(clippy::needless_pass_by_value)]
    fn union_preserves_iter(a: Vec<u16>, b: Vec<u16>) -> bool {
        let expected: HashSet<_> = std::iter::chain(a.iter().copied(), b.iter().copied())
            .map_into::<usize>()
            .collect();
        let a = BitSet::from_elements(&a);
        let b = BitSet::from_elements(&b);
        let union = &a | &b;
        let actual: HashSet<_> = union.iter().collect();
        expected == actual
    }

    #[quickcheck]
    #[expect(clippy::needless_pass_by_value)]
    fn union_preserves_membership(a: Vec<u16>, b: Vec<u16>, member: u16) -> bool {
        let a = BitSet::from_elements(&a);
        let b = BitSet::from_elements(&b);
        let union = &a | &b;
        let member = member as usize;
        (a.contains(member) || b.contains(member)) == (union.contains(member))
    }

    #[quickcheck]
    #[expect(clippy::needless_pass_by_value)]
    fn union_has_left_identity(elements: Vec<u16>) -> bool {
        let bitset = BitSet::from_elements(&elements);
        let empty = BitSet::default();
        let union = &empty | &bitset;
        BitSet::equals(&bitset, &union)
    }

    #[quickcheck]
    #[expect(clippy::needless_pass_by_value)]
    fn union_has_right_identity(elements: Vec<u16>) -> bool {
        let bitset = BitSet::from_elements(&elements);
        let empty = BitSet::default();
        let union = &bitset | &empty;
        BitSet::equals(&bitset, &union)
    }

    #[quickcheck]
    #[expect(clippy::needless_pass_by_value)]
    fn union_is_idempotent(elements: Vec<u16>) -> bool {
        let bitset = BitSet::from_elements(&elements);
        let union = &bitset | &bitset;
        BitSet::equals(&bitset, &union)
    }

    #[quickcheck]
    #[expect(clippy::needless_pass_by_value)]
    fn union_is_commutative(a: Vec<u16>, b: Vec<u16>) -> bool {
        let a = BitSet::from_elements(&a);
        let b = BitSet::from_elements(&b);
        let leftwards = &a | &b;
        let rightwards = &b | &a;
        BitSet::equals(&leftwards, &rightwards)
    }

    #[quickcheck]
    #[expect(clippy::needless_pass_by_value)]
    fn union_is_associative(a: Vec<u16>, b: Vec<u16>, c: Vec<u16>) -> bool {
        let a = BitSet::from_elements(&a);
        let b = BitSet::from_elements(&b);
        let c = BitSet::from_elements(&c);
        let leftwards = &(&a | &b) | &c;
        let rightwards = &a | &(&b | &c);
        BitSet::equals(&leftwards, &rightwards)
    }

    #[quickcheck]
    #[expect(clippy::needless_pass_by_value)]
    fn overlaps_with_iff_not_disjoint(a: Vec<u16>, b: Vec<u16>) -> bool {
        let set_a: HashSet<_> = a.iter().copied().collect();
        let set_b: HashSet<_> = b.iter().copied().collect();
        let a = BitSet::from_elements(&a);
        let b = BitSet::from_elements(&b);
        a.overlaps_with(&b) != set_a.is_disjoint(&set_b)
    }
}
