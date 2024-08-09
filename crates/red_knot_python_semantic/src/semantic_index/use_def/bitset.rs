use std::collections::{btree_set, BTreeSet};

/// Ordered set of `u32`.
///
/// Uses an inline bit-set for small values (up to 128 * B), falls back to a [`BTreeSet`] if a
/// larger value is inserted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum BitSet<const B: usize> {
    /// Bit-set (in 128-bit blocks) for the first 128 * B entries.
    Blocks([u128; B]),

    /// Overflow beyond 128 * B.
    Overflow(BTreeSet<u32>),
}

impl<const B: usize> Default for BitSet<B> {
    fn default() -> Self {
        // B * 128 must fit in a u32, or else we have unusable bits; this makes the truncating
        // casts to u32 below safe. This would be better as a const assertion, but that's not
        // possible on stable with const generic params. (Note, this data structure doesn't really
        // make sense -- uses too much memory -- with B anywhere close to this big, it should
        // usually be much smaller.)
        debug_assert!(B * 128 < (u32::MAX as usize));
        Self::Blocks([0; B])
    }
}

impl<const B: usize> BitSet<B> {
    // SAFETY: We check above that B * 128 < u32::MAX.
    #[allow(clippy::cast_possible_truncation)]
    const BITS: u32 = (128 * B) as u32;

    /// Create and return a new [`BitSet`] with a single `value` inserted.
    pub(super) fn with(value: u32) -> Self {
        let mut bitset = Self::default();
        bitset.insert(value);
        bitset
    }

    /// Convert from Blocks to Overflow representation.
    fn overflow(&mut self) {
        if matches!(self, Self::Blocks(_)) {
            *self = Self::Overflow(self.iter().collect());
        }
    }

    /// Insert a value into the [`BitSet`].
    ///
    /// Return true if the value was newly inserted, false if already present.
    pub(super) fn insert(&mut self, value: u32) -> bool {
        if value >= Self::BITS {
            self.overflow();
        }
        match self {
            Self::Blocks(blocks) => {
                let value_usize = value as usize;
                let (block, index) = (value_usize / 128, value_usize % 128);
                let missing = blocks[block] & (1_u128 << index) == 0;
                blocks[block] |= 1_u128 << index;
                missing
            }
            Self::Overflow(set) => set.insert(value),
        }
    }

    /// Intersect in-place with another [`BitSet`].
    pub(super) fn intersect(&mut self, other: &BitSet<B>) {
        match (self, other) {
            (Self::Blocks(myblocks), Self::Blocks(other_blocks)) => {
                for i in 0..B {
                    myblocks[i] &= other_blocks[i];
                }
            }
            (Self::Overflow(myset), Self::Overflow(other_set)) => {
                *myset = myset.intersection(other_set).copied().collect();
            }
            (me, other) => {
                for value in other.iter() {
                    me.insert(value);
                }
            }
        }
    }

    /// Return an iterator over the values (in ascending order) in this [`BitSet`].
    pub(super) fn iter(&self) -> BitSetIterator<'_, B> {
        match self {
            Self::Blocks(blocks) => BitSetIterator::Blocks {
                blocks,
                cur_block_index: 0,
                cur_block: blocks[0],
            },
            Self::Overflow(set) => BitSetIterator::Overflow(set.iter()),
        }
    }
}

/// Iterator over values in a [`BitSet`].
#[derive(Debug)]
pub(super) enum BitSetIterator<'a, const B: usize> {
    Blocks {
        /// The blocks we are iterating over.
        blocks: &'a [u128; B],

        /// The index of the block we are currently iterating through.
        cur_block_index: usize,

        /// The block we are currently iterating through (and zeroing as we go.)
        cur_block: u128,
    },
    Overflow(btree_set::Iter<'a, u32>),
}

