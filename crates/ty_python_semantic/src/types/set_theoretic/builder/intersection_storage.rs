//! Storage history and primitive mutations of signed intersection elements.

use std::hash::{Hash, Hasher};

use super::InnerIntersectionBuilder;
use super::intersection_insertion::Sign;
use crate::FxOrderSet;
use crate::types::{NegativeIntersectionElements, Type};

#[cfg(any(test, feature = "experimental-analysis"))]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct RetainedStorage {
    positive: RetainedSetStorage,
    negative: RetainedSetStorage,
}

#[cfg(any(test, feature = "experimental-analysis"))]
#[derive(Clone, Copy, Debug, Default)]
struct RetainedSetStorage {
    table_slots: usize,
    dense_capacity: usize,
    max_inline_bytes: usize,
}

/// Finite bounds for one signed collection; `capacity` is only a growth lower bound.
#[cfg(any(test, feature = "experimental-analysis"))]
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct SignedSetStorage {
    pub len: usize,
    pub capacity: usize,
    pub table_slots: usize,
    pub dense_capacity: usize,
    pub max_inline_bytes: usize,
    pub has_table: bool,
}

#[cfg(any(test, feature = "experimental-analysis"))]
fn table_slots(population: usize) -> Option<usize> {
    if population == 0 {
        return Some(0);
    }
    // Indexmap's index table uses hashbrown's 7/8 load factor. Growth after removals
    // either rehashes in place or requires the requested population to exceed half
    // the old full capacity before doubling its buckets. Eight slots per requested
    // element bound that rounding; the extra slots cover control bytes and padding.
    population.checked_mul(8)?.checked_add(32)
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl SignedSetStorage {
    pub(in crate::types) fn insertion_bound(self, ty: Type<'_>) -> Option<Self> {
        let required = self.len.checked_add(1)?;
        let has_table = self.has_table || self.len != 0;
        let table_slots = if has_table {
            self.table_slots.max(table_slots(required)?)
        } else {
            0
        };
        Some(Self {
            len: required,
            capacity: self.capacity,
            table_slots,
            // A new dense allocation can grow to the retained table's capacity,
            // including when an allocated empty table was cloned without entries.
            dense_capacity: self.dense_capacity.max(table_slots),
            max_inline_bytes: self.max_inline_bytes.max(ty.inline_payload_bytes()),
            has_table,
        })
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl RetainedSetStorage {
    fn inserted(&mut self, requested: usize, capacity: usize, has_table: bool, ty: Type<'_>) {
        if has_table {
            // Retain a bound derived from the actual requested population, never
            // repeatedly double an earlier pessimistic allocation quotation.
            self.table_slots = self
                .table_slots
                .max(table_slots(requested).unwrap_or(usize::MAX));
            self.dense_capacity = self.dense_capacity.max(capacity);
        }
        self.max_inline_bytes = self.max_inline_bytes.max(ty.inline_payload_bytes());
    }

    fn snapshot(self, len: usize, capacity: usize, has_table: bool) -> SignedSetStorage {
        SignedSetStorage {
            len,
            capacity,
            table_slots: self.table_slots,
            dense_capacity: self.dense_capacity,
            max_inline_bytes: self.max_inline_bytes,
            has_table,
        }
    }

    fn shrunk(&mut self, len: usize, has_table: bool) {
        self.table_slots = if has_table {
            table_slots(len).unwrap_or(usize::MAX)
        } else {
            0
        };
        if len == 0 {
            self.dense_capacity = 0;
        }
        // A nonempty Vec shrink is permitted to retain spare capacity. Keep its
        // previous dense bound until the owner is replaced or cloned.
    }
}

impl<'db> Clone for InnerIntersectionBuilder<'db> {
    fn clone(&self) -> Self {
        let positive = self.positive.clone();
        let negative = self.negative.clone();
        #[cfg(any(test, feature = "experimental-analysis"))]
        let storage = {
            let mut storage = self.storage;
            // Indexmap clones the entire index table, even for zero entries. Its
            // new dense Vec starts empty and reserves against the cloned table's
            // available capacity, so public capacity describes that new Vec.
            storage.positive.dense_capacity = positive.capacity();
            storage.negative.dense_capacity = match &negative {
                NegativeIntersectionElements::Multiple(set) => set.capacity(),
                _ => 0,
            };
            storage
        };
        Self {
            positive,
            negative,
            #[cfg(any(test, feature = "experimental-analysis"))]
            storage,
        }
    }
}

impl PartialEq for InnerIntersectionBuilder<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.positive == other.positive && self.negative == other.negative
    }
}

impl Eq for InnerIntersectionBuilder<'_> {}

impl Hash for InnerIntersectionBuilder<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.positive.hash(state);
        self.negative.hash(state);
    }
}

impl<'db> InnerIntersectionBuilder<'db> {
    pub(in crate::types) fn signed_lengths(&self) -> (usize, usize) {
        (self.positive.len(), self.negative.len())
    }

