//! Admission for the existing callable guard's collections and scope cleanup.
//!
//! A descendant can grow a set before an ancestor removes its entry. Each table therefore
//! retains its backing-capacity history and prepays the increased removal cost of every live
//! entry before growth. Scope receipts identify those obligations without freezing their cost.
//! Admission approves work and requested bytes before mutation. Preparation holds no
//! collection borrow across that callback, and commit rejects a changed collection or ledger.

use std::cell::RefCell;
use std::collections::hash_set;
use std::hash::Hash;
use std::mem::{needs_drop, size_of};

use rustc_hash::{FxBuildHasher, FxHashSet};

use super::{
    CallableExpansion, CallableRecursionGuard, CallableVisitScope, DefinitionUse,
    DescriptorDispatchScope, DescriptorDispatches, DescriptorOrigin, RecursiveDefinition, Type,
    TypeIdentity,
};
use crate::types::constraints::control::{GrowthPlan, sequence_growth};

#[cfg(test)]
pub(in crate::types) mod observations;
#[cfg(test)]
mod tests;

type ExactKey<'db> = (CallableExpansion, Type<'db>);
type IdentityKey<'db> = (CallableExpansion, TypeIdentity<'db>);
type DefinitionKey<'db> = (DefinitionUse<'db>, DescriptorOrigin<'db>);
type Anchor<'db> = (RecursiveDefinition<'db>, DescriptorDispatches<'db>);

#[derive(Debug)]
pub(super) struct CallableActiveSet<K> {
    pub(super) seen: RefCell<ActiveSet<K>>,
}

impl<K> Default for CallableActiveSet<K> {
    fn default() -> Self {
        Self {
            seen: RefCell::new(ActiveSet::Empty),
        }
    }
}

/// Keeps the first active key inline and retains hash storage after the first spill.
#[derive(Debug)]
pub(super) enum ActiveSet<K> {
    Empty,
    One(K),
    Spilled(FxHashSet<K>),
}

impl<K> ActiveSet<K> {
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::One(_) => 1,
            Self::Spilled(values) => values.len(),
        }
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn capacity(&self) -> usize {
        match self {
            Self::Empty | Self::One(_) => 0,
            Self::Spilled(values) => values.capacity(),
        }
    }

    fn layout(&self) -> SetLayout {
        match self {
            Self::Empty => SetLayout::Empty,
            Self::One(_) => SetLayout::One,
            Self::Spilled(_) => SetLayout::Spilled,
        }
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &K> {
        match self {
            Self::Empty => ActiveSetIter::Inline(None.into_iter()),
            Self::One(key) => ActiveSetIter::Inline(Some(key).into_iter()),
            Self::Spilled(values) => ActiveSetIter::Spilled(values.iter()),
        }
    }
}

impl<K: Copy + Eq + Hash> ActiveSet<K> {
    pub(super) fn contains(&self, key: &K) -> bool {
        match self {
            Self::Empty => false,
            Self::One(previous) => previous == key,
            Self::Spilled(values) => values.contains(key),
        }
    }

    fn insert(&mut self, key: K) -> bool {
        match self {
            Self::Empty => {
                *self = Self::One(key);
                true
            }
            Self::One(previous) => {
                // Reserve before comparing the candidate, even if it repeats the inline key.
                // Keeping the resulting table makes its removal funding and capacity history
                // valid for the ancestor that owns the inline key's receipt.
                let mut values = FxHashSet::with_capacity_and_hasher(2, FxBuildHasher::default());
                values.insert(*previous);
                let inserted = values.insert(key);
                *self = Self::Spilled(values);
                inserted
            }
            Self::Spilled(values) => values.insert(key),
        }
    }

    fn remove(&mut self, key: &K) -> bool {
        match self {
            Self::Empty => false,
            Self::One(previous) if previous == key => {
                *self = Self::Empty;
                true
            }
            Self::One(_) => false,
            Self::Spilled(values) => values.remove(key),
        }
    }
}

enum ActiveSetIter<'a, K> {
    Inline(std::option::IntoIter<&'a K>),
    Spilled(hash_set::Iter<'a, K>),
}

impl<'a, K> Iterator for ActiveSetIter<'a, K> {
    type Item = &'a K;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Inline(values) => values.next(),
            Self::Spilled(values) => values.next(),
        }
    }
}

#[derive(Clone, Copy)]
enum SetRef<'guard, K> {
    Active(&'guard RefCell<ActiveSet<K>>),
    Dependency(&'guard RefCell<FxHashSet<K>>),
}

impl<'guard, K> From<&'guard RefCell<ActiveSet<K>>> for SetRef<'guard, K> {
    fn from(values: &'guard RefCell<ActiveSet<K>>) -> Self {
        Self::Active(values)
    }
}

impl<'guard, K> From<&'guard RefCell<FxHashSet<K>>> for SetRef<'guard, K> {
    fn from(values: &'guard RefCell<FxHashSet<K>>) -> Self {
        Self::Dependency(values)
    }
}