impl<const B: usize> Iterator for BitSetIterator<'_, B> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Blocks {
                blocks,
                cur_block_index,
                cur_block,
            } => {
                while *cur_block == 0 {
                    if *cur_block_index == B - 1 {
                        return None;
                    }
                    *cur_block_index += 1;
                    *cur_block = blocks[*cur_block_index];
                }
                let value = cur_block.trailing_zeros();
                // reset the lowest set bit
                *cur_block &= cur_block.wrapping_sub(1);
                // SAFETY: `value` cannot be more than 128, `cur_block_index` cannot be more than
                // `B - 1`, and we check above that `B * 128 < u32::MAX`. So both `128 *
                // cur_block_index` and the final value here must fit in u32.
                #[allow(clippy::cast_possible_truncation)]
                Some(value + (128 * *cur_block_index) as u32)
            }
            Self::Overflow(set_iter) => set_iter.next().copied(),
        }
    }
}

impl<const B: usize> std::iter::FusedIterator for BitSetIterator<'_, B> {}

/// Array of [`BitSet<B>`]. Up to N stored inline, more than that in overflow vector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum BitSetArray<const B: usize, const N: usize> {
    Array {
        /// Array of N [`BitSet`].
        array: [BitSet<B>; N],

        /// How many of the bitsets are used?
        size: usize,
    },

    Overflow(Vec<BitSet<B>>),
}

impl<const B: usize, const N: usize> Default for BitSetArray<B, N> {
    fn default() -> Self {
        Self::Array {
            array: std::array::from_fn(|_| BitSet::default()),
            size: 0,
        }
    }
}

impl<const B: usize, const N: usize> BitSetArray<B, N> {
    /// Create a [`BitSetArray`] of `size` empty [`BitSet`]s.
    pub(super) fn of_size(size: usize) -> Self {
        let mut array = Self::default();
        for _ in 0..size {
            array.push(BitSet::default());
        }
        array
    }

    fn overflow(&mut self) {
        match self {
            Self::Array { array, size } => {
                let mut vec: Vec<BitSet<B>> = vec![];
                for bitset in array.iter().take(*size) {
                    vec.push(bitset.clone());
                }
                *self = Self::Overflow(vec);
            }
            Self::Overflow(_) => {}
        }
    }

    /// Push a [`BitSet`] onto the end of the array.
    pub(super) fn push(&mut self, new: BitSet<B>) {
        match self {
            Self::Array { array, size } => {
                *size += 1;
                if *size > N {
                    self.overflow();
                    self.push(new);
                } else {
                    array[*size - 1] = new;
                }
            }
            Self::Overflow(vec) => vec.push(new),
        }
    }

    /// Return a mutable reference to the last [`BitSet`] in the array, or None.
    pub(super) fn last_mut(&mut self) -> Option<&mut BitSet<B>> {
        match self {
            Self::Array { array, size } => {
                if *size == 0 {
                    None
                } else {
                    Some(&mut array[*size - 1])
                }
            }
            Self::Overflow(vec) => vec.last_mut(),
        }
    }

    /// Insert `value` into every [`BitSet`] in this [`BitSetArray`].
    pub(super) fn insert_in_each(&mut self, value: u32) {
        match self {
            Self::Array { array, size } => {
                for bitset in array.iter_mut().take(*size) {
                    bitset.insert(value);
                }
            }
            Self::Overflow(vec) => {
                for bitset in vec {
                    bitset.insert(value);
                }
            }
        }
    }

    /// Return an iterator over each [`BitSet`] in this [`BitSetArray`].
    pub(super) fn iter(&self) -> BitSetArrayIterator<'_, B, N> {
        match self {
            Self::Array { array, size } => BitSetArrayIterator::Array {
                array,
                index: 0,
                size: *size,
            },
            Self::Overflow(vec) => BitSetArrayIterator::Overflow(vec.iter()),
        }
    }
}

/// Iterator over a [`BitSetArray`].
#[derive(Debug)]
pub(super) enum BitSetArrayIterator<'a, const B: usize, const N: usize> {
    Array {
        array: &'a [BitSet<B>; N],
        index: usize,
        size: usize,
    },

    Overflow(core::slice::Iter<'a, BitSet<B>>),
}

