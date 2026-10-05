use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};

use salsa::Database;
use salsa::attempt_probe::{AttemptOutcome, Incomplete};
use salsa::execution_probe::{
    ExecutionAdmission, ExecutionLimits, ExecutionReceipt, RegistryBuilder,
    try_with_metered_execution_budget,
};

use super::*;
use crate::db::tests::{TestDb, setup_db};
use crate::types::constructor::expansion_probe;

#[derive(Default)]
struct Admission {
    events: RefCell<Vec<ExecutionWork>>,
    refuse_after: Cell<Option<usize>>,
    fired: Cell<bool>,
}

impl ExecutionAdmission for Admission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.events.borrow_mut().push(work);
        if let Some(remaining) = self.refuse_after.get() {
            self.refuse_after.set(remaining.checked_sub(1));
            if remaining == 0 {
                self.fired.set(true);
                return Err(RunError::Refused(Incomplete::Allowance));
            }
        }
        Ok(())
    }
}

fn allocate<'pool, T>(
    db: &TestDb,
    storage: &'pool StableStorage<T>,
    admission: &Admission,
    refuse_after: Option<usize>,
    make: impl FnOnce() -> T,
) -> Result<RunResult<&'pool T>, expansion_probe::Incomplete> {
    expansion_probe::run(db, usize::MAX, || {
        RegistryBuilder::new(db, admission)?
            .seal()?
            .run(|endpoint| async move {
                admission.refuse_after.set(refuse_after);
                Ok(endpoint
                    .local_call(|| storage.allocate_admitted(&endpoint, 2, make))
                    .await)
            })
    })
    .0
}

#[derive(Debug)]
struct Value<'a> {
    ordinal: usize,
    drops: &'a Cell<usize>,
}

impl Drop for Value<'_> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

#[test]
fn spills_preserve_values_and_references_outlive_local_calls() {
    let db = setup_db();
    let drops = Cell::new(0);
    let storage = StableStorage::new();
    let admission = Admission::default();
    let result = expansion_probe::run(&db, usize::MAX, || {
        let storage = &storage;
        let admission = &admission;
        let drops = &drops;
        RegistryBuilder::new(&db, admission)?
            .seal()?
            .run(|endpoint| async move {
                let mut values = [None; 65];
                let mut addresses = [std::ptr::null(); 65];
                for ordinal in 0..values.len() {
                    let before = storage.snapshot();
                    let start = admission.events.borrow().len();
                    let value = endpoint
                        .local_call(|| {
                            storage.allocate_admitted(&endpoint, 2, || Value { ordinal, drops })
                        })
                        .await;
                    values[ordinal] = Some(value);
                    addresses[ordinal] = std::ptr::from_ref(value);
                    let after = storage.snapshot();
                    assert_eq!(after.state.initialized, ordinal + 1);
                    let events = admission.events.borrow();
                    let events = &events[start..];
                    match before.state.current_capacity {
                        None => {
                            assert_eq!(
                                events,
                                &[
                                    ExecutionWork::Work { units: 10 },
                                    ExecutionWork::Resource {
                                        requested_bytes: size_of::<Value<'_>>(),
                                    },
                                ]
                            );
                            assert_eq!(after.state.previous_chunks, 0);
                            assert_eq!(after.state.current_capacity, Some(after.remaining + 1));
                        }
                        Some(capacity) if before.remaining == 0 => {
                            let previous = before.state.previous_chunks;
                            assert_eq!(
                                events,
                                &[
                                    ExecutionWork::Work {
                                        units: 14 + previous
                                    },
                                    ExecutionWork::Resource {
                                        requested_bytes: 2 * capacity * size_of::<Value<'_>>(),
                                    },
                                    ExecutionWork::Resource {
                                        requested_bytes: (2 * previous).max(4)
                                            * size_of::<Vec<Value<'_>>>(),
                                    },
                                ]
                            );
                            assert_eq!(after.state.previous_chunks, previous + 1);
                            assert_eq!(after.state.current_capacity, Some(after.remaining + 1));
                        }
                        Some(capacity) => {
                            assert_eq!(
                                events,
                                &[
                                    ExecutionWork::Work { units: 6 },
                                    ExecutionWork::Resource {
                                        requested_bytes: size_of::<Value<'_>>(),
                                    },
                                ]
                            );
                            assert_eq!(after.state.current_capacity, Some(capacity));
                            assert_eq!(after.state.previous_chunks, before.state.previous_chunks);
                            assert_eq!(after.remaining + 1, before.remaining);
                        }
                    }
                    for (expected, value) in values[..=ordinal].iter().enumerate() {
                        let value = value.expect("previously initialized value");
                        assert_eq!(value.ordinal, expected);
                        assert_eq!(std::ptr::from_ref(value), addresses[expected]);
                    }
                }
                Ok(values)
            })
    })
    .0;
    let values = result.expect("completed attempt").expect("completed run");
    assert!(storage.state.get().previous_chunks >= 5);
    for (ordinal, value) in values.into_iter().enumerate() {
        assert_eq!(value.expect("initialized value").ordinal, ordinal);
    }
    assert_eq!(drops.get(), 0);
    drop(storage);
    assert_eq!(drops.get(), 65);
}

