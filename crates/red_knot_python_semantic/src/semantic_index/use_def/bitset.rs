/// Ordered set of `u32`.
///
/// Uses an inline bit-set for small values (up to 64 * B), falls back to heap allocated vector of
/// blocks for larger values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum BitSet<const B: usize> {
    /// Bit-set (in 64-bit blocks) for the first 64 * B entries.
    Inline([u64; B]),

    /// Overflow beyond 64 * B.
    Heap(Vec<u64>),
}

impl<const B: usize> Default for BitSet<B> {
    fn default() -> Self {
        // B * 64 must fit in a u32, or else we have unusable bits; this assertion makes the
        // truncating casts to u32 below safe. This would be better as a const assertion, but
        // that's not possible on stable with const generic params. (B should never really be
        // anywhere close to this large.)
        assert!(B * 64 < (u32::MAX as usize));
        // This implementation requires usize >= 32 bits.
        static_assertions::const_assert!(usize::BITS >= 32);
        Self::Inline([0; B])
    }
}

impl<const B: usize> BitSet<B> {
    /// Create and return a new [`BitSet`] with a single `value` inserted.
    pub(super) fn with(value: u32) -> Self {
        let mut bitset = Self::default();
        bitset.insert(value);
        bitset
    }

    pub(super) fn is_empty(&self) -> bool {
        self.blocks().iter().all(|&b| b == 0)
    }

    /// Convert from Inline to Heap, if needed, and resize the Heap vector, if needed.
    fn resize(&mut self, value: u32) {
        let num_blocks_needed = (value / 64) + 1;
        self.resize_blocks(num_blocks_needed as usize);
    }

    fn resize_blocks(&mut self, num_blocks_needed: usize) {
        match self {
            Self::Inline(blocks) => {
                let mut vec = blocks.to_vec();
                vec.resize(num_blocks_needed, 0);
                *self = Self::Heap(vec);
            }
            Self::Heap(vec) => {
                vec.resize(num_blocks_needed, 0);
            }
        }
    }

    fn blocks_mut(&mut self) -> &mut [u64] {
        match self {
            Self::Inline(blocks) => blocks.as_mut_slice(),
            Self::Heap(blocks) => blocks.as_mut_slice(),
        }
    }

    fn blocks(&self) -> &[u64] {
        match self {
            Self::Inline(blocks) => blocks.as_slice(),
            Self::Heap(blocks) => blocks.as_slice(),
        }
    }

    /// Insert a value into the [`BitSet`].
    ///
    /// Return true if the value was newly inserted, false if already present.
    pub(super) fn insert(&mut self, value: u32) -> bool {
        let value_usize = value as usize;
        let (block, index) = (value_usize / 64, value_usize % 64);
        if block >= self.blocks().len() {
            self.resize(value);
        }
        let blocks = self.blocks_mut();
        let missing = blocks[block] & (1 << index) == 0;
        blocks[block] |= 1 << index;
        missing
    }

    /// Intersect in-place with another [`BitSet`].
    pub(super) fn intersect(&mut self, other: &BitSet<B>) {
        let my_blocks = self.blocks_mut();
        let other_blocks = other.blocks();
        let min_len = my_blocks.len().min(other_blocks.len());
        for i in 0..min_len {
            my_blocks[i] &= other_blocks[i];
        }
        for block in my_blocks.iter_mut().skip(min_len) {
            *block = 0;
        }
    }

    /// Union in-place with another [`BitSet`].
    pub(super) fn union(&mut self, other: &BitSet<B>) {
        let mut max_len = self.blocks().len();
        let other_len = other.blocks().len();
        if other_len > max_len {
            max_len = other_len;
            self.resize_blocks(max_len);
        }
        for (my_block, other_block) in self.blocks_mut().iter_mut().zip(other.blocks()) {
            *my_block |= other_block;
        }
    }

    /// Return an iterator over the values (in ascending order) in this [`BitSet`].
    pub(super) fn iter(&self) -> BitSetIterator<'_, B> {
        let blocks = self.blocks();
        BitSetIterator {
            blocks,
            current_block_index: 0,
            current_block: blocks[0],
        }
    }
}

/// Iterator over values in a [`BitSet`].
#[derive(Debug)]
pub(super) struct BitSetIterator<'a, const B: usize> {
    /// The blocks we are iterating over.
    blocks: &'a [u64],

    /// The index of the block we are currently iterating through.
    current_block_index: usize,

    /// The block we are currently iterating through (and zeroing as we go.)
    current_block: u64,
}

