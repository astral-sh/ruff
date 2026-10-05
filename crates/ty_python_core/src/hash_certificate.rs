/// Native hash lookup with retained bounds on equality candidates and control-byte reads.
///
/// Hashbrown 0.17.1 filters candidates by seven-bit control tags. Its generic backend can also
/// match a tag differing in the lowest bit, so this certificate counts adjacent pairs of tags.
/// It relies on that implementation visiting each occupied bucket at most once during lookup.
#[derive(Debug, get_size2::GetSize)]
pub(crate) struct FrozenHashTable<I> {
    table: hashbrown::HashTable<I>,
    max_candidates: usize,
    max_probe_bytes: usize,
}

impl<I> FrozenHashTable<I> {
    /// Certify the actual entries after the mutable table's final rehash.
    ///
    /// The keys used by `hash` must remain unchanged while this table is retained. The temporary
    /// histogram stays local to finalization; only its maximum is kept with the immutable table.
    pub(crate) fn new(table: hashbrown::HashTable<I>, hash: impl Fn(&I) -> u64) -> Self {
        let buckets = table.num_buckets();
        // In hashbrown 0.17.1, capacity is occupied entries plus growth allowance. Deleted
        // buckets reduce that sum, so maximum capacity certifies that absent buckets are empty.
        let max_probe_bytes =
            if buckets >= 16 && buckets.is_power_of_two() && table.capacity() == buckets / 8 * 7 {
                full_group_probe_bytes(buckets, |index| table.get_bucket(index).is_some())
            } else {
                buckets
            };
        let mut populations = [0usize; 64];
        let mut max_candidates = 0;
        for id in table.iter() {
            let Some(class) = paired_tag_for_pointer_bytes(hash(id), size_of::<usize>()) else {
                return Self {
                    max_candidates: table.len(),
                    max_probe_bytes: buckets,
                    table,
                };
            };
            let population = &mut populations[class];
            *population = population.saturating_add(1);
            max_candidates = max_candidates.max(*population);
        }
        Self {
            table,
            max_candidates,
            max_probe_bytes,
        }
    }

    #[inline]
    pub(crate) fn find(&self, hash: u64, equals: impl FnMut(&I) -> bool) -> Option<&I> {
        self.table.find(hash, equals)
    }

    /// Bound key hashing, equality and probing without inspecting the key before admission.
    ///
    /// `key_work` must cover the hash and one complete key comparison. Tables without a
    /// certified group bound retain the full bucket-probe charge.
    pub(crate) fn lookup_work(&self, key_work: usize) -> Option<usize> {
        self.max_candidates
            .checked_add(1)?
            .checked_mul(key_work)?
            .checked_add(self.max_probe_bytes.checked_add(32)?)?
            .checked_mul(4)
    }

    #[cfg(test)]
    pub(crate) const fn max_candidates(&self) -> usize {
        self.max_candidates
    }
}

/// Bounds control bytes read by native lookup in a table with no deleted buckets.
///
/// Requires a power-of-two bucket count of at least 16 and at least one empty bucket. Circular
/// groups with the same starting offset modulo their width partition the buckets. Each probe
/// stays within one such partition and visits distinct groups. All groups before termination
/// must be full, so a partition with F full groups needs at most F + 1 loads. The audited generic
/// and SIMD backends use widths 4, 8 or 16, independently of pointer width.
fn full_group_probe_bytes(buckets: usize, is_full: impl Fn(usize) -> bool) -> usize {
    let mut maximum = 0;
    for width in [4, 8, 16] {
        let mut full_groups = [0usize; 16];
        let mut occupied = (0..width).filter(|&index| is_full(index)).count();
        for start in 0..buckets {
            if occupied == width {
                full_groups[start % width] += 1;
            }
            occupied -= usize::from(is_full(start));
            occupied += usize::from(is_full((start + width) & (buckets - 1)));
        }
        for full in &full_groups[..width] {
            maximum = maximum.max((full + 1) * width);
        }
    }
    maximum
}