#[test]
fn refusal_precedes_allocation_and_construction_and_allows_retry() {
    let db = setup_db();
    for initialized in [0, 1, 2, 3, 7] {
        // Every value has work and payload admissions; a spill also admits the chunk list.
        let admissions = match initialized {
            0 | 2 => 2,
            _ => 3,
        };
        for refuse_after in 0..admissions {
            let storage = StableStorage::new();
            let admission = Admission::default();
            for value in 0..initialized {
                allocate(&db, &storage, &admission, None, || value)
                    .expect("completed setup attempt")
                    .expect("completed setup run");
            }
            let before = storage.snapshot();
            let constructed = Cell::new(false);
            let result = allocate(&db, &storage, &admission, Some(refuse_after), || {
                constructed.set(true);
                initialized
            });
            assert!(matches!(
                result,
                Err(expansion_probe::Incomplete::Allowance)
            ));
            assert!(admission.fired.get());
            assert!(!constructed.get());
            assert_eq!(storage.snapshot(), before);
            assert!(!storage.allocating.get());
            let value = allocate(&db, &storage, &admission, None, || initialized)
                .expect("completed retry attempt")
                .expect("completed retry run");
            assert_eq!(*value, initialized);
            assert_eq!(storage.state.get().initialized, initialized + 1);
        }
    }
}

#[test]
fn refused_backing_does_not_run_an_admitted_allocating_constructor() {
    let db = setup_db();
    let storage = StableStorage::new();
    let admission = Admission::default();
    let constructed = Cell::new(false);
    let result = expansion_probe::run(&db, usize::MAX, || {
        let storage = &storage;
        let admission = &admission;
        let constructed = &constructed;
        RegistryBuilder::new(&db, admission)?
            .seal()?
            .run(|endpoint| async move {
                endpoint
                    .local_call(|| {
                        endpoint.admit(ExecutionWork::Resource {
                            requested_bytes: size_of::<usize>(),
                        })?;
                        admission.refuse_after.set(Some(1));
                        storage.allocate_admitted(&endpoint, 4, || {
                            constructed.set(true);
                            Box::new(42usize)
                        })
                    })
                    .await;
                Ok(())
            })
    })
    .0;
    assert!(matches!(
        result,
        Err(expansion_probe::Incomplete::Allowance)
    ));
    assert!(admission.fired.get());
    assert!(!constructed.get());
    assert!(storage.arena.get().is_none());
}

