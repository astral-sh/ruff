//! Ordered distribution storage and finite profiles of its owned inner builders.

use smallvec::SmallVec;

use super::InnerIntersectionBuilder;
#[cfg(any(test, feature = "experimental-analysis"))]
use super::intersection_insertion::Sign;
#[cfg(any(test, feature = "experimental-analysis"))]
use super::intersection_storage::SignedSetStorage;
use crate::FxIndexSet;
#[cfg(any(test, feature = "experimental-analysis"))]
use crate::types::Type;

#[cfg(any(test, feature = "experimental-analysis"))]
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct InnerProfile {
    pub key_work: usize,
    pub backing_bytes: usize,
    pub retirement_work: usize,
    pub clone_work: usize,
    pub clone_bytes: usize,
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl InnerProfile {
    const OVERFLOW: Self = Self {
        key_work: usize::MAX,
        backing_bytes: usize::MAX,
        retirement_work: usize::MAX,
        clone_work: usize::MAX,
        clone_bytes: usize::MAX,
    };
}

#[cfg(any(test, feature = "experimental-analysis"))]
fn signed_backing(storage: SignedSetStorage) -> Option<usize> {
    storage
        .table_slots
        .checked_mul(size_of::<usize>().checked_add(1)?)?
        .checked_add(
            storage
                .dense_capacity
                .max(storage.table_slots)
                .checked_mul(size_of::<(usize, Type<'_>)>())?,
        )
}

#[cfg(any(test, feature = "experimental-analysis"))]
fn signed_clone_bytes(storage: SignedSetStorage) -> Option<usize> {
    // Cloning copies the retained index table. Its dense allocation can reserve
    // against that table even when the source's public capacity is smaller.
    storage
        .table_slots
        .checked_mul(size_of::<usize>().checked_add(1)?)?
        .checked_add(
            storage
                .table_slots
                .checked_mul(size_of::<(usize, Type<'_>)>())?,
        )
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn inner_profile(
    builder: &InnerIntersectionBuilder<'_>,
) -> Option<InnerProfile> {
    let positive = builder.signed_storage(Sign::Positive);
    let negative = builder.signed_storage(Sign::Negative);
    let key_work = positive
        .len
        .checked_mul(size_of::<Type<'_>>().checked_add(positive.max_inline_bytes)?)?
        .checked_add(
            negative
                .len
                .checked_mul(size_of::<Type<'_>>().checked_add(negative.max_inline_bytes)?)?,
        )?
        .checked_add(size_of::<InnerIntersectionBuilder<'_>>())?
        .checked_add(8)?;
    let backing_bytes = signed_backing(positive)?.checked_add(signed_backing(negative)?)?;
    let retirement_work = backing_bytes
        .checked_add(
            positive
                .len
                .checked_add(negative.len)?
                .checked_mul(size_of::<Type<'_>>())?,
        )?
        .checked_add(size_of::<InnerIntersectionBuilder<'_>>())?
        .checked_add(8)?;
    let clone_bytes = signed_clone_bytes(positive)?.checked_add(signed_clone_bytes(negative)?)?;
    let clone_work = clone_bytes
        .checked_mul(2)?
        .checked_add(backing_bytes)?
        .checked_add(key_work)?
        .checked_add(retirement_work)?;
    Some(InnerProfile {
        key_work,
        backing_bytes,
        retirement_work,
        clone_work,
        clone_bytes,
    })
}

#[cfg(any(test, feature = "experimental-analysis"))]
fn table_slots(population: usize) -> Option<usize> {
    if population == 0 {
        Some(0)
    } else {
        population.checked_mul(8)?.checked_add(32)
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
#[derive(Default)]
struct RetainedDistributionStorage {
    table_slots: usize,
    dense_capacity: usize,
    max_key_work: usize,
    inner_retirement_work: usize,
}

#[cfg(any(test, feature = "experimental-analysis"))]
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct DistributionStorage {
    pub len: usize,
    pub capacity: usize,
    pub table_slots: usize,
    pub dense_capacity: usize,
    pub max_key_work: usize,
    pub inner_retirement_work: usize,
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl DistributionStorage {
    pub(in crate::types) fn insertion_bound(self, incoming: InnerProfile) -> Option<Self> {
        let len = self.len.checked_add(1)?;
        let table_slots = self.table_slots.max(table_slots(len)?);
        Some(Self {
            len,
            capacity: self.capacity,
            table_slots,
            dense_capacity: self.dense_capacity.max(table_slots),
            max_key_work: self.max_key_work.max(incoming.key_work),
            inner_retirement_work: self
                .inner_retirement_work
                .checked_add(incoming.retirement_work)?,
        })
    }

    pub(in crate::types) fn backing_bytes(self) -> Option<usize> {
        self.table_slots
            .checked_mul(size_of::<usize>().checked_add(1)?)?
            .checked_add(
                self.dense_capacity
                    .checked_mul(size_of::<(usize, InnerIntersectionBuilder<'_>)>())?,
            )
    }

    pub(in crate::types) fn retirement_work(self) -> Option<usize> {
        self.backing_bytes()?
            .checked_add(self.inner_retirement_work)?
            .checked_add(size_of::<DistributionSet<'_>>())?
            .checked_add(4)
    }
}

#[derive(Default)]
pub(in crate::types) struct DistributionSet<'db> {
    values: FxIndexSet<InnerIntersectionBuilder<'db>>,
    #[cfg(any(test, feature = "experimental-analysis"))]
    storage: RetainedDistributionStorage,
}

impl<'db> DistributionSet<'db> {
    pub(in crate::types) fn len(&self) -> usize {
        self.values.len()
    }

    pub(in crate::types) fn get(&self, index: usize) -> Option<&InnerIntersectionBuilder<'db>> {
        self.values.get_index(index)
    }

    pub(in crate::types) fn next(
        &self,
        cursor: &mut usize,
    ) -> Option<(usize, &InnerIntersectionBuilder<'db>)> {
        let value = self.get(*cursor)?;
        let index = *cursor;
        *cursor += 1;
        Some((index, value))
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn storage(&self) -> DistributionStorage {
        DistributionStorage {
            len: self.values.len(),
            capacity: self.values.capacity(),
            table_slots: self.storage.table_slots,
            dense_capacity: self.storage.dense_capacity,
            max_key_work: self.storage.max_key_work,
            inner_retirement_work: self.storage.inner_retirement_work,
        }
    }

    pub(in crate::types) fn insert(&mut self, value: InnerIntersectionBuilder<'db>) -> bool {
        #[cfg(any(test, feature = "experimental-analysis"))]
        let requested = self.values.len().saturating_add(1);
        #[cfg(any(test, feature = "experimental-analysis"))]
        let profile = inner_profile(&value).unwrap_or(InnerProfile::OVERFLOW);
        let inserted = self.values.insert(value);
        #[cfg(any(test, feature = "experimental-analysis"))]
        {
            self.storage.table_slots = self
                .storage
                .table_slots
                .max(table_slots(requested).unwrap_or(usize::MAX));
            // Public capacity is the minimum of the table and dense capacities.
            // Keep the population-derived bound for dense allocation rounding too.
            self.storage.dense_capacity = self
                .storage
                .dense_capacity
                .max(self.storage.table_slots)
                .max(self.values.capacity());
            self.storage.max_key_work = self.storage.max_key_work.max(profile.key_work);
            if inserted {
                self.storage.inner_retirement_work = self
                    .storage
                    .inner_retirement_work
                    .saturating_add(profile.retirement_work);
            }
        }
        inserted
    }

    pub(in crate::types) fn apply_removals(&mut self, removals: RemovalIndices) {
        let mut removals = removals.indices.into_iter().peekable();
        let mut index = 0;
        self.values.retain(|value| {
            let remove = removals.peek().is_some_and(|next| *next == index);
            if remove {
                removals.next();
                #[cfg(any(test, feature = "experimental-analysis"))]
                {
                    // Once an aggregate overflows, keep it unavailable rather than
                    // subtracting from the sentinel and exposing an incomplete bound.
                    if self.storage.inner_retirement_work != usize::MAX {
                        self.storage.inner_retirement_work =
                            self.storage.inner_retirement_work.saturating_sub(
                                inner_profile(value)
                                    .unwrap_or(InnerProfile::OVERFLOW)
                                    .retirement_work,
                            );
                    }
                }
            }
            #[cfg(not(any(test, feature = "experimental-analysis")))]
            let _ = value;
            index += 1;
            !remove
        });
    }

    pub(in crate::types) fn into_intersections(self) -> Vec<InnerIntersectionBuilder<'db>> {
        self.values.into_iter().collect()
    }
}

/// Indexes recorded in visitation order before the distribution is compacted.
#[derive(Default)]
pub(in crate::types) struct RemovalIndices {
    indices: SmallVec<[usize; 4]>,
}

impl RemovalIndices {
    pub(in crate::types) fn push(&mut self, index: usize) {
        self.indices.push(index);
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn storage(&self) -> (usize, usize, bool) {
        (
            self.indices.len(),
            self.indices.capacity(),
            self.indices.spilled(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{DistributionSet, InnerIntersectionBuilder, RemovalIndices, inner_profile};
    use crate::types::set_theoretic::builder::intersection_insertion::Sign;
    use crate::types::{Type, todo_type};

    #[test]
    fn removal_preserves_order_and_backing_with_remaining_payload() -> Result<(), &'static str> {
        let mut distributed = DistributionSet::default();
        let mut removed_work = 0;
        let mut removals = RemovalIndices::default();
        for index in 0..12 {
            let mut inner = InnerIntersectionBuilder::default();
            inner.insert_signed(Sign::Positive, Type::int_literal(index));
            if index % 2 == 0 {
                removed_work += inner_profile(&inner)
                    .ok_or("finite inner profile")?
                    .retirement_work;
                removals.push(index as usize);
            }
            assert!(distributed.insert(inner));
        }
        assert!(removals.storage().2);
        let before = distributed.storage();
        distributed.apply_removals(removals);
        let after = distributed.storage();
        assert_eq!(after.len, 6);
        assert_eq!(after.table_slots, before.table_slots);
        assert_eq!(after.dense_capacity, before.dense_capacity);
        assert_eq!(after.max_key_work, before.max_key_work);
        assert_eq!(
            after.inner_retirement_work,
            before.inner_retirement_work - removed_work
        );
        for (index, inner) in distributed.into_intersections().into_iter().enumerate() {
            assert!(
                inner.contains_signed(Sign::Positive, Type::int_literal((index * 2 + 1) as i64))
            );
        }
        Ok(())
    }

    #[test]
    fn duplicate_does_not_add_its_retained_payload_to_the_distribution() -> Result<(), &'static str>
    {
        let mut resident = InnerIntersectionBuilder::default();
        resident.insert_signed(Sign::Positive, Type::int_literal(0));
        let mut duplicate = resident.clone();
        for value in 1..32 {
            duplicate.insert_signed(Sign::Positive, Type::int_literal(value));
        }
        for value in 1..32 {
            assert!(duplicate.remove_signed(Sign::Positive, Type::int_literal(value)));
        }
        assert_eq!(resident, duplicate);
        let incoming = inner_profile(&duplicate).ok_or("finite duplicate profile")?;
        let mut distributed = DistributionSet::default();
        assert!(distributed.insert(resident));
        let before = distributed.storage();
        assert!(incoming.retirement_work > before.inner_retirement_work);
        let bound = before
            .insertion_bound(incoming)
            .ok_or("finite insertion bound")?;
        assert!(!distributed.insert(duplicate));
        let after = distributed.storage();
        assert_eq!(after.len, 1);
        assert_eq!(after.inner_retirement_work, before.inner_retirement_work);
        assert!(after.table_slots <= bound.table_slots);
        assert!(after.dense_capacity <= bound.dense_capacity);
        assert!(
            after.retirement_work().ok_or("finite retirement")?
                <= bound.retirement_work().ok_or("finite bound retirement")?
        );
        Ok(())
    }

    #[test]
    fn whole_inner_profile_preserves_inline_payload_and_empty_table_backing()
    -> Result<(), &'static str> {
        let short = todo_type!("short");
        let long = todo_type!(
            "an inline Type payload visited by whole-inner distribution hashing and equality, even though semantic handles are not followed"
        );
        let mut inner = InnerIntersectionBuilder::default();
        inner.insert_signed(Sign::Positive, short);
        let short_profile = inner_profile(&inner).ok_or("finite short profile")?;
        inner.insert_signed(Sign::Positive, long);
        if cfg!(debug_assertions) {
            assert!(inner.remove_signed(Sign::Positive, long));
            assert!(
                inner_profile(&inner)
                    .ok_or("finite retained profile")?
                    .key_work
                    > short_profile.key_work
            );
        }
        assert!(inner.remove_signed(Sign::Positive, short));
        let empty = inner_profile(&inner).ok_or("finite empty profile")?;
        assert!(empty.backing_bytes > 0);
        assert!(empty.clone_bytes > 0);
        assert!(empty.clone_work >= empty.clone_bytes);
        let signed = inner.signed_storage(Sign::Positive);
        assert!(empty.backing_bytes >= signed.table_slots * size_of::<(usize, Type<'_>)>());
        Ok(())
    }
}
