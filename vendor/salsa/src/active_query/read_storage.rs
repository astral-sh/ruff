use std::alloc::Layout;

use super::ActiveQuery;
#[cfg(all(test, feature = "accumulator"))]
use crate::accumulator::accumulated_map::InputAccumulatedValues;
use crate::cycle::CycleHead;
#[cfg(test)]
use crate::cycle::IterationStamp;
use crate::zalsa_local::QueryEdge;
#[cfg(test)]
use crate::{DatabaseKeyIndex, Durability, Revision};

/// The largest control group in the pinned hashbrown implementation, including its SIMD targets.
const HASH_GROUP_WIDTH: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StorageShape {
    len: usize,
    capacity: usize,
}

impl StorageShape {
    fn target(self, additional: usize) -> Option<usize> {
        if self.len > self.capacity {
            return None;
        }
        let required = self.len.checked_add(additional)?;
        if required <= self.capacity {
            Some(self.capacity)
        } else {
            Some(required.max(self.capacity.checked_mul(2)?).max(4))
        }
    }
}

/// Scalar metadata that can be captured without scanning dependencies or allocating storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReadStorageFootprint {
    edges: StorageShape,
    cycle_heads: StorageShape,
}

impl ReadStorageFootprint {
    /// Includes work for the read and for clearing the storage when its query retires.
    /// The caller admits this quote before reserving or updating the active query.
    pub(crate) fn prepare(
        self,
        record_input: bool,
        incoming_cycle_heads: usize,
    ) -> Option<ReadStoragePlan> {
        let edge_target = self.edges.target(usize::from(record_input))?;
        let cycle_target = self.cycle_heads.target(incoming_cycle_heads)?;

        // Capture and recheck the four shape fields, compare the two reservation bounds, and
        // update durability, revision and the accumulator flag. Each is a scalar operation.
        let mut units = 4usize.checked_mul(2)?.checked_add(2)?.checked_add(3)?;
        let mut requested_bytes = 0usize;

        if record_input {
            units = units.checked_add(hash_probe_work(self.edges.len)?)?;
            // Hash two key words, append three edge words and an index/hash, then retire them.
            units = units.checked_add(2 + 3 + 2 + 5)?;

            if self.edges.len == 0 {
                // Clearing a reused frame writes its entire retained control array. Empty
                // tables skip that traversal, so its first possible insertion pays for it.
                units = units.checked_add(retained_buckets(self.edges.capacity)?)?;
                units = units.checked_add(HASH_GROUP_WIDTH)?;
            }

            if edge_target > self.edges.capacity {
                let buckets = hash_buckets(edge_target)?;
                let table_bytes = hash_allocation_bytes(buckets)?;
                let entries_bytes = Layout::array::<(usize, QueryEdge)>(edge_target)
                    .ok()?
                    .size();
                requested_bytes = table_bytes.checked_add(entries_bytes)?;

                // Rebuilding scans the old control array and reinserts every occupied index.
                // A collision can make every reinsertion probe all preceding entries. The
                // ordered vector's reallocation may copy its complete old allocation.
                units = units
                    .checked_add(retained_buckets(self.edges.capacity)?)?
                    .checked_add(
                        self.edges
                            .len
                            .checked_mul(hash_probe_work(self.edges.len)?)?,
                    )?
                    .checked_add(
                        retained_entries(self.edges.capacity)?.checked_mul(size_of::<(
                            usize,
                            QueryEdge,
                        )>(
                        ))?,
                    )?
                    // Initialize the new control array and pay for its eventual clearing.
                    .checked_add(buckets.checked_add(HASH_GROUP_WIDTH)?.checked_mul(2)?)?;
            }
        }

        if incoming_cycle_heads != 0 {
            let comparisons = incoming_cycle_heads
                .checked_mul(self.cycle_heads.len)?
                .checked_add(triangular(incoming_cycle_heads)?)?;
            // Scan removed source slots too. Each destination comparison examines a three-word
            // key; each incoming head also loads its flags/stamp, updates or appends a head,
            // and contributes passive retirement work.
            units = units
                .checked_add(comparisons.checked_mul(3)?)?
                .checked_add(incoming_cycle_heads.checked_mul(1 + 2 + 5 + 5)?)?;

            if cycle_target > self.cycle_heads.capacity {
                let header = Layout::new::<[usize; 2]>();
                let heads = Layout::array::<CycleHead>(cycle_target).ok()?;
                let (allocation, _) = header.extend(heads).ok()?;
                requested_bytes = requested_bytes.checked_add(allocation.size())?;
                units = units
                    .checked_add(
                        self.cycle_heads
                            .capacity
                            .checked_mul(size_of::<CycleHead>())?,
                    )?
                    .checked_add(header.size())?;
            }
        }

        Some(ReadStoragePlan {
            footprint: self,
            edge_target,
            cycle_target,
            work: ReadStorageWork {
                units,
                requested_bytes,
            },
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReadStorageWork {
    pub(crate) units: usize,
    pub(crate) requested_bytes: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ReadStoragePlan {
    footprint: ReadStorageFootprint,
    edge_target: usize,
    cycle_target: usize,
    work: ReadStorageWork,
}

impl ReadStoragePlan {
    pub(crate) fn work(&self) -> ReadStorageWork {
        self.work
    }
}

#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReadState {
    footprint: ReadStorageFootprint,
    edges: Vec<QueryEdge>,
    durability: Durability,
    changed_at: Revision,
    cycle_heads: Vec<(DatabaseKeyIndex, IterationStamp, bool)>,
    #[cfg(feature = "accumulator")]
    accumulated_inputs: InputAccumulatedValues,
}

impl ActiveQuery {
    #[cfg(test)]
    pub(crate) fn read_state(&self) -> ReadState {
        ReadState {
            footprint: self.read_storage_footprint(),
            edges: self.input_outputs.iter().copied().collect(),
            durability: self.durability,
            changed_at: self.changed_at,
            // Owning iteration includes removed slots; the final flag records whether each
            // physical slot is visible through the normal cycle-head iterator.
            cycle_heads: self
                .cycle_heads
                .clone()
                .into_iter()
                .map(|head| {
                    (
                        head.database_key_index,
                        head.iteration.load(),
                        self.cycle_heads.contains(&head.database_key_index),
                    )
                })
                .collect(),
            #[cfg(feature = "accumulator")]
            accumulated_inputs: self.accumulated_inputs,
        }
    }

    pub(crate) fn read_storage_footprint(&self) -> ReadStorageFootprint {
        ReadStorageFootprint {
            edges: StorageShape {
                len: self.input_outputs.len(),
                capacity: self.input_outputs.capacity(),
            },
            cycle_heads: StorageShape {
                len: self.cycle_heads.storage_len(),
                capacity: self.cycle_heads.storage_capacity(),
            },
        }
    }

    /// Reserves an admitted plan without callbacks. A stale footprint makes no storage request.
    /// The caller retains the recipient's owner and applies the read without intervening callouts.
    pub(crate) fn reserve_read_storage(&mut self, plan: &ReadStoragePlan) -> bool {
        if self.read_storage_footprint() != plan.footprint {
            return false;
        }
        if plan.edge_target > plan.footprint.edges.capacity {
            self.input_outputs
                .reserve_exact(plan.edge_target - plan.footprint.edges.len);
        }
        if plan.cycle_target > plan.footprint.cycle_heads.capacity {
            self.cycle_heads
                .reserve_additional(plan.cycle_target - plan.footprint.cycle_heads.len);
        }
        true
    }
}

/// Indexmap 2.14 stores a dense `(usize hash, QueryEdge, ())` vector and a hashbrown 0.17
/// table of `usize` indices. `reserve_exact` reserves both, including on duplicate reads.
/// Quoting both whole allocations avoids depending on which backing has spare capacity.
fn hash_buckets(capacity: usize) -> Option<usize> {
    match capacity {
        0 => Some(0),
        1..=3 => Some(4),
        4..=7 => Some(8),
        8..=14 => Some(16),
        _ => (capacity.checked_mul(8)? / 7).checked_next_power_of_two(),
    }
}

fn hash_allocation_bytes(buckets: usize) -> Option<usize> {
    let indices = Layout::array::<usize>(buckets).ok()?;
    let controls =
        Layout::from_size_align(buckets.checked_add(HASH_GROUP_WIDTH)?, HASH_GROUP_WIDTH).ok()?;
    let (allocation, _) = indices.extend(controls).ok()?;
    Some(allocation.size())
}

/// Active-query sets insert, clear, drain, reserve exactly, and transfer whole backing stores.
/// They never shrink or remove individual entries. The index table can outgrow the ordered
/// vector when insertion reserves before discovering a duplicate; one further entry therefore
/// bounds its retained bucket count even though IndexSet reports the smaller capacity.
fn retained_buckets(capacity: usize) -> Option<usize> {
    if capacity == 0 {
        Some(0)
    } else {
        hash_buckets(capacity.checked_add(1)?)
    }
}

fn retained_entries(capacity: usize) -> Option<usize> {
    // The public capacity can instead be the smaller hash-table capacity. Allow the vector's
    // geometric rounding as well as the extra entry that can trigger either backing to grow.
    if capacity == 0 {
        Some(0)
    } else {
        capacity.checked_add(1)?.checked_mul(2)
    }
}

fn hash_probe_work(entries: usize) -> Option<usize> {
    // Without deleted buckets, every unsuccessful group before the final group is occupied.
    // Bound the group loads separately from all possible three-word equality comparisons.
    entries
        .checked_add(1)?
        .checked_mul(HASH_GROUP_WIDTH)?
        .checked_add(entries.checked_mul(3)?)
}

fn triangular(count: usize) -> Option<usize> {
    let previous = count.saturating_sub(1);
    if count % 2 == 0 {
        (count / 2).checked_mul(previous)
    } else {
        count.checked_mul(previous / 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cycle::{CycleHeads, IterationStamp};
    #[cfg(not(feature = "shuttle"))]
    use crate::function::{Allocations, observe_allocations};
    use crate::{DatabaseKeyIndex, Durability, Id, IngredientIndex, Revision};

    fn key(index: u32) -> DatabaseKeyIndex {
        DatabaseKeyIndex::new(IngredientIndex::new(0), Id::from_bits(u64::from(index) + 1))
    }

    fn insert(query: &mut ActiveQuery, index: u32) {
        query.add_read_simple(key(index), Durability::LOW, Revision::start());
    }

    #[cfg(not(feature = "shuttle"))]
    fn measured_reservation(
        query: &mut ActiveQuery,
        record_input: bool,
        incoming_cycle_heads: usize,
    ) -> (ReadStoragePlan, Allocations) {
        let (plan, preparation) = observe_allocations(|| {
            query
                .read_storage_footprint()
                .prepare(record_input, incoming_cycle_heads)
                .unwrap()
        });
        assert_eq!(preparation.allocation_requests(), 0);
        let (reserved, allocations) = observe_allocations(|| query.reserve_read_storage(&plan));
        assert!(reserved);
        assert!(
            allocations.requested_bytes <= plan.work().requested_bytes,
            "reservation exceeded {plan:?}: {allocations:?}",
        );
        (plan, allocations)
    }

    #[cfg(not(feature = "shuttle"))]
    fn measured_simple_read(query: &mut ActiveQuery, index: u32) -> Allocations {
        let (_, reservation) = measured_reservation(query, true, 0);
        let ((), insertion) = observe_allocations(|| insert(query, index));
        assert_eq!(insertion.allocation_requests(), 0, "{insertion:?}");
        reservation
    }

    #[cfg(not(feature = "shuttle"))]
    fn measured_full_read(query: &mut ActiveQuery, incoming: &CycleHeads) -> Allocations {
        let (_, reservation) = measured_reservation(query, true, incoming.storage_len());
        let ((), insertion) = observe_allocations(|| {
            query.add_read_observed(
                key(100),
                Durability::LOW,
                Revision::start(),
                incoming,
                #[cfg(feature = "accumulator")]
                InputAccumulatedValues::Empty,
            );
        });
        assert_eq!(insertion.allocation_requests(), 0, "{insertion:?}");
        reservation
    }

    #[cfg(not(feature = "shuttle"))]
    #[test]
    fn allocation_quotes_cover_initial_growing_and_duplicate_edges() {
        let mut query = ActiveQuery::new(key(100));
        assert!(measured_simple_read(&mut query, 0).requested_bytes > 0);
        assert_eq!(measured_simple_read(&mut query, 0).requested_bytes, 0);
        let mut growing_duplicates = 0;
        for index in 1..128 {
            measured_simple_read(&mut query, index);
            if measured_simple_read(&mut query, index).requested_bytes != 0 {
                growing_duplicates += 1;
            }
        }
        assert!(growing_duplicates > 0);
        assert_eq!(query.input_outputs.len(), 128);
    }

    #[cfg(not(feature = "shuttle"))]
    #[test]
    fn allocation_quotes_cover_hidden_hash_growth_and_frame_reuse() {
        let mut query = ActiveQuery::new(key(100));
        for index in 0..3 {
            insert(&mut query, index);
        }
        let full = query.read_storage_footprint();
        assert_eq!(full.edges.len, full.edges.capacity);
        let ((), duplicate) = observe_allocations(|| insert(&mut query, 1));
        assert!(duplicate.requested_bytes > 0);
        assert_eq!(query.read_storage_footprint(), full);

        query.clear();
        query.reset_for(key(101));
        assert_eq!(measured_simple_read(&mut query, 0).requested_bytes, 0);
        for index in 1..32 {
            measured_simple_read(&mut query, index);
        }
        assert_eq!(query.input_outputs.len(), 32);
    }

    #[cfg(not(feature = "shuttle"))]
    #[test]
    fn allocation_quotes_cover_cycle_growth_duplicates_and_removed_slots() {
        let iteration = IterationStamp::initial(0);
        let mut query = ActiveQuery::new(key(200));
        let mut incoming = CycleHeads::default();
        for index in 0..4 {
            incoming.insert(key(index), iteration);
        }
        assert!(measured_full_read(&mut query, &incoming).requested_bytes > 0);
        assert_eq!(query.cycle_heads.storage_len(), 4);
        assert_eq!(query.cycle_heads.storage_capacity(), 4);

        assert!(measured_full_read(&mut query, &incoming).requested_bytes > 0);
        assert_eq!(query.cycle_heads.storage_len(), 4);
        query.cycle_heads.remove_all_except(key(0));
        incoming.remove_all_except(key(1));
        assert_eq!(measured_full_read(&mut query, &incoming).requested_bytes, 0);
        assert_eq!(query.read_state().cycle_heads.len(), 4);
        assert_eq!(query.cycle_heads.iter().count(), 2);

        for index in 4..12 {
            incoming.insert(key(index), iteration);
        }
        assert!(measured_full_read(&mut query, &incoming).requested_bytes > 0);
        assert_eq!(query.cycle_heads.storage_len(), 12);
        incoming.remove_all_except(key(999));
        let before = query.read_state();
        assert!(measured_full_read(&mut query, &incoming).requested_bytes > 0);
        assert_eq!(query.read_state().cycle_heads, before.cycle_heads);
    }

    #[test]
    fn duplicate_at_capacity_reserves_before_canonical_insertion() {
        let mut query = ActiveQuery::new(key(100));
        for index in 0..3 {
            insert(&mut query, index);
        }
        assert_eq!(query.input_outputs.len(), query.input_outputs.capacity());
        let plan = query.read_storage_footprint().prepare(true, 0).unwrap();
        assert!(plan.work().requested_bytes > 0);
        assert!(query.reserve_read_storage(&plan));
        let reserved = query.read_storage_footprint();
        insert(&mut query, 1);
        assert_eq!(query.read_storage_footprint(), reserved);
        assert_eq!(
            query.input_outputs.iter().copied().collect::<Vec<_>>(),
            [0, 1, 2].map(|index| QueryEdge::input(key(index))),
        );
    }

    #[test]
    fn stale_plan_does_not_reserve() {
        let mut query = ActiveQuery::new(key(100));
        let plan = query.read_storage_footprint().prepare(true, 0).unwrap();
        insert(&mut query, 0);
        let changed = query.read_storage_footprint();
        assert!(!query.reserve_read_storage(&plan));
        assert_eq!(query.read_storage_footprint(), changed);
    }

    #[test]
    fn geometric_growth_and_retained_backing() {
        let mut query = ActiveQuery::new(key(100));
        let mut growths = 0;
        for index in 0..128 {
            let before = query.read_storage_footprint();
            let plan = before.prepare(true, 0).unwrap();
            if plan.work().requested_bytes != 0 {
                growths += 1;
                assert!(plan.edge_target >= before.edges.capacity * 2);
            }
            assert!(query.reserve_read_storage(&plan));
            let reserved = query.input_outputs.capacity();
            insert(&mut query, index);
            assert_eq!(query.input_outputs.capacity(), reserved);
        }
        assert!(growths <= 6);
        query.clear();
        query.reset_for(key(101));
        let reused = query.read_storage_footprint().prepare(true, 0).unwrap();
        let fresh = ActiveQuery::new(key(102))
            .read_storage_footprint()
            .prepare(true, 0)
            .unwrap();
        assert_eq!(reused.work().requested_bytes, 0);
        assert!(reused.work().units > fresh.work().units);
    }

    #[test]
    fn revision_only_read_needs_no_dependency_storage() {
        let mut query = ActiveQuery::new(key(100));
        let original = query.read_storage_footprint();
        let plan = original.prepare(false, 0).unwrap();
        assert_eq!(plan.work().requested_bytes, 0);
        assert!(query.reserve_read_storage(&plan));
        query.add_changed_at(Revision::start());
        assert_eq!(query.read_storage_footprint(), original);
    }

    #[test]
    fn cycle_merge_accounts_for_removed_entries_and_preserves_order() {
        let iteration = IterationStamp::initial(0);
        let mut query = ActiveQuery::new(key(100));
        query.cycle_heads.insert(key(0), iteration);
        query.cycle_heads.insert(key(1), iteration);
        query.cycle_heads.remove_all_except(key(0));
        let mut incoming = CycleHeads::default();
        incoming.insert(key(1), iteration);
        incoming.insert(key(2), iteration);
        incoming.insert(key(3), iteration);
        incoming.remove_all_except(key(1));
        let footprint = query.read_storage_footprint();
        let plan = footprint.prepare(true, incoming.storage_len()).unwrap();
        let live_only = footprint.prepare(true, 1).unwrap();
        assert!(plan.work().units > live_only.work().units);
        assert!(query.reserve_read_storage(&plan));
        let capacity = query.cycle_heads.storage_capacity();
        query.cycle_heads.extend(&incoming);
        assert_eq!(query.cycle_heads.storage_capacity(), capacity);
        assert_eq!(query.cycle_heads.storage_len(), 2);
        assert_eq!(
            query.cycle_heads.ids().collect::<Vec<_>>(),
            [key(0).key_index(), key(1).key_index()],
        );
    }

    #[test]
    fn overflow_is_rejected_before_storage_requests() {
        let shape = ReadStorageFootprint {
            edges: StorageShape {
                len: 0,
                capacity: 0,
            },
            cycle_heads: StorageShape {
                len: 0,
                capacity: 0,
            },
        };
        assert!(shape.prepare(false, usize::MAX).is_none());
        assert!(hash_buckets(usize::MAX).is_none());
        assert!(hash_allocation_bytes(usize::MAX).is_none());
        assert!(triangular(usize::MAX).is_none());
        assert_eq!(triangular(0), Some(0));
        assert_eq!(triangular(3), Some(3));
    }
}