#[test]
fn failed_constructor_preserves_reservation_and_retires_only_initialized_values() {
    let db = setup_db();
    let drops = Cell::new(0);
    let storage = StableStorage::new();
    let admission = Admission::default();
    let original = allocate(&db, &storage, &admission, None, || Value {
        ordinal: 0,
        drops: &drops,
    })
    .expect("completed initial attempt")
    .expect("completed initial run");
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        allocate(&db, &storage, &admission, None, || {
            let _partial = Value {
                ordinal: 1,
                drops: &drops,
            };
            panic!("constructor failed");
        })
    }));
    assert!(outcome.is_err());
    assert_eq!(drops.get(), 1);
    assert_eq!(storage.state.get().initialized, 1);
    assert_eq!(storage.state.get().previous_chunks, 1);
    assert_eq!(
        storage.state.get().current_capacity,
        Some(storage.snapshot().remaining)
    );
    assert!(!storage.allocating.get());
    let retry = allocate(&db, &storage, &admission, None, || Value {
        ordinal: 2,
        drops: &drops,
    })
    .expect("completed retry attempt")
    .expect("completed retry run");
    assert_eq!(original.ordinal, 0);
    assert_eq!(retry.ordinal, 2);
    assert_eq!(drops.get(), 1);
    drop(storage);
    assert_eq!(drops.get(), 3);
}

struct CancelOnWork<'db> {
    db: &'db TestDb,
}

impl ExecutionAdmission for CancelOnWork<'_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if matches!(work, ExecutionWork::Work { .. }) {
            self.db.cancellation_token().cancel();
        }
        Ok(())
    }
}

#[test]
fn cancellation_preserves_values_until_pool_destruction() {
    let db = setup_db();
    let drops = Cell::new(0);
    let storage = StableStorage::new();
    allocate(&db, &storage, &Admission::default(), None, || Value {
        ordinal: 0,
        drops: &drops,
    })
    .expect("completed initial attempt")
    .expect("completed initial run");
    let before = storage.snapshot();
    let constructed = Cell::new(false);
    let cancellation = CancelOnWork { db: &db };
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        expansion_probe::run(&db, usize::MAX, || {
            let storage = &storage;
            let constructed = &constructed;
            let drops = &drops;
            RegistryBuilder::new(&db, &cancellation)?
                .seal()?
                .run(|endpoint| async move {
                    endpoint
                        .local_call(|| {
                            storage.allocate_admitted(&endpoint, 2, || {
                                constructed.set(true);
                                Value { ordinal: 1, drops }
                            })
                        })
                        .await;
                    Ok(())
                })
        })
    }));
    let payload = outcome.expect_err("native cancellation");
    assert!(matches!(
        payload.downcast_ref::<salsa::Cancelled>(),
        Some(salsa::Cancelled::Local)
    ));
    assert!(!constructed.get());
    assert_eq!(storage.snapshot(), before);
    assert!(!storage.allocating.get());
    assert_eq!(drops.get(), 0);
    drop(storage);
    assert_eq!(drops.get(), 1);
}

#[test]
fn recursive_constructor_cannot_consume_the_reserved_slot() {
    let db = setup_db();
    let storage = StableStorage::new();
    let admission = Admission::default();
    let result = expansion_probe::run(&db, usize::MAX, || {
        let storage = &storage;
        RegistryBuilder::new(&db, &admission)?
            .seal()?
            .run(|endpoint| async move {
                let value = endpoint
                    .local_call(|| {
                        storage.allocate_admitted(&endpoint, 2, || {
                            assert_eq!(
                                storage.allocate_admitted(&endpoint, 2, || 99),
                                Err(RunError::Contract(
                                    "recursive stable relation storage allocation"
                                )),
                            );
                            42
                        })
                    })
                    .await;
                Ok(*value)
            })
    })
    .0;
    assert!(matches!(result, Ok(Ok(42))));
    assert_eq!(storage.state.get().initialized, 1);
    assert_eq!(storage.state.get().previous_chunks, 0);
    assert!(!storage.allocating.get());
}