impl<const B: usize> Iterator for BitSetIterator<'_, B> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_block == 0 {
            if self.current_block_index + 1 >= self.blocks.len() {
                return None;
            }
            self.current_block_index += 1;
            self.current_block = self.blocks[self.current_block_index];
        }
        let lowest_bit_set = self.current_block.trailing_zeros();
        // reset the lowest set bit, without a data dependency on `lowest_bit_set`
        self.current_block &= self.current_block.wrapping_sub(1);
        // SAFETY: `lowest_bit_set` cannot be more than 64, `current_block_index` cannot be more
        // than `B - 1`, and we check above that `B * 64 < u32::MAX`. So both `64 *
        // current_block_index` and the final value here must fit in u32.
        #[allow(clippy::cast_possible_truncation)]
        Some(lowest_bit_set + (64 * self.current_block_index) as u32)
    }
}

impl<const B: usize> std::iter::FusedIterator for BitSetIterator<'_, B> {}

#[cfg(test)]
mod tests {
    use super::BitSet;

    fn assert_bitset<const B: usize>(bitset: &BitSet<B>, contents: &[u32]) {
        assert_eq!(bitset.iter().collect::<Vec<_>>(), contents);
    }

    #[test]
    fn iter() {
        let mut b = BitSet::<1>::with(3);
        b.insert(27);
        b.insert(6);
        assert!(matches!(b, BitSet::Inline(_)));
        assert_bitset(&b, &[3, 6, 27]);
    }

    #[test]
    fn iter_overflow() {
        let mut b = BitSet::<1>::with(140);
        b.insert(100);
        b.insert(129);
        assert!(matches!(b, BitSet::Heap(_)));
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
    fn intersect_mixed_1() {
        let mut b1 = BitSet::<1>::with(4);
        let mut b2 = BitSet::<1>::with(4);
        b1.insert(89);
        b2.insert(5);

        b1.intersect(&b2);
        assert_bitset(&b1, &[4]);
    }

    #[test]
    fn intersect_mixed_2() {
        let mut b1 = BitSet::<1>::with(4);
        let mut b2 = BitSet::<1>::with(4);
        b1.insert(23);
        b2.insert(89);

        b1.intersect(&b2);
        assert_bitset(&b1, &[4]);
    }

    #[test]
    fn intersect_heap() {
        let mut b1 = BitSet::<1>::with(4);
        let mut b2 = BitSet::<1>::with(4);
        b1.insert(89);
        b2.insert(90);

        b1.intersect(&b2);
        assert_bitset(&b1, &[4]);
    }

    #[test]
    fn intersect_heap_2() {
        let mut b1 = BitSet::<1>::with(89);
        let mut b2 = BitSet::<1>::with(89);
        b1.insert(91);
        b2.insert(90);

        b1.intersect(&b2);
        assert_bitset(&b1, &[89]);
    }

    #[test]
    fn union() {
        let mut b1 = BitSet::<1>::with(2);
        let b2 = BitSet::<1>::with(4);

        b1.union(&b2);
        assert_bitset(&b1, &[2, 4]);
    }

    #[test]
    fn union_mixed_1() {
        let mut b1 = BitSet::<1>::with(4);
        let mut b2 = BitSet::<1>::with(4);
        b1.insert(89);
        b2.insert(5);

        b1.union(&b2);
        assert_bitset(&b1, &[4, 5, 89]);
    }

    #[test]
    fn union_mixed_2() {
        let mut b1 = BitSet::<1>::with(4);
        let mut b2 = BitSet::<1>::with(4);
        b1.insert(23);
        b2.insert(89);

        b1.union(&b2);
        assert_bitset(&b1, &[4, 23, 89]);
    }

    #[test]
    fn union_heap() {
        let mut b1 = BitSet::<1>::with(4);
        let mut b2 = BitSet::<1>::with(4);
        b1.insert(89);
        b2.insert(90);

        b1.union(&b2);
        assert_bitset(&b1, &[4, 89, 90]);
    }

    #[test]
    fn union_heap_2() {
        let mut b1 = BitSet::<1>::with(89);
        let mut b2 = BitSet::<1>::with(89);
        b1.insert(91);
        b2.insert(90);

        b1.union(&b2);
        assert_bitset(&b1, &[89, 90, 91]);
    }

    #[test]
    fn multiple_blocks() {
        let mut b = BitSet::<2>::with(120);
        b.insert(45);
        assert!(matches!(b, BitSet::Inline(_)));
        assert_bitset(&b, &[45, 120]);
    }

    #[test]
    fn empty() {
        let b = BitSet::<1>::default();

        assert!(b.is_empty());
    }
}