    pub(in crate::types) fn signed_parts(
        &self,
    ) -> (&FxOrderSet<Type<'db>>, &NegativeIntersectionElements<'db>) {
        (&self.positive, &self.negative)
    }

    pub(in crate::types) fn into_parts(
        self,
    ) -> (FxOrderSet<Type<'db>>, NegativeIntersectionElements<'db>) {
        (self.positive, self.negative)
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn signed_storage(&self, sign: Sign) -> SignedSetStorage {
        match sign {
            Sign::Positive => {
                self.storage
                    .positive
                    .snapshot(self.positive.len(), self.positive.capacity(), true)
            }
            Sign::Negative => {
                let (capacity, has_table) = match &self.negative {
                    NegativeIntersectionElements::Multiple(set) => (set.capacity(), true),
                    _ => (0, false),
                };
                self.storage
                    .negative
                    .snapshot(self.negative.len(), capacity, has_table)
            }
        }
    }

    pub(in crate::types) fn contains_signed(&self, sign: Sign, ty: Type<'db>) -> bool {
        match sign {
            Sign::Positive => self.positive.contains(&ty),
            Sign::Negative => self.negative.contains(&ty),
        }
    }

    pub(in crate::types) fn insert_signed(&mut self, sign: Sign, ty: Type<'db>) {
        #[cfg(any(test, feature = "experimental-analysis"))]
        let requested = match sign {
            Sign::Positive => self.positive.len(),
            Sign::Negative => self.negative.len(),
        }
        .saturating_add(1);
        match sign {
            Sign::Positive => {
                self.positive.insert(ty);
                #[cfg(any(test, feature = "experimental-analysis"))]
                self.storage
                    .positive
                    .inserted(requested, self.positive.capacity(), true, ty);
            }
            Sign::Negative => {
                self.negative.insert(ty);
                #[cfg(any(test, feature = "experimental-analysis"))]
                {
                    let (capacity, has_table) = match &self.negative {
                        NegativeIntersectionElements::Multiple(set) => (set.capacity(), true),
                        _ => (0, false),
                    };
                    self.storage
                        .negative
                        .inserted(requested, capacity, has_table, ty);
                }
            }
        }
    }

    pub(in crate::types) fn remove_signed(&mut self, sign: Sign, ty: Type<'db>) -> bool {
        match sign {
            Sign::Positive => self.positive.swap_remove(&ty),
            Sign::Negative => self.negative.swap_remove(&ty),
        }
    }

    pub(in crate::types) fn remove_signed_index(&mut self, sign: Sign, index: usize) {
        match sign {
            Sign::Positive => {
                self.positive.swap_remove_index(index);
            }
            Sign::Negative => {
                self.negative.swap_remove_index(index);
            }
        }
    }

    pub(in crate::types) fn reset_to(&mut self, ty: Type<'db>) {
        *self = Self::default();
        self.insert_signed(Sign::Positive, ty);
    }

    pub(in crate::types) fn shrink_signed(&mut self) {
        self.positive.shrink_to_fit();
        self.negative.shrink_to_fit();
        #[cfg(any(test, feature = "experimental-analysis"))]
        {
            self.storage.positive.shrunk(self.positive.len(), true);
            self.storage.negative.shrunk(
                self.negative.len(),
                matches!(self.negative, NegativeIntersectionElements::Multiple(_)),
            );
        }
    }

    pub(in crate::types) fn next_signed(
        &self,
        sign: Sign,
        cursor: &mut usize,
    ) -> Option<(usize, Type<'db>)> {
        let ty = match sign {
            Sign::Positive => self.positive.get_index(*cursor),
            Sign::Negative => match &self.negative {
                NegativeIntersectionElements::Empty => None,
                NegativeIntersectionElements::Single(ty) => (*cursor == 0).then_some(ty),
                NegativeIntersectionElements::Multiple(set) => set.get_index(*cursor),
            },
        };
        ty.map(|ty| {
            let index = *cursor;
            *cursor += 1;
            (index, *ty)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::hash::{Hash, Hasher};

    use rustc_hash::FxHasher;

    use super::InnerIntersectionBuilder;
    use crate::types::set_theoretic::builder::intersection_insertion::{Frame, Insertion, Sign};
    use crate::types::{Type, todo_type};

    #[test]
    fn allocated_empty_clone_retains_table_history() {
        for sign in [Sign::Positive, Sign::Negative] {
            let mut builder = InnerIntersectionBuilder::default();
            for value in 0..32 {
                builder.insert_signed(sign, Type::int_literal(value));
            }
            let allocated = builder.signed_storage(sign);
            for value in 0..32 {
                assert!(builder.remove_signed(sign, Type::int_literal(value)));
            }
            let empty = builder.signed_storage(sign);
            assert_eq!(empty.len, 0);
            assert_eq!(empty.table_slots, allocated.table_slots);
            assert_eq!(empty.dense_capacity, allocated.dense_capacity);

            let mut cloned = builder.clone();
            let storage = cloned.signed_storage(sign);
            assert!(storage.has_table);
            assert_eq!(storage.capacity, 0);
            assert_eq!(storage.dense_capacity, 0);
            assert_eq!(storage.table_slots, allocated.table_slots);
            cloned.insert_signed(sign, Type::unknown());
            assert!(cloned.signed_storage(sign).dense_capacity > 0);
        }
    }

    #[test]
    fn insertion_bounds_cover_growth_removal_clone_and_reset() -> Result<(), &'static str> {
        for sign in [Sign::Positive, Sign::Negative] {
            let mut builder = InnerIntersectionBuilder::default();
            for value in 0..64 {
                let ty = Type::int_literal(value);
                let bound = builder
                    .signed_storage(sign)
                    .insertion_bound(ty)
                    .ok_or("finite insertion bound")?;
                builder.insert_signed(sign, ty);
                let stored = builder.signed_storage(sign);
                assert!(stored.table_slots <= bound.table_slots);
                assert!(stored.dense_capacity <= bound.dense_capacity);
            }
            for _ in 0..63 {
                builder.remove_signed_index(sign, 0);
            }
            builder.insert_signed(sign, Type::int_literal(63));
            let before_duplicates = builder.signed_storage(sign);
            for _ in 0..64 {
                builder.insert_signed(sign, Type::int_literal(63));
            }
            assert_eq!(
                builder.signed_storage(sign).table_slots,
                before_duplicates.table_slots,
            );

            let mut cloned = builder.clone();
            cloned.shrink_signed();
            assert!(cloned.signed_storage(sign).table_slots <= before_duplicates.table_slots);
            cloned.reset_to(Type::Never);
            assert_eq!(cloned.signed_storage(Sign::Positive).len, 1);
            assert_eq!(cloned.signed_storage(Sign::Negative).len, 0);
            assert_eq!(cloned.signed_storage(Sign::Negative).table_slots, 0);
            assert_eq!(cloned.signed_storage(Sign::Negative).dense_capacity, 0);
        }
        Ok(())
    }

    #[test]
    fn storage_history_does_not_affect_ordered_identity() {
        let mut retained = InnerIntersectionBuilder::default();
        for sign in [Sign::Positive, Sign::Negative] {
            retained.insert_signed(sign, Type::Never);
            retained.insert_signed(sign, Type::unknown());
            retained.remove_signed(sign, Type::unknown());
        }
        let mut fresh = InnerIntersectionBuilder::default();
        for sign in [Sign::Positive, Sign::Negative] {
            fresh.insert_signed(sign, Type::Never);
        }
        assert_eq!(retained, fresh);
        let mut retained_hash = FxHasher::default();
        let mut fresh_hash = FxHasher::default();
        retained.hash(&mut retained_hash);
        fresh.hash(&mut fresh_hash);
        assert_eq!(retained_hash.finish(), fresh_hash.finish());
    }

    #[test]
    fn resident_inline_payload_survives_removal_and_clone() -> Result<(), &'static str> {
        let short = todo_type!("short");
        let long = todo_type!(
            "an inline Type payload whose bytes are visited by hashing and equality even though all interned semantic handles remain unvisited"
        );
        let mut builder = InnerIntersectionBuilder::default();
        for sign in [Sign::Positive, Sign::Negative] {
            builder.insert_signed(sign, long);
            builder.insert_signed(sign, short);
            builder.remove_signed(sign, long);
            let stored = builder.clone().signed_storage(sign);
            assert_eq!(stored.max_inline_bytes, long.inline_payload_bytes());
            let bound = stored
                .insertion_bound(short)
                .ok_or("finite payload bound")?;
            assert_eq!(bound.max_inline_bytes, long.inline_payload_bytes());
            if cfg!(debug_assertions) {
                assert!(bound.max_inline_bytes > short.inline_payload_bytes());
            }
        }
        Ok(())
    }

    #[test]
    fn insertion_buffers_retain_spilled_backing() {
        let mut builder = InnerIntersectionBuilder::default();
        let mut insertion =
            Insertion::new(&mut builder, Frame::Add(Type::unknown(), Sign::Positive));
        for index in 0..8 {
            insertion.push_frame(Frame::Add(Type::int_literal(index as i64), Sign::Positive));
            insertion.defer_removal(index);
        }
        let frame_capacity = insertion.frames_storage().1;
        let removal_capacity = insertion.removals_storage().1;
        assert!(insertion.frames_storage().2);
        assert!(insertion.removals_storage().2);
        while insertion.next_frame().is_some() {}
        while insertion.next_removal().is_some() {}
        assert_eq!(insertion.frames_storage(), (0, frame_capacity, true));
        assert_eq!(insertion.removals_storage(), (0, removal_capacity, true));
    }
}