#[test]
fn quotation_rejects_overflow_and_zero_sized_values() {
    let empty = Snapshot {
        state: StorageState::default(),
        remaining: 0,
    };
    assert!(Quote::new::<usize>(empty, usize::MAX).is_err());
    assert!(Quote::new::<()>(empty, 0).is_err());
    for current_capacity in [usize::MAX, isize::MAX as usize] {
        let snapshot = Snapshot {
            state: StorageState {
                current_capacity: Some(current_capacity),
                ..empty.state
            },
            ..empty
        };
        assert!(Quote::new::<usize>(snapshot, 0).is_err());
    }
    let snapshot = Snapshot {
        state: StorageState {
            initialized: usize::MAX,
            ..empty.state
        },
        ..empty
    };
    assert!(Quote::new::<usize>(snapshot, 0).is_err());
}

fn storage_with_spare_slot<const N: usize>(db: &TestDb) -> StableStorage<[u8; N]> {
    let storage = StableStorage::new();
    for _ in 0..2 {
        allocate(db, &storage, &Admission::default(), None, || [0; N])
            .expect("completed setup attempt")
            .expect("completed setup run");
    }
    assert_eq!(storage.snapshot().remaining, 1);
    storage
}

fn allocate_with_budget<const N: usize>(
    db: &TestDb,
    storage: &StableStorage<[u8; N]>,
    limits: ExecutionLimits,
    constructed: &Cell<bool>,
) -> ExecutionReceipt<RunResult<()>> {
    try_with_metered_execution_budget(db, limits, |budget| {
        RegistryBuilder::with_budget(db, &budget)?
            .seal()?
            .run(|endpoint| async move {
                endpoint
                    .local_call(|| {
                        storage.allocate_admitted(&endpoint, 2, || {
                            constructed.set(true);
                            [42; N]
                        })?;
                        Ok(())
                    })
                    .await;
                Ok(())
            })
    })
    .expect("valid budget entry")
}

/// Spare arena slots require bytes from the current evaluation before construction.
/// Larger values consume more bytes but the same semantic work; byte refusal leaves
/// the slot available for a retry without changing the database revision.
#[test]
fn spare_slot_construction_uses_the_current_byte_budget() {
    let db = setup_db();
    let revision = salsa::plumbing::current_revision(&db);
    let limits = ExecutionLimits {
        semantic_work: 1_000_000,
        requested_bytes: 16 * 1024 * 1024,
    };
    let small = storage_with_spare_slot::<1>(&db);
    let large = storage_with_spare_slot::<4096>(&db);
    let constructed = Cell::new(false);
    let small_receipt = allocate_with_budget(&db, &small, limits, &constructed);
    assert_eq!(small_receipt.outcome, AttemptOutcome::Complete(Ok(())));
    assert!(constructed.replace(false));
    let large_receipt = allocate_with_budget(&db, &large, limits, &constructed);
    assert_eq!(large_receipt.outcome, AttemptOutcome::Complete(Ok(())));
    assert!(constructed.replace(false));
    assert_eq!(
        large_receipt.usage.semantic_work,
        small_receipt.usage.semantic_work
    );
    assert_eq!(
        large_receipt.usage.requested_bytes - small_receipt.usage.requested_bytes,
        4096 - 1,
    );

    let storage = storage_with_spare_slot::<4096>(&db);
    let before = storage.snapshot();
    let refused = allocate_with_budget(
        &db,
        &storage,
        ExecutionLimits {
            requested_bytes: small_receipt.usage.requested_bytes,
            ..limits
        },
        &constructed,
    );
    assert_eq!(
        refused.outcome,
        AttemptOutcome::Incomplete(Incomplete::RequestedAllocation),
    );
    assert!(!constructed.get());
    assert_eq!(storage.snapshot(), before);
    assert!(!storage.allocating.get());
    let retry = allocate_with_budget(&db, &storage, limits, &constructed);
    assert_eq!(retry.outcome, AttemptOutcome::Complete(Ok(())));
    assert!(constructed.get());
    assert_eq!(
        storage.snapshot().state.initialized,
        before.state.initialized + 1
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}