/// Extract the audited paired tag for 32-bit or 64-bit pointers.
///
/// Hashbrown takes the top seven bits of the lower pointer-sized part of the hash. Ignoring that
/// tag's lowest bit includes the generic backend's false positives. Other pointer widths keep
/// the complete occupied-entry bound instead of assuming the same extraction contract.
pub(crate) const fn paired_tag_for_pointer_bytes(hash: u64, pointer_bytes: usize) -> Option<usize> {
    let shift = match pointer_bytes {
        4 => 26,
        8 => 58,
        _ => return None,
    };
    Some(((hash >> shift) & 0x3f) as usize)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{FrozenHashTable, full_group_probe_bytes, paired_tag_for_pointer_bytes};

    #[derive(Debug, Eq, PartialEq)]
    struct Entry {
        id: usize,
        hash: u64,
    }

    fn tagged_hash(tag: u8, low: u64) -> u64 {
        let shift = size_of::<usize>() * 8 - 7;
        (u64::from(tag) << shift) | (low & ((1u64 << shift) - 1))
    }

    fn table(hashes: &[u64], capacity: usize, shrink: bool) -> FrozenHashTable<Entry> {
        let mut table = hashbrown::HashTable::with_capacity(capacity);
        for (id, &hash) in hashes.iter().enumerate() {
            table.insert_unique(hash, Entry { id, hash }, |entry| entry.hash);
        }
        if shrink {
            table.shrink_to_fit(|entry| entry.hash);
        }
        FrozenHashTable::new(table, |entry| entry.hash)
    }

    fn check_callbacks(table: &FrozenHashTable<Entry>, hash: u64, wanted: Option<usize>) {
        let mut visited = BTreeSet::new();
        let mut comparisons = 0;
        let found = table.find(hash, |entry| {
            comparisons += 1;
            assert!(
                visited.insert(entry.id),
                "native lookup repeated a candidate"
            );
            assert_eq!(
                paired_tag_for_pointer_bytes(entry.hash, size_of::<usize>()),
                paired_tag_for_pointer_bytes(hash, size_of::<usize>()),
            );
            wanted == Some(entry.id)
        });
        assert_eq!(found.map(|entry| entry.id), wanted);
        assert!(comparisons <= table.max_candidates());
    }

    #[test]
    fn pointer_width_tag_extraction_includes_adjacent_tags() {
        for bytes in [4, 8] {
            let shift = bytes * 8 - 7;
            for tag in 0u8..128 {
                let hash = u64::from(tag) << shift;
                assert_eq!(
                    paired_tag_for_pointer_bytes(hash, bytes),
                    Some(usize::from(tag / 2))
                );
                assert_eq!(
                    paired_tag_for_pointer_bytes(hash | ((1u64 << shift) - 1), bytes),
                    Some(usize::from(tag / 2)),
                );
                if bytes == 4 {
                    assert_eq!(
                        paired_tag_for_pointer_bytes(hash | (u64::from(u32::MAX) << 32), bytes),
                        Some(usize::from(tag / 2)),
                    );
                }
            }
        }
        for bytes in [0, 1, 2, 3, 5, 16, usize::MAX] {
            assert_eq!(paired_tag_for_pointer_bytes(u64::MAX, bytes), None);
        }
    }

    #[test]
    fn native_lookup_respects_certificate_across_sizes_and_capacities() {
        for len in [0, 1, 2, 3, 4, 7, 8, 9, 15, 16, 17, 31, 32, 65, 129] {
            let hashes: Vec<_> = (0..len)
                .map(|id| tagged_hash((id % 128) as u8, (id as u64).wrapping_mul(17)))
                .collect();
            for capacity in [len, len * 4 + 17] {
                for shrink in [false, true] {
                    let table = table(&hashes, capacity, shrink);
                    let mut populations = [0; 64];
                    for &hash in &hashes {
                        if let Some(class) = paired_tag_for_pointer_bytes(hash, size_of::<usize>())
                        {
                            populations[class] += 1;
                        }
                    }
                    assert_eq!(
                        table.max_candidates(),
                        populations.into_iter().max().unwrap_or(0)
                    );
                    for (id, &hash) in hashes.iter().enumerate() {
                        check_callbacks(&table, hash, Some(id));
                    }
                    for tag in 0u8..128 {
                        for start in [0, 1, 7, 15, 31, 63, 127, 255, u64::MAX] {
                            check_callbacks(&table, tagged_hash(tag, start), None);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn full_hash_and_same_tag_collisions_keep_every_id() {
        for len in [1, 3, 7, 15, 17, 63, 129] {
            for varied_low_bits in [false, true] {
                let hashes: Vec<_> = (0..len)
                    .map(|id| {
                        tagged_hash(
                            42,
                            if varied_low_bits {
                                id as u64 * 256 + 15
                            } else {
                                15
                            },
                        )
                    })
                    .collect();
                let table = table(&hashes, 0, true);
                assert_eq!(table.max_candidates(), len);
                for (id, &hash) in hashes.iter().enumerate() {
                    check_callbacks(&table, hash, Some(id));
                }
                check_callbacks(&table, tagged_hash(42, 15), None);
                check_callbacks(&table, tagged_hash(43, 15), None);
                check_callbacks(&table, tagged_hash(44, 15), None);
                if !varied_low_bits {
                    let mut comparisons = 0;
                    assert!(
                        table
                            .find(hashes[0], |_| {
                                comparisons += 1;
                                false
                            })
                            .is_none()
                    );
                    assert_eq!(comparisons, len);
                }
            }
        }
    }

    #[test]
    fn certificate_counts_occupied_entries_after_removal_and_reservation() {
        let mut native = hashbrown::HashTable::with_capacity(129);
        for id in 0..129 {
            let hash = tagged_hash((id % 128) as u8, id as u64);
            native.insert_unique(hash, Entry { id, hash }, |entry| entry.hash);
        }
        for id in (0..129).step_by(3) {
            let hash = tagged_hash((id % 128) as u8, id as u64);
            let removed = native
                .find_entry(hash, |entry| entry.id == id)
                .ok()
                .map(|entry| entry.remove().0);
            assert_eq!(removed.map(|entry| entry.id), Some(id));
        }
        native.reserve(257, |entry| entry.hash);
        let table = FrozenHashTable::new(native, |entry| entry.hash);
        for id in 0..129 {
            let hash = tagged_hash((id % 128) as u8, id as u64);
            check_callbacks(&table, hash, (id % 3 != 0).then_some(id));
        }
        for tag in 0u8..128 {
            check_callbacks(&table, tagged_hash(tag, u64::MAX), None);
        }
    }

    #[test]
    fn quotation_uses_certified_probe_work_and_checks_overflow() {
        let hashes: Vec<_> = (0..128)
            .map(|tag| tagged_hash(tag, u64::from(tag)))
            .collect();
        let table = table(&hashes, 1024, false);
        assert_eq!(table.max_candidates(), 2);
        let probes = table.max_probe_bytes + 32;
        assert!(table.max_probe_bytes < table.table.num_buckets());
        for key_work in [8, 32, 1024] {
            assert_eq!(
                table.lookup_work(key_work),
                Some(4 * ((2 + 1) * key_work + probes))
            );
        }
        assert_eq!(table.lookup_work(usize::MAX), None);
        assert_eq!(table.lookup_work(usize::MAX / 4), None);
    }
    fn check_probe_bound(buckets: usize, is_full: impl Fn(usize) -> bool) {
        let bound = full_group_probe_bytes(buckets, &is_full);
        assert!(bound <= buckets);
        for width in [4, 8, 16] {
            for start in 0..buckets {
                let mut position = start;
                let mut stride = 0;
                let mut bytes = 0;
                loop {
                    bytes += width;
                    assert!(
                        bytes <= bound,
                        "buckets={buckets}, width={width}, start={start}"
                    );
                    if (0..width).any(|offset| !is_full((position + offset) & (buckets - 1))) {
                        break;
                    }
                    stride += width;
                    position = (position + stride) & (buckets - 1);
                }
            }
        }
    }

    #[test]
    fn full_group_certificate_bounds_wrapped_triangular_probes() {
        for mask in 0u32..u32::from(u16::MAX) {
            check_probe_bound(16, |index| mask & (1 << index) != 0);
        }
        for buckets in [32, 64, 128, 256] {
            for gap in 0..buckets {
                check_probe_bound(buckets, |index| index != gap);
                check_probe_bound(buckets, |index| (index + gap) % 7 != 0);
            }
        }
    }

    #[test]
    fn native_table_probe_bound_handles_collisions_and_spare_capacity() {
        for len in [0, 1, 7, 14, 17, 28, 63, 129] {
            for collide in [false, true] {
                let hashes: Vec<_> = (0..len)
                    .map(|id| tagged_hash(42, if collide { 15 } else { id as u64 * 17 }))
                    .collect();
                for capacity in [len, len * 4 + 17] {
                    let table = table(&hashes, capacity, false);
                    let buckets = table.table.num_buckets();
                    if buckets >= 16 {
                        assert_eq!(table.table.capacity(), buckets / 8 * 7);
                        check_probe_bound(buckets, |index| table.table.get_bucket(index).is_some());
                    } else {
                        assert_eq!(table.max_probe_bytes, buckets);
                    }
                    for (id, &hash) in hashes.iter().enumerate() {
                        check_callbacks(&table, hash, Some(id));
                    }
                }
            }
        }
    }

    #[test]
    fn retained_tombstones_keep_the_full_probe_bound() {
        let hash = tagged_hash(42, 15);
        let mut native = hashbrown::HashTable::with_capacity(28);
        for id in 0..28 {
            native.insert_unique(hash, Entry { id, hash }, |entry| entry.hash);
        }
        for id in 0..28 {
            let removed = native
                .find_entry(hash, |entry| entry.id == id)
                .ok()
                .map(|entry| entry.remove().0.id);
            assert_eq!(removed, Some(id));
            if native.capacity() != native.num_buckets() / 8 * 7 {
                break;
            }
        }
        assert_ne!(native.capacity(), native.num_buckets() / 8 * 7);
        let buckets = native.num_buckets();
        let table = FrozenHashTable::new(native, |entry| entry.hash);
        assert_eq!(table.max_probe_bytes, buckets);
        check_callbacks(&table, hash, None);
    }
}