impl<'a, const B: usize, const N: usize> Iterator for BitSetArrayIterator<'a, B, N> {
    type Item = &'a BitSet<B>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Array { array, index, size } => {
                if *index >= *size {
                    return None;
                }
                let ret = Some(&array[*index]);
                *index += 1;
                ret
            }
            Self::Overflow(iter) => iter.next(),
        }
    }
}

impl<const B: usize, const N: usize> std::iter::FusedIterator for BitSetArrayIterator<'_, B, N> {}

#[cfg(test)]
mod tests {
    use super::{BitSet, BitSetArray};

    fn assert_bitset<const B: usize>(bitset: &BitSet<B>, contents: &[u32]) {
        assert_eq!(bitset.iter().collect::<Vec<_>>(), contents);
    }

    mod bitset {
        use super::{assert_bitset, BitSet};

        #[test]
        fn iter() {
            let mut b = BitSet::<1>::with(3);
            b.insert(27);
            b.insert(6);
            assert!(matches!(b, BitSet::Blocks(_)));
            assert_bitset(&b, &[3, 6, 27]);
        }

        #[test]
        fn iter_overflow() {
            let mut b = BitSet::<1>::with(140);
            b.insert(100);
            b.insert(129);
            assert!(matches!(b, BitSet::Overflow(_)));
            assert_bitset(&b, &[100, 129, 140]);
        }

        #[test]
        fn intersect() {
            let mut b1 = BitSet::<1>::with(4);
            let mut b2 = BitSet::<1>::with(4);
            b1.insert(23);
            b2.insert(5);

            b1.intersect(&b2);
            assert_bitset(&b1, &[4]);
        }

        #[test]
        fn multiple_blocks() {
            let mut b = BitSet::<2>::with(130);
            b.insert(45);
            assert!(matches!(b, BitSet::Blocks(_)));
            assert_bitset(&b, &[45, 130]);
        }
    }

    fn assert_array<const B: usize, const N: usize>(
        array: &BitSetArray<B, N>,
        contents: &[Vec<u32>],
    ) {
        assert_eq!(
            array
                .iter()
                .map(|bitset| bitset.iter().collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            contents
        );
    }

    mod bitset_array {
        use super::{assert_array, BitSet, BitSetArray};

        #[test]
        fn insert_in_each() {
            let mut ba = BitSetArray::<1, 2>::default();
            assert_array(&ba, &[]);

            ba.push(BitSet::default());
            assert_array(&ba, &[vec![]]);

            ba.insert_in_each(3);
            assert_array(&ba, &[vec![3]]);

            ba.push(BitSet::default());
            assert_array(&ba, &[vec![3], vec![]]);

            ba.insert_in_each(79);
            assert_array(&ba, &[vec![3, 79], vec![79]]);

            assert!(matches!(ba, BitSetArray::Array { .. }));

            ba.push(BitSet::default());
            assert!(matches!(ba, BitSetArray::Overflow(_)));
            assert_array(&ba, &[vec![3, 79], vec![79], vec![]]);

            ba.insert_in_each(130);
            assert_array(&ba, &[vec![3, 79, 130], vec![79, 130], vec![130]]);
        }

        #[test]
        fn of_size() {
            let mut ba = BitSetArray::<1, 2>::of_size(1);
            ba.insert_in_each(5);
            assert_array(&ba, &[vec![5]]);
        }

        #[test]
        fn last_mut() {
            let mut ba = BitSetArray::<1, 2>::of_size(1);
            ba.insert_in_each(3);
            ba.insert_in_each(5);

            ba.last_mut()
                .expect("last to not be None")
                .intersect(&BitSet::with(3));

            assert_array(&ba, &[vec![3]]);
        }

        #[test]
        fn last_mut_none() {
            let mut ba = BitSetArray::<1, 1>::default();

            assert!(ba.last_mut().is_none());
        }
    }
}