impl<K: CallableGuardKey> SetRef<'_, K> {
    fn insert(self, key: K) -> (bool, usize) {
        match self {
            Self::Active(values) => {
                let mut values = values.borrow_mut();
                let inserted = values.insert(key);
                (inserted, values.capacity())
            }
            Self::Dependency(values) => {
                let mut values = values.borrow_mut();
                let inserted = values.insert(key);
                (inserted, values.capacity())
            }
        }
    }

    fn remove(self, key: &K) {
        match self {
            Self::Active(values) => {
                values.borrow_mut().remove(key);
            }
            Self::Dependency(values) => {
                values.borrow_mut().remove(key);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum CallableGuardTable {
    Exact,
    Identity,
    DefinitionDispatch,
    Dependencies,
}

impl CallableGuardTable {
    const fn index(self) -> usize {
        match self {
            Self::Exact => 0,
            Self::Identity => 1,
            Self::DefinitionDispatch => 2,
            Self::Dependencies => 3,
        }
    }

    const fn removes_entries(self) -> bool {
        !matches!(self, Self::Dependencies)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct CallableGuardStorageQuote {
    pub(in crate::types) work_units: usize,
    pub(in crate::types) requested_payload_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum CallableGuardRehash {
    None,
    Spill,
    InPlace,
    Replacement { requested_capacity: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum CallableGuardStorageWork {
    Owner(CallableGuardStorageQuote),
    EmptyLookup {
        table: CallableGuardTable,
    },
    Lookup {
        table: CallableGuardTable,
        slots: usize,
        key_weight: usize,
    },
    Insert {
        table: CallableGuardTable,
        rehash: CallableGuardRehash,
        cleanup_units: usize,
        quote: CallableGuardStorageQuote,
    },
    DefinitionSnapshot(CallableGuardStorageQuote),
    AnchorPush(CallableGuardStorageQuote),
    Dispatch,
}

impl CallableGuardStorageWork {
    pub(in crate::types) fn quote(self) -> Option<CallableGuardStorageQuote> {
        match self {
            Self::Owner(quote)
            | Self::Insert { quote, .. }
            | Self::DefinitionSnapshot(quote)
            | Self::AnchorPush(quote) => Some(quote),
            Self::EmptyLookup { .. } => Some(CallableGuardStorageQuote {
                work_units: 64,
                requested_payload_bytes: 0,
            }),
            Self::Lookup {
                slots, key_weight, ..
            } => Some(CallableGuardStorageQuote {
                work_units: key_access_units(slots, key_weight)?,
                requested_payload_bytes: 0,
            }),
            Self::Dispatch => Some(CallableGuardStorageQuote {
                work_units: 5,
                requested_payload_bytes: size_of::<DescriptorOrigin<'_>>().checked_mul(4)?,
            }),
        }
    }
}

pub(in crate::types) trait CallableGuardStorageControl {
    type Error;

    fn admit(&self, work: CallableGuardStorageWork) -> Result<(), Self::Error>;
}

#[derive(Debug, Eq, PartialEq)]
pub(in crate::types) enum CallableGuardStorageError<E> {
    Refused(E),
    UntrackedGuard,
    CapacityExhausted,
    StalePreparation,
    ScopeAlreadyEntered,
    DifferentGuard,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TableState {
    full_capacity: usize,
    funded_slots: usize,
    key_weight_sum: usize,
}

#[derive(Debug, Default)]
pub(super) struct CallableGuardStorage {
    tables: [TableState; 4],
}

#[cfg(test)]
impl Drop for CallableGuardStorage {
    fn drop(&mut self) {
        observations::storage_dropped(self);
    }
}

#[derive(Debug)]
enum CallableRemovalFunding {
    Ordinary,
    Admitted { weight: usize },
}

pub(super) struct CallableGuardEntry<K> {
    key: K,
    funding: CallableRemovalFunding,
}

// These are the guard's complete set-key vocabulary. Their interned handles do not hash
// referenced fields; the only variable inline payload is Type's debug-only Todo text.
trait CallableGuardKey: Copy + Eq + Hash {
    fn inline_bytes(self) -> usize;

    fn weight(self) -> Option<usize> {
        size_of::<Self>()
            .checked_mul(2)?
            .checked_add(self.inline_bytes().checked_mul(2)?)?
            .checked_add(1)
    }
}

impl CallableGuardKey for ExactKey<'_> {
    fn inline_bytes(self) -> usize {
        self.1.inline_payload_bytes()
    }
}

impl CallableGuardKey for IdentityKey<'_> {
    fn inline_bytes(self) -> usize {
        self.1.inline_payload_bytes()
    }
}

impl CallableGuardKey for DefinitionKey<'_> {
    fn inline_bytes(self) -> usize {
        0
    }
}

const _: () = {
    assert!(size_of::<ExactKey<'_>>() >= 4 && !needs_drop::<ExactKey<'_>>());
    assert!(size_of::<IdentityKey<'_>>() >= 4 && !needs_drop::<IdentityKey<'_>>());
    assert!(size_of::<DefinitionKey<'_>>() >= 4 && !needs_drop::<DefinitionKey<'_>>());
};

// FxHashSet uses std's hashbrown 0.17.1 backend in the pinned toolchain. Unlike current
// capacity after deletion, the greatest capacity observed after insertion retains the full
// backing capacity. This uses the same numerical bound as control::hash_slots with that
// stronger input invariant. Re-audit the backend's reservation and erasure rules on update.
fn backing_slots(full_capacity: usize) -> Option<usize> {
    full_capacity
        .checked_add(1)?
        .checked_mul(4)?
        .checked_add(32)
}

fn key_access_units(slots: usize, weight: usize) -> Option<usize> {
    slots.checked_add(1)?.checked_mul(weight)?.checked_add(64)
}

// Insertion constructs and installs a set variant and a removal receipt. Removal takes
// that receipt and may replace the inline variant. Fund two transfers of each owner at
// each boundary, including when a spilled set can perform its mutation in place.
fn entry_transfer_bytes<K>() -> Option<usize> {
    size_of::<ActiveSet<K>>()
        .checked_add(size_of::<Option<CallableGuardEntry<K>>>())?
        .checked_mul(2)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SetLayout {
    Empty,
    One,
    Spilled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SetSnapshot {
    layout: SetLayout,
    state: TableState,
    len: usize,
    capacity: usize,
}

impl SetSnapshot {
    fn read<K>(
        values: SetRef<'_, K>,
        storage: &RefCell<CallableGuardStorage>,
        table: CallableGuardTable,
    ) -> Self {
        let state = storage.borrow().tables[table.index()];
        match values {
            SetRef::Active(values) => {
                let values = values.borrow();
                Self {
                    layout: values.layout(),
                    state,
                    len: values.len(),
                    capacity: values.capacity(),
                }
            }
            SetRef::Dependency(values) => {
                let values = values.borrow();
                Self {
                    layout: SetLayout::Spilled,
                    state,
                    len: values.len(),
                    capacity: values.capacity(),
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct InsertPlan {
    key_weight: usize,
    next_key_weight_sum: usize,
    funded_slots: usize,
    work: CallableGuardStorageWork,
}

impl InsertPlan {
    fn checked<K: CallableGuardKey>(
        table: CallableGuardTable,
        snapshot: SetSnapshot,
        key: K,
    ) -> Option<Self> {
        let SetSnapshot {
            layout,
            state,
            len,
            capacity,
        } = snapshot;
        match layout {
            SetLayout::Spilled if len > capacity || capacity > state.full_capacity => return None,
            SetLayout::Empty | SetLayout::One
                if !table.removes_entries()
                    || capacity != 0
                    || state.full_capacity != 0
                    || state.funded_slots > 1
                    || len != usize::from(layout == SetLayout::One) =>
            {
                return None;
            }
            _ => {}
        }
        let key_weight = key.weight()?;
        let next_key_weight_sum = state.key_weight_sum.checked_add(key_weight)?;
        let old_slots = if layout == SetLayout::Spilled {
            backing_slots(state.full_capacity)?
        } else {
            0
        };
        let (rehash, next_slots, requested_payload_bytes) = if layout == SetLayout::Empty {
            (CallableGuardRehash::None, 1, 0)
        } else if layout == SetLayout::One {
            let (slots, bytes) = replacement_storage::<K>(2)?;
            (CallableGuardRehash::Spill, slots, bytes)
        } else if len < capacity {
            (CallableGuardRehash::None, old_slots, 0)
        } else if len.checked_add(1)? <= state.full_capacity / 2 {
            (CallableGuardRehash::InPlace, old_slots, 0)
        } else {
            let requested_capacity = state.full_capacity.checked_add(1)?;
            let (slots, bytes) = replacement_storage::<K>(requested_capacity)?;
            (
                CallableGuardRehash::Replacement { requested_capacity },
                slots,
                bytes,
            )
        };

        let funded_slots = state.funded_slots.max(next_slots);
        let transfer_bytes = if table.removes_entries() {
            entry_transfer_bytes::<K>()?.checked_mul(2)?
        } else {
            0
        };
        let spill_carrier_bytes = if rehash == CallableGuardRehash::Spill {
            size_of::<FxHashSet<K>>().checked_mul(2)?
        } else {
            0
        };
        let requested_payload_bytes = requested_payload_bytes
            .checked_add(transfer_bytes)?
            .checked_add(spill_carrier_bytes)?;
        let cleanup_units = if table.removes_entries() {
            let existing = state
                .key_weight_sum
                .checked_mul(funded_slots.checked_sub(state.funded_slots)?)?;
            existing.checked_add(key_access_units(funded_slots, key_weight)?)?
        } else {
            0
        };
        let relocation_units = if matches!(rehash, CallableGuardRehash::None) {
            0
        } else if rehash == CallableGuardRehash::Spill {
            // The retained inline key enters through normal insertion, which can compare
            // keys. Fund that insertion separately from native unique-entry rehashing.
            next_slots
                .checked_mul(3)?
                .checked_add(key_access_units(next_slots, state.key_weight_sum)?)?
                .checked_add(1)?
                .checked_add(64)?
        } else {
            // Rehashing can probe every destination group for every retained entry. The
            // aggregate weight also covers hashing and moving their inline key payloads.
            old_slots
                .checked_add(next_slots)?
                .checked_mul(3)?
                .checked_add(
                    state
                        .key_weight_sum
                        .checked_add(len.checked_mul(next_slots)?)?
                        .checked_mul(2)?,
                )?
                .checked_add(64)?
        };
        // A replacement retires the old allocation and owns a future deallocation. All
        // stored keys are passive, so neither operation scans elements for destructors.
        let deallocations = match rehash {
            CallableGuardRehash::Replacement { .. } => 2,
            CallableGuardRehash::Spill => 1,
            CallableGuardRehash::None | CallableGuardRehash::InPlace => 0,
        };
        let insertion_slots = if layout == SetLayout::Empty {
            0
        } else {
            next_slots
        };
        let work_units = key_access_units(insertion_slots, key_weight)?
            .checked_add(relocation_units)?
            .checked_add(cleanup_units)?
            .checked_add(deallocations)?;
        Some(Self {
            key_weight,
            next_key_weight_sum,
            funded_slots,
            work: CallableGuardStorageWork::Insert {
                table,
                rehash,
                cleanup_units,
                quote: CallableGuardStorageQuote {
                    work_units,
                    requested_payload_bytes,
                },
            },
        })
    }
}

fn replacement_storage<K>(requested_capacity: usize) -> Option<(usize, usize)> {
    let maximum_capacity = requested_capacity.max(4).checked_mul(2)?;
    let slots = backing_slots(maximum_capacity)?;
    let layout = size_of::<K>().checked_add(1)?.checked_mul(slots)?;
    if layout > isize::MAX as usize {
        return None;
    }
    Some((slots, requested_capacity.checked_mul(size_of::<K>())?))
}

struct PreparedSetInsert<'guard, K> {
    values: SetRef<'guard, K>,
    table: CallableGuardTable,
    key: K,
    admission: SetInsertAdmission<'guard>,
}

enum SetInsertAdmission<'guard> {
    Ordinary,
    Admitted {
        storage: &'guard RefCell<CallableGuardStorage>,
        snapshot: SetSnapshot,
        plan: InsertPlan,
    },
}

impl<'guard, K: CallableGuardKey> PreparedSetInsert<'guard, K> {
    fn ordinary(values: SetRef<'guard, K>, table: CallableGuardTable, key: K) -> Self {
        Self {
            values,
            table,
            key,
            admission: SetInsertAdmission::Ordinary,
        }
    }

    fn is_fresh(&self) -> bool {
        match self.admission {
            SetInsertAdmission::Ordinary => true,
            SetInsertAdmission::Admitted {
                storage, snapshot, ..
            } => SetSnapshot::read(self.values, storage, self.table) == snapshot,
        }
    }

    fn commit(&self) -> bool {
        // Keep reservation before equality when spilling or inserting into a hash table. No
        // admission or semantic callback occurs between this mutation and its receipt.
        let (inserted, capacity) = self.values.insert(self.key);
        if let SetInsertAdmission::Admitted { storage, plan, .. } = self.admission {
            let mut storage = storage.borrow_mut();
            let state = &mut storage.tables[self.table.index()];
            state.full_capacity = state.full_capacity.max(capacity);
            state.funded_slots = plan.funded_slots;
            if inserted {
                state.key_weight_sum = plan.next_key_weight_sum;
            }
        }
        inserted
    }

    fn commit_into(&self, entry: &mut Option<CallableGuardEntry<K>>) -> bool {
        let inserted = self.commit();
        if inserted {
            *entry = Some(CallableGuardEntry {
                key: self.key,
                funding: match self.admission {
                    SetInsertAdmission::Ordinary => CallableRemovalFunding::Ordinary,
                    SetInsertAdmission::Admitted { plan, .. } => CallableRemovalFunding::Admitted {
                        weight: plan.key_weight,
                    },
                },
            });
        }
        inserted
    }
}

fn prepare_set_insert<'guard, K: CallableGuardKey, C: CallableGuardStorageControl>(
    values: SetRef<'guard, K>,
    storage: &'guard RefCell<CallableGuardStorage>,
    table: CallableGuardTable,
    key: K,
    control: &C,
) -> Result<PreparedSetInsert<'guard, K>, CallableGuardStorageError<C::Error>> {
    let snapshot = SetSnapshot::read(values, storage, table);
    let plan = InsertPlan::checked(table, snapshot, key)
        .ok_or(CallableGuardStorageError::CapacityExhausted)?;
    control
        .admit(plan.work)
        .map_err(CallableGuardStorageError::Refused)?;
    let prepared = PreparedSetInsert {
        values,
        table,
        key,
        admission: SetInsertAdmission::Admitted {
            storage,
            snapshot,
            plan,
        },
    };
    if !prepared.is_fresh() {
        return Err(CallableGuardStorageError::StalePreparation);
    }
    Ok(prepared)
}

impl<'db> CallableRecursionGuard<'db> {
    pub(super) fn insert_dependency_ordinary(&self, key: ExactKey<'db>) {
        PreparedSetInsert::ordinary(
            (&self.cache.dependencies).into(),
            CallableGuardTable::Dependencies,
            key,
        )
        .commit();
    }

    /// Constructs an empty guard after funding its admission state and eventual destruction.
    /// Controlled storage operations require this state; current set capacities cannot reconstruct it.
    pub(in crate::types) fn new_admitted<C: CallableGuardStorageControl>(
        control: &C,
    ) -> Result<Self, CallableGuardStorageError<C::Error>> {
        Self::new_admitted_with_constructor_cache(control, false)
    }

    /// Constructs admitted guard storage with the constructor cache policy selected for its root.
    /// Pass the result of `constructor_guard_cache_with`: `true` permits canonical constructor
    /// query reuse, subject to the cache's active-expansion checks.
    pub(in crate::types) fn new_admitted_with_constructor_cache<C: CallableGuardStorageControl>(
        control: &C,
        use_shared_cache: bool,
    ) -> Result<Self, CallableGuardStorageError<C::Error>> {
        let requested_payload_bytes = size_of::<RefCell<CallableGuardStorage>>()
            .checked_add(size_of::<Self>())
            .and_then(|bytes| bytes.checked_mul(2))
            .and_then(|bytes| bytes.checked_add(size_of::<RefCell<CallableGuardStorage>>()))
            .ok_or(CallableGuardStorageError::CapacityExhausted)?;
        control
            .admit(CallableGuardStorageWork::Owner(CallableGuardStorageQuote {
                work_units: 2,
                requested_payload_bytes,
            }))
            .map_err(CallableGuardStorageError::Refused)?;
        Ok(Self {
            storage_admission: Some(Box::new(RefCell::new(CallableGuardStorage::default()))),
            cache: super::ConstructorCallableCache {
                use_shared_cache,
                ..super::ConstructorCallableCache::default()
            },
            ..Self::default()
        })
    }

    fn admitted_storage<E>(
        &self,
    ) -> Result<&RefCell<CallableGuardStorage>, CallableGuardStorageError<E>> {
        self.storage_admission
            .as_deref()
            .ok_or(CallableGuardStorageError::UntrackedGuard)
    }

    /// Creates an empty scope that removes the active entries subsequently inserted through it.
    /// The caller retains this partial scope while semantic dependencies suspend or refuse entry.
    pub(in crate::types) fn begin_scope(&self) -> CallableVisitScope<'_, 'db> {
        #[cfg(test)]
        observations::scope_opened(self);
        CallableVisitScope {
            guard: self,
            key: None,
            identity: None,
            dispatch_reference: None,
            anchor_introduced: false,
        }
    }

    pub(in crate::types) fn contains_exact_with<C: CallableGuardStorageControl>(
        &self,
        key: ExactKey<'db>,
        control: &C,
    ) -> Result<bool, CallableGuardStorageError<C::Error>> {
        let storage = self.admitted_storage()?;
        let table = CallableGuardTable::Exact;
        let snapshot = SetSnapshot::read((&self.active.seen).into(), storage, table);
        let work = if snapshot.len == 0 {
            CallableGuardStorageWork::EmptyLookup { table }
        } else {
            let slots = match snapshot.layout {
                SetLayout::Empty | SetLayout::One => 1,
                SetLayout::Spilled => backing_slots(snapshot.state.full_capacity)
                    .ok_or(CallableGuardStorageError::CapacityExhausted)?,
            };
            let key_weight = key
                .weight()
                .ok_or(CallableGuardStorageError::CapacityExhausted)?;
            CallableGuardStorageWork::Lookup {
                table,
                slots,
                key_weight,
            }
        };
        work.quote()
            .ok_or(CallableGuardStorageError::CapacityExhausted)?;
        control
            .admit(work)
            .map_err(CallableGuardStorageError::Refused)?;
        if SetSnapshot::read((&self.active.seen).into(), storage, table) != snapshot {
            return Err(CallableGuardStorageError::StalePreparation);
        }
        Ok(snapshot.len != 0 && self.active.seen.borrow().contains(&key))
    }

    pub(in crate::types) fn prepare_dependency_insert<C: CallableGuardStorageControl>(
        &self,
        key: ExactKey<'db>,
        control: &C,
    ) -> Result<PreparedCallableDependencyInsert<'_, 'db>, CallableGuardStorageError<C::Error>>
    {
        Ok(PreparedCallableDependencyInsert {
            prepared: prepare_set_insert(
                (&self.cache.dependencies).into(),
                self.admitted_storage()?,
                CallableGuardTable::Dependencies,
                key,
                control,
            )?,
        })
    }

    /// Copies active definition uses after funding the backing scan, output, and its disposal.
    /// The resulting snapshot can cross semantic callouts without retaining a set borrow.
    pub(in crate::types) fn active_definition_uses_with<C: CallableGuardStorageControl>(
        &self,
        control: &C,
    ) -> Result<Vec<DefinitionKey<'db>>, CallableGuardStorageError<C::Error>> {
        let storage = self.admitted_storage()?;
        let table = CallableGuardTable::DefinitionDispatch;
        let values = &self.growth.active.seen;
        let snapshot = SetSnapshot::read(values.into(), storage, table);
        let slots = backing_slots(snapshot.state.full_capacity)
            .ok_or(CallableGuardStorageError::CapacityExhausted)?;
        let allocation_bytes = snapshot
            .len
            .checked_mul(size_of::<DefinitionKey<'db>>())
            .filter(|bytes| *bytes <= isize::MAX as usize)
            .ok_or(CallableGuardStorageError::CapacityExhausted)?;
        let requested_payload_bytes = allocation_bytes
            .checked_mul(3)
            .ok_or(CallableGuardStorageError::CapacityExhausted)?;
        let work_units = snapshot
            .len
            .checked_mul(2)
            .and_then(|units| units.checked_add(slots))
            .and_then(|units| units.checked_add(65))
            .ok_or(CallableGuardStorageError::CapacityExhausted)?;
        control
            .admit(CallableGuardStorageWork::DefinitionSnapshot(
                CallableGuardStorageQuote {
                    work_units,
                    requested_payload_bytes,
                },
            ))
            .map_err(CallableGuardStorageError::Refused)?;
        if SetSnapshot::read(values.into(), storage, table) != snapshot {
            return Err(CallableGuardStorageError::StalePreparation);
        }
        let mut result = Vec::with_capacity(snapshot.len);
        match &*values.borrow() {
            ActiveSet::Empty => {}
            ActiveSet::One(key) => result.push(*key),
            ActiveSet::Spilled(values) => result.extend(values.iter().copied()),
        }
        Ok(result)
    }

    pub(in crate::types) fn begin_dependency_scope(&self) -> DescriptorDispatchScope<'_, 'db> {
        DescriptorDispatchScope {
            current: &self.growth.dispatch,
            previous: None,
        }
    }

    pub(in crate::types) fn prepare_dependency_replace<'scope, 'guard, C: CallableGuardStorageControl>(
        &'guard self,
        scope: &'scope mut DescriptorDispatchScope<'guard, 'db>,
        origin: DescriptorOrigin<'db>,
        control: &C,
    ) -> Result<PreparedDescriptorDispatch<'scope, 'guard, 'db>, CallableGuardStorageError<C::Error>>
    {
        self.admitted_storage()?;
        if !std::ptr::eq(scope.current, &self.growth.dispatch) {
            return Err(CallableGuardStorageError::DifferentGuard);
        }
        if scope.previous.is_some() {
            return Err(CallableGuardStorageError::ScopeAlreadyEntered);
        }
        let current = scope.current.get();
        control
            .admit(CallableGuardStorageWork::Dispatch)
            .map_err(CallableGuardStorageError::Refused)?;
        if scope.current.get() != current {
            return Err(CallableGuardStorageError::StalePreparation);
        }
        Ok(PreparedDescriptorDispatch {
            scope,
            current,
            origin,
        })
    }
}

pub(in crate::types) struct PreparedCallableDependencyInsert<'guard, 'db> {
    prepared: PreparedSetInsert<'guard, ExactKey<'db>>,
}

impl PreparedCallableDependencyInsert<'_, '_> {
    /// Inserts the dependency without a scope-removal obligation. Stale preparation is returned
    /// unchanged; retry must obtain a new quote without refunding the earlier admission.
    /// An existing dependency returns `Ok(false)`, even if its insertion reserves more storage.
    pub(in crate::types) fn try_commit(self) -> Result<bool, Self> {
        if !self.prepared.is_fresh() {
            return Err(self);
        }
        Ok(self.prepared.commit())
    }
}

enum PreparedScopeInsert<'guard, 'db> {
    Exact(PreparedSetInsert<'guard, ExactKey<'db>>),
    Identity(PreparedSetInsert<'guard, IdentityKey<'db>>),
    Definition(PreparedSetInsert<'guard, DefinitionKey<'db>>),
}

pub(in crate::types) struct PreparedCallableInsert<'scope, 'guard, 'db> {
    scope: &'scope mut CallableVisitScope<'guard, 'db>,
    prepared: PreparedScopeInsert<'guard, 'db>,
}

impl PreparedCallableInsert<'_, '_, '_> {
    /// Inserts an active entry and installs its prepaid removal directly in the retained scope.
    /// A duplicate returns `Ok(false)` without a new receipt; stale preparation returns `Err(self)`.
    /// The caller retains the scope if runtime completion is rejected after commit, so children
    /// can be drained before the scope removes its active entries.
    pub(in crate::types) fn try_commit(self) -> Result<bool, Self> {
        let fresh = match &self.prepared {
            PreparedScopeInsert::Exact(prepared) => self.scope.key.is_none() && prepared.is_fresh(),
            PreparedScopeInsert::Identity(prepared) => {
                self.scope.identity.is_none() && prepared.is_fresh()
            }
            PreparedScopeInsert::Definition(prepared) => {
                self.scope.dispatch_reference.is_none() && prepared.is_fresh()
            }
        };
        if !fresh {
            return Err(self);
        }
        Ok(match &self.prepared {
            PreparedScopeInsert::Exact(prepared) => prepared.commit_into(&mut self.scope.key),
            PreparedScopeInsert::Identity(prepared) => {
                prepared.commit_into(&mut self.scope.identity)
            }
            PreparedScopeInsert::Definition(prepared) => {
                prepared.commit_into(&mut self.scope.dispatch_reference)
            }
        })
    }
}

impl<'guard, 'db> CallableVisitScope<'guard, 'db> {
    pub(super) fn insert_exact_ordinary(&mut self, key: ExactKey<'db>) -> bool {
        PreparedSetInsert::ordinary(
            (&self.guard.active.seen).into(),
            CallableGuardTable::Exact,
            key,
        )
        .commit_into(&mut self.key)
    }

    pub(super) fn insert_identity_ordinary(&mut self, key: IdentityKey<'db>) -> bool {
        PreparedSetInsert::ordinary(
            (&self.guard.identities.seen).into(),
            CallableGuardTable::Identity,
            key,
        )
        .commit_into(&mut self.identity)
    }

    pub(super) fn insert_definition_ordinary(&mut self, key: DefinitionKey<'db>) -> bool {
        PreparedSetInsert::ordinary(
            (&self.guard.growth.active.seen).into(),
            CallableGuardTable::DefinitionDispatch,
            key,
        )
        .commit_into(&mut self.dispatch_reference)
    }

    pub(super) fn push_anchor_ordinary(&mut self, anchor: Anchor<'db>) {
        self.push_anchor(anchor, None);
    }

    fn push_anchor(&mut self, anchor: Anchor<'db>, growth: Option<GrowthPlan>) {
        let mut anchors = self.guard.growth.anchors.borrow_mut();
        if let Some(growth) = growth {
            let additional = growth.requested_capacity - anchors.len();
            anchors.reserve_exact(additional);
        }
        anchors.push(anchor);
        self.anchor_introduced = true;
    }

    pub(in crate::types) fn prepare_exact_insert<'scope, C: CallableGuardStorageControl>(
        &'scope mut self,
        key: ExactKey<'db>,
        control: &C,
    ) -> Result<PreparedCallableInsert<'scope, 'guard, 'db>, CallableGuardStorageError<C::Error>>
    {
        if self.key.is_some() {
            return Err(CallableGuardStorageError::ScopeAlreadyEntered);
        }
        let prepared = prepare_set_insert(
            (&self.guard.active.seen).into(),
            self.guard.admitted_storage()?,
            CallableGuardTable::Exact,
            key,
            control,
        )?;
        Ok(PreparedCallableInsert {
            scope: self,
            prepared: PreparedScopeInsert::Exact(prepared),
        })
    }

    pub(in crate::types) fn prepare_identity_insert<'scope, C: CallableGuardStorageControl>(
        &'scope mut self,
        key: IdentityKey<'db>,
        control: &C,
    ) -> Result<PreparedCallableInsert<'scope, 'guard, 'db>, CallableGuardStorageError<C::Error>>
    {
        if self.identity.is_some() {
            return Err(CallableGuardStorageError::ScopeAlreadyEntered);
        }
        let prepared = prepare_set_insert(
            (&self.guard.identities.seen).into(),
            self.guard.admitted_storage()?,
            CallableGuardTable::Identity,
            key,
            control,
        )?;
        Ok(PreparedCallableInsert {
            scope: self,
            prepared: PreparedScopeInsert::Identity(prepared),
        })
    }

    pub(in crate::types) fn prepare_definition_insert<'scope, C: CallableGuardStorageControl>(
        &'scope mut self,
        key: DefinitionKey<'db>,
        control: &C,
    ) -> Result<PreparedCallableInsert<'scope, 'guard, 'db>, CallableGuardStorageError<C::Error>>
    {
        if self.dispatch_reference.is_some() {
            return Err(CallableGuardStorageError::ScopeAlreadyEntered);
        }
        let prepared = prepare_set_insert(
            (&self.guard.growth.active.seen).into(),
            self.guard.admitted_storage()?,
            CallableGuardTable::DefinitionDispatch,
            key,
            control,
        )?;
        Ok(PreparedCallableInsert {
            scope: self,
            prepared: PreparedScopeInsert::Definition(prepared),
        })
    }

    pub(super) fn prepare_anchor_push<'scope, C: CallableGuardStorageControl>(
        &'scope mut self,
        anchor: Anchor<'db>,
        control: &C,
    ) -> Result<PreparedCallableAnchor<'scope, 'guard, 'db>, CallableGuardStorageError<C::Error>>
    {
        self.guard.admitted_storage()?;
        if self.anchor_introduced {
            return Err(CallableGuardStorageError::ScopeAlreadyEntered);
        }
        let snapshot = AnchorSnapshot::read(self.guard);
        let growth = if snapshot.len == snapshot.capacity {
            let required = snapshot
                .len
                .checked_add(1)
                .ok_or(CallableGuardStorageError::CapacityExhausted)?;
            Some(
                sequence_growth::<Anchor<'db>, C::Error>(snapshot.capacity, required)
                    .map_err(|_| CallableGuardStorageError::CapacityExhausted)?,
            )
        } else {
            None
        };
        let transfer_bytes = size_of::<Anchor<'db>>()
            .checked_mul(2)
            .ok_or(CallableGuardStorageError::CapacityExhausted)?;
        let quote = if let Some(growth) = growth {
            let relocation_bytes = snapshot
                .len
                .checked_mul(size_of::<Anchor<'db>>())
                .ok_or(CallableGuardStorageError::CapacityExhausted)?;
            CallableGuardStorageQuote {
                work_units: growth
                    .relocation_units
                    .checked_add(66)
                    .and_then(|units| units.checked_add(2))
                    .ok_or(CallableGuardStorageError::CapacityExhausted)?,
                requested_payload_bytes: growth
                    .requested_payload_bytes
                    .checked_add(transfer_bytes)
                    .and_then(|bytes| bytes.checked_add(relocation_bytes))
                    .ok_or(CallableGuardStorageError::CapacityExhausted)?,
            }
        } else {
            CallableGuardStorageQuote {
                work_units: 66,
                requested_payload_bytes: transfer_bytes,
            }
        };
        control
            .admit(CallableGuardStorageWork::AnchorPush(quote))
            .map_err(CallableGuardStorageError::Refused)?;
        if AnchorSnapshot::read(self.guard) != snapshot {
            return Err(CallableGuardStorageError::StalePreparation);
        }
        Ok(PreparedCallableAnchor {
            scope: self,
            anchor,
            snapshot,
            growth,
        })
    }

    pub(super) fn remove_storage_entries(&mut self) {
        remove_prepaid(
            (&self.guard.active.seen).into(),
            self.guard.storage_admission.as_deref(),
            CallableGuardTable::Exact,
            self.key.take(),
        );
        remove_prepaid(
            (&self.guard.identities.seen).into(),
            self.guard.storage_admission.as_deref(),
            CallableGuardTable::Identity,
            self.identity.take(),
        );
        remove_prepaid(
            (&self.guard.growth.active.seen).into(),
            self.guard.storage_admission.as_deref(),
            CallableGuardTable::DefinitionDispatch,
            self.dispatch_reference.take(),
        );
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct AnchorSnapshot {
    len: usize,
    capacity: usize,
}

impl AnchorSnapshot {
    fn read(guard: &CallableRecursionGuard<'_>) -> Self {
        let anchors = guard.growth.anchors.borrow();
        Self {
            len: anchors.len(),
            capacity: anchors.capacity(),
        }
    }
}

pub(super) struct PreparedCallableAnchor<'scope, 'guard, 'db> {
    scope: &'scope mut CallableVisitScope<'guard, 'db>,
    anchor: Anchor<'db>,
    snapshot: AnchorSnapshot,
    growth: Option<GrowthPlan>,
}

impl PreparedCallableAnchor<'_, '_, '_> {
    /// Publishes the anchor and its prepaid pop together in the already-owned visit scope.
    /// Returns preparation unchanged if the stack changed or the scope already owns an anchor.
    pub(super) fn try_commit(self) -> Result<(), Self> {
        if self.scope.anchor_introduced || AnchorSnapshot::read(self.scope.guard) != self.snapshot {
            return Err(self);
        }
        self.scope.push_anchor(self.anchor, self.growth);
        Ok(())
    }
}

pub(in crate::types) struct PreparedDescriptorDispatch<'scope, 'guard, 'db> {
    scope: &'scope mut DescriptorDispatchScope<'guard, 'db>,
    current: DescriptorOrigin<'db>,
    origin: DescriptorOrigin<'db>,
}

impl PreparedDescriptorDispatch<'_, '_, '_> {
    /// Replaces dispatch state and retains its prepaid restoration in the existing scope.
    /// A default origin inherits the current state and therefore needs no restoration.
    /// Returns preparation unchanged if the dispatch changed or the scope already owns a restoration.
    pub(in crate::types) fn try_commit(self) -> Result<(), Self> {
        if self.scope.previous.is_some() || self.scope.current.get() != self.current {
            return Err(self);
        }
        self.scope.replace_storage(self.origin);
        Ok(())
    }
}

impl<'db> DescriptorDispatchScope<'_, 'db> {
    pub(super) fn replace_storage(&mut self, origin: DescriptorOrigin<'db>) {
        self.previous =
            (origin != DescriptorOrigin::default()).then(|| self.current.replace(origin));
    }
}

fn remove_prepaid<K: CallableGuardKey>(
    values: SetRef<'_, K>,
    storage: Option<&RefCell<CallableGuardStorage>>,
    table: CallableGuardTable,
    entry: Option<CallableGuardEntry<K>>,
) {
    let Some(entry) = entry else {
        return;
    };
    values.remove(&entry.key);
    if let CallableRemovalFunding::Admitted { weight } = entry.funding
        && let Some(storage) = storage
    {
        // The non-cloneable receipt is installed with the insertion, so its weight is
        // included exactly once. Descendant growth has already topped up its removal cost.
        storage.borrow_mut().tables[table.index()].key_weight_sum -= weight;
    }
}
