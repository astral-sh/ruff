use std::cell::{Cell, RefCell};
use std::fmt::Debug;
use std::hash::BuildHasher;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use rustc_hash::FxBuildHasher;
use salsa::attempt_probe::{AttemptOutcome, Incomplete, try_with_attempt};
use salsa::execution_probe::{
    Demand, ExecutionAdmission, ExecutionWork, RegistryBuilder, RunError, RunResult, TaskEndpoint,
};

use super::*;
use crate::db::tests::setup_db;
use crate::types::KnownClass;
use crate::types::cyclic::CallableDefinition;
use crate::types::todo_type;
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event {
    Work(CallableGuardStorageWork),
    Resource(usize),
}

#[derive(Default)]
struct Recording {
    events: RefCell<Vec<Event>>,
    refuse_at: Option<usize>,
    remaining: Cell<Option<usize>>,
    remaining_bytes: Cell<Option<usize>>,
}

impl Recording {
    fn record(&self, event: Event) -> Result<(), usize> {
        let mut events = self.events.borrow_mut();
        let index = events.len();
        events.push(event);
        if self.refuse_at == Some(index) {
            return Err(index);
        }
        if let Event::Work(work) = event
            && let Some(remaining) = self.remaining.get()
        {
            let units = work.quote().ok_or(index)?.work_units;
            self.remaining
                .set(Some(remaining.checked_sub(units).ok_or(index)?));
        }
        if let Event::Resource(bytes) = event
            && let Some(remaining) = self.remaining_bytes.get()
        {
            self.remaining_bytes
                .set(Some(remaining.checked_sub(bytes).ok_or(index)?));
        }
        Ok(())
    }
}

impl CallableGuardStorageControl for Recording {
    type Error = usize;

    fn admit(&self, work: CallableGuardStorageWork) -> Result<(), usize> {
        self.record(Event::Work(work))?;
        let quote = work.quote().ok_or(self.events.borrow().len())?;
        if quote.requested_payload_bytes != 0 {
            self.record(Event::Resource(quote.requested_payload_bytes))?;
        }
        Ok(())
    }
}

fn accepted<T, E: Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("unexpected refusal: {error:?}"),
    }
}

fn committed<T, P>(result: Result<T, P>) -> T {
    match result {
        Ok(value) => value,
        Err(_) => panic!("fresh preparation was rejected"),
    }
}

fn key<'db>(value: i64) -> ExactKey<'db> {
    (CallableExpansion::Bindings, Type::int_literal(value))
}

fn state(guard: &CallableRecursionGuard<'_>, table: CallableGuardTable) -> TableState {
    accepted(guard.admitted_storage::<usize>()).borrow().tables[table.index()]
}

fn insert_exact<'guard, 'db>(
    guard: &'guard CallableRecursionGuard<'db>,
    key: ExactKey<'db>,
    control: &Recording,
) -> CallableVisitScope<'guard, 'db> {
    let mut scope = guard.begin_scope();
    assert!(committed(
        accepted(scope.prepare_exact_insert(key, control)).try_commit()
    ));
    scope
}

fn fixture<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
) -> (DefinitionKey<'db>, Anchor<'db>, DescriptorOrigin<'db>) {
    let Some(class) = KnownClass::Int.try_to_class_literal(db, env) else {
        panic!("missing int class");
    };
    let target = RecursiveDefinition::Callable(
        CallableDefinition::Constructor(class),
        CallableExpansion::Bindings,
    );
    let dispatches = DescriptorDispatches::new(
        db,
        Box::<[crate::types::DescriptorDispatch<'db>]>::default(),
    );
    let origin = DescriptorOrigin {
        dispatches: Some(dispatches),
        ..DescriptorOrigin::default()
    };
    (
        (
            DefinitionUse {
                target,
                specialization: None,
            },
            origin,
        ),
        (target, dispatches),
        origin,
    )
}

#[test]
fn ordinary_scopes_use_receipts_without_an_admission_sidecar() {
    let guard = CallableRecursionGuard::new();
    let mut outer = guard.begin_scope();
    assert!(outer.insert_exact_ordinary(key(1)));
    assert!(
        outer
            .insert_identity_ordinary((CallableExpansion::Bindings, TypeIdentity::Other(key(1).1)))
    );
    guard.insert_dependency_ordinary(key(1));
    let mut duplicate = guard.begin_scope();
    assert!(!duplicate.insert_exact_ordinary(key(1)));
    assert!(duplicate.key.is_none());
    assert!(matches!(
        outer.key.as_ref().map(|entry| &entry.funding),
        Some(CallableRemovalFunding::Ordinary)
    ));
    assert!(guard.storage_admission.is_none());
    drop(duplicate);
    assert_eq!(guard.active.seen.borrow().len(), 1);
    drop(outer);
    assert!(guard.active.seen.borrow().is_empty());
    assert!(guard.identities.seen.borrow().is_empty());
    assert_eq!(guard.cache.dependencies.borrow().len(), 1);
}

#[test]
fn first_active_entries_remain_inline_and_cleanup_without_allocations() {
    eprintln!(
        "callable-inline-storage-layout guard={} sidecar={} exact-set={} identity-set={} definition-set={} exact-receipt={} identity-receipt={} definition-receipt={}",
        size_of::<CallableRecursionGuard<'_>>(),
        size_of::<RefCell<CallableGuardStorage>>(),
        size_of::<ActiveSet<ExactKey<'_>>>(),
        size_of::<ActiveSet<IdentityKey<'_>>>(),
        size_of::<ActiveSet<DefinitionKey<'_>>>(),
        size_of::<Option<CallableGuardEntry<ExactKey<'_>>>>(),
        size_of::<Option<CallableGuardEntry<IdentityKey<'_>>>>(),
        size_of::<Option<CallableGuardEntry<DefinitionKey<'_>>>>(),
    );
    let db = setup_db();
    let env = db.program_environment();
    let (definition, _, _) = fixture(&db, &env);
    let guard = accepted(CallableRecursionGuard::new_admitted(&Recording::default()));
    let control = Recording::default();
    let exact = key(1);
    let identity = (CallableExpansion::Bindings, TypeIdentity::Other(exact.1));
    let mut scope = guard.begin_scope();
    assert!(committed(
        accepted(scope.prepare_exact_insert(exact, &control)).try_commit()
    ));
    assert!(committed(
        accepted(scope.prepare_identity_insert(identity, &control)).try_commit()
    ));
    assert!(committed(
        accepted(scope.prepare_definition_insert(definition, &control)).try_commit()
    ));
    assert!(guard.active.seen.borrow().contains(&exact));
    assert!(guard.identities.seen.borrow().contains(&identity));
    assert!(guard.growth.active.seen.borrow().contains(&definition));
    assert_eq!(guard.active.seen.borrow().capacity(), 0);
    assert_eq!(guard.identities.seen.borrow().capacity(), 0);
    assert_eq!(guard.growth.active.seen.borrow().capacity(), 0);
    for (table, weight) in [
        (CallableGuardTable::Exact, exact.weight()),
        (CallableGuardTable::Identity, identity.weight()),
        (CallableGuardTable::DefinitionDispatch, definition.weight()),
    ] {
        assert_eq!(
            state(&guard, table),
            TableState {
                full_capacity: 0,
                funded_slots: 1,
                key_weight_sum: accepted(weight.ok_or("weight overflow")),
            }
        );
    }
    assert_eq!(
        control
            .events
            .borrow()
            .iter()
            .filter(|event| matches!(event, Event::Resource(bytes) if *bytes > 0))
            .count(),
        3
    );
    let events = control.events.borrow().len();
    drop(scope);
    assert_eq!(control.events.borrow().len(), events);
    assert!(guard.active.seen.borrow().is_empty());
    assert!(guard.identities.seen.borrow().is_empty());
    assert!(guard.growth.active.seen.borrow().is_empty());
    for table in [
        CallableGuardTable::Exact,
        CallableGuardTable::Identity,
        CallableGuardTable::DefinitionDispatch,
    ] {
        assert_eq!(state(&guard, table).key_weight_sum, 0);
        assert_eq!(state(&guard, table).full_capacity, 0);
    }
}

#[test]
fn exact_lookup_distinguishes_empty_singleton_and_retained_hash_storage() {
    let control = Recording::default();
    let guard = accepted(CallableRecursionGuard::new_admitted(&control));
    assert!(!accepted(guard.contains_exact_with(key(1), &control)));
    assert!(matches!(
        control.events.borrow().last(),
        Some(Event::Work(CallableGuardStorageWork::EmptyLookup {
            table: CallableGuardTable::Exact,
        }))
    ));
    let outer = insert_exact(&guard, key(1), &control);
    for (query, present) in [(key(1), true), (key(2), false)] {
        assert_eq!(
            accepted(guard.contains_exact_with(query, &control)),
            present
        );
        assert!(matches!(
            control.events.borrow().last(),
            Some(Event::Work(CallableGuardStorageWork::Lookup {
                table: CallableGuardTable::Exact,
                slots: 1,
                ..
            }))
        ));
    }
    let child = insert_exact(&guard, key(2), &control);
    drop(child);
    drop(outer);
    let before = state(&guard, CallableGuardTable::Exact);
    assert!(guard.active.seen.borrow().capacity() > 0);
    assert!(!accepted(guard.contains_exact_with(key(1), &control)));
    assert!(matches!(
        control.events.borrow().last(),
        Some(Event::Work(CallableGuardStorageWork::EmptyLookup {
            table: CallableGuardTable::Exact,
        }))
    ));
    assert_eq!(state(&guard, CallableGuardTable::Exact), before);
}

#[test]
fn duplicate_inline_entries_spill_without_acquiring_receipts() {
    let db = setup_db();
    let env = db.program_environment();
    let (definition, _, _) = fixture(&db, &env);
    let control = Recording::default();
    let guard = accepted(CallableRecursionGuard::new_admitted(&control));
    let exact = key(1);
    let identity = (CallableExpansion::Bindings, TypeIdentity::Other(exact.1));
    let mut outer = insert_exact(&guard, exact, &control);
    assert!(committed(
        accepted(outer.prepare_identity_insert(identity, &control)).try_commit()
    ));
    assert!(committed(
        accepted(outer.prepare_definition_insert(definition, &control)).try_commit()
    ));
    let before = accepted(guard.admitted_storage::<usize>()).borrow().tables;
    let mut duplicate = guard.begin_scope();
    assert!(!committed(
        accepted(duplicate.prepare_exact_insert(exact, &control)).try_commit()
    ));
    assert!(!committed(
        accepted(duplicate.prepare_identity_insert(identity, &control)).try_commit()
    ));
    assert!(!committed(
        accepted(duplicate.prepare_definition_insert(definition, &control)).try_commit()
    ));
    assert!(duplicate.key.is_none());
    assert!(duplicate.identity.is_none());
    assert!(duplicate.dispatch_reference.is_none());
    for table in [
        CallableGuardTable::Exact,
        CallableGuardTable::Identity,
        CallableGuardTable::DefinitionDispatch,
    ] {
        let after = state(&guard, table);
        assert!(after.full_capacity > 0);
        assert!(after.funded_slots > before[table.index()].funded_slots);
        assert_eq!(after.key_weight_sum, before[table.index()].key_weight_sum);
    }
    drop(duplicate);
    assert!(guard.active.seen.borrow().contains(&exact));
    assert!(guard.identities.seen.borrow().contains(&identity));
    assert!(guard.growth.active.seen.borrow().contains(&definition));
    let grown = accepted(guard.admitted_storage::<usize>()).borrow().tables;
    drop(outer);
    assert!(guard.active.seen.borrow().is_empty());
    assert!(guard.identities.seen.borrow().is_empty());
    assert!(guard.growth.active.seen.borrow().is_empty());
    assert!(guard.active.seen.borrow().capacity() > 0);
    assert!(guard.identities.seen.borrow().capacity() > 0);
    assert!(guard.growth.active.seen.borrow().capacity() > 0);
    for table in [
        CallableGuardTable::Exact,
        CallableGuardTable::Identity,
        CallableGuardTable::DefinitionDispatch,
    ] {
        assert_eq!(
            state(&guard, table),
            TableState {
                key_weight_sum: 0,
                ..grown[table.index()]
            }
        );
    }
}

#[test]
fn descendant_growth_prepays_every_outstanding_removal() {
    let ancestor = (
        CallableExpansion::Bindings,
        todo_type!(
            "an active callable ancestor with enough inline Todo text to distinguish the retained key's insertion cost from its inexpensive descendant"
        ),
    );
    let descendant = key(1);
    let ancestor_identity = (ancestor.0, TypeIdentity::Other(ancestor.1));
    let descendant_identity = (descendant.0, TypeIdentity::Other(descendant.1));
    for (table, ancestor_weight, descendant_weight) in [
        (
            CallableGuardTable::Exact,
            ancestor.weight(),
            descendant.weight(),
        ),
        (
            CallableGuardTable::Identity,
            ancestor_identity.weight(),
            descendant_identity.weight(),
        ),
    ] {
        let ancestor_weight = accepted(ancestor_weight.ok_or("ancestor weight overflow"));
        let descendant_weight = accepted(descendant_weight.ok_or("descendant weight overflow"));
        #[cfg(debug_assertions)]
        assert!(ancestor_weight > descendant_weight);
        let control = Recording::default();
        let guard = accepted(CallableRecursionGuard::new_admitted(&control));
        let mut outer = guard.begin_scope();
        let prepared = if table == CallableGuardTable::Exact {
            outer.prepare_exact_insert(ancestor, &control)
        } else {
            outer.prepare_identity_insert(ancestor_identity, &control)
        };
        assert!(committed(accepted(prepared).try_commit()));
        assert_eq!(state(&guard, table).full_capacity, 0);
        assert_eq!(state(&guard, table).key_weight_sum, ancestor_weight);
        let mut child = guard.begin_scope();
        let prepared = if table == CallableGuardTable::Exact {
            child.prepare_exact_insert(descendant, &control)
        } else {
            child.prepare_identity_insert(descendant_identity, &control)
        };
        assert!(committed(accepted(prepared).try_commit()));
        let spilled = state(&guard, table);
        assert!(spilled.full_capacity > 0);
        assert_eq!(spilled.key_weight_sum, ancestor_weight + descendant_weight);
        let work = control.events.borrow().iter().rev().find_map(|event| {
            if let Event::Work(CallableGuardStorageWork::Insert {
                table: observed_table,
                rehash: CallableGuardRehash::Spill,
                cleanup_units,
                quote,
            }) = event
                && *observed_table == table
            {
                Some((quote.work_units, *cleanup_units))
            } else {
                None
            }
        });
        let Some((work_units, cleanup_units)) = work else {
            panic!("missing inline spill admission");
        };
        assert!(
            work_units - cleanup_units
                >= accepted(
                    key_access_units(spilled.funded_slots, ancestor_weight)
                        .ok_or("retained key insertion overflow")
                )
        );
        assert!(cleanup_units >= ancestor_weight * (spilled.funded_slots - 1));
        let events = control.events.borrow().len();
        drop(child);
        assert_eq!(state(&guard, table).key_weight_sum, ancestor_weight);
        drop(outer);
        assert_eq!(state(&guard, table).key_weight_sum, 0);
        assert_eq!(control.events.borrow().len(), events);
    }

    let control = Recording::default();
    let guard = accepted(CallableRecursionGuard::new_admitted(&control));
    let outer = insert_exact(&guard, key(0), &control);
    let initial = state(&guard, CallableGuardTable::Exact);
    let mut children = Vec::new();
    for value in 1..128 {
        let before = state(&guard, CallableGuardTable::Exact);
        let query = key(value);
        children.push(insert_exact(&guard, query, &control));
        let after = state(&guard, CallableGuardTable::Exact);
        let weight = accepted(query.weight().ok_or("weight overflow"));
        let expected = before.key_weight_sum * (after.funded_slots - before.funded_slots)
            + accepted(key_access_units(after.funded_slots, weight).ok_or("cleanup overflow"));
        let cleanup = control
            .events
            .borrow()
            .iter()
            .rev()
            .find_map(|event| match event {
                Event::Work(CallableGuardStorageWork::Insert { cleanup_units, .. }) => {
                    Some(*cleanup_units)
                }
                _ => None,
            });
        assert_eq!(cleanup, Some(expected));
        assert_eq!(after.key_weight_sum, before.key_weight_sum + weight);
        assert!(
            after.funded_slots
                >= accepted(backing_slots(after.full_capacity).ok_or("slots overflow"))
        );
    }
    let grown = state(&guard, CallableGuardTable::Exact);
    assert!(grown.full_capacity > initial.full_capacity);
    let charges = control.events.borrow().len();
    while let Some(child) = children.pop() {
        drop(child);
    }
    assert_eq!(
        state(&guard, CallableGuardTable::Exact).key_weight_sum,
        initial.key_weight_sum
    );
    drop(outer);
    assert_eq!(control.events.borrow().len(), charges);
    assert_eq!(
        state(&guard, CallableGuardTable::Exact),
        TableState {
            key_weight_sum: 0,
            ..grown
        }
    );
    assert!(guard.active.seen.borrow().is_empty());
}

#[test]
fn duplicate_reservation_growth_keeps_only_the_original_receipt() {
    let control = Recording::default();
    let guard = accepted(CallableRecursionGuard::new_admitted(&control));
    let mut owners = Vec::new();
    for value in 0..7 {
        owners.push(insert_exact(&guard, key(value), &control));
    }
    let before = state(&guard, CallableGuardTable::Exact);
    assert_eq!(
        guard.active.seen.borrow().len(),
        guard.active.seen.borrow().capacity()
    );
    let mut duplicate = guard.begin_scope();
    assert!(!committed(
        accepted(duplicate.prepare_exact_insert(key(0), &control)).try_commit()
    ));
    assert!(duplicate.key.is_none());
    let after = state(&guard, CallableGuardTable::Exact);
    assert!(after.full_capacity > before.full_capacity);
    assert!(after.funded_slots > before.funded_slots);
    assert_eq!(after.key_weight_sum, before.key_weight_sum);
    drop(duplicate);
    while let Some(owner) = owners.pop() {
        drop(owner);
    }
    assert_eq!(state(&guard, CallableGuardTable::Exact).key_weight_sum, 0);
}

#[test]
fn refused_inline_spill_preserves_the_ancestor_and_its_removal_funding() {
    for at in 0..2 {
        let guard = accepted(CallableRecursionGuard::new_admitted(&Recording::default()));
        let outer = insert_exact(&guard, key(0), &Recording::default());
        let before = state(&guard, CallableGuardTable::Exact);
        let control = Recording {
            refuse_at: Some(at),
            ..Recording::default()
        };
        let mut descendant = guard.begin_scope();
        assert!(
            matches!(descendant.prepare_exact_insert(key(1), &control), Err(CallableGuardStorageError::Refused(index)) if index == at)
        );
        assert!(descendant.key.is_none());
        assert_eq!(state(&guard, CallableGuardTable::Exact), before);
        assert_eq!(guard.active.seen.borrow().capacity(), 0);
        assert_eq!(guard.active.seen.borrow().len(), 1);
        assert!(guard.active.seen.borrow().contains(&key(0)));
        drop(descendant);
        let events = control.events.borrow().len();
        drop(outer);
        assert_eq!(control.events.borrow().len(), events);
        assert!(guard.active.seen.borrow().is_empty());
        assert_eq!(state(&guard, CallableGuardTable::Exact).key_weight_sum, 0);
    }
}

#[test]
fn refused_growth_keeps_ancestor_removal_funding_and_capacity_unchanged() {
    for at in 0..2 {
        let guard = accepted(CallableRecursionGuard::new_admitted(&Recording::default()));
        let mut owners = Vec::new();
        for value in 0..7 {
            owners.push(insert_exact(&guard, key(value), &Recording::default()));
        }
        let before = state(&guard, CallableGuardTable::Exact);
        let capacity = guard.active.seen.borrow().capacity();
        let control = Recording {
            refuse_at: Some(at),
            ..Recording::default()
        };
        let mut descendant = guard.begin_scope();
        assert!(
            matches!(descendant.prepare_exact_insert(key(7), &control), Err(CallableGuardStorageError::Refused(index)) if index == at)
        );
        assert!(descendant.key.is_none());
        assert_eq!(state(&guard, CallableGuardTable::Exact), before);
        assert_eq!(guard.active.seen.borrow().capacity(), capacity);
        assert_eq!(guard.active.seen.borrow().len(), 7);
        drop(descendant);
        while let Some(owner) = owners.pop() {
            drop(owner);
        }
        assert_eq!(state(&guard, CallableGuardTable::Exact).key_weight_sum, 0);
    }
}

#[test]
fn tombstone_churn_retains_history_and_funds_duplicate_in_place_rehash() {
    let control = Recording::default();
    let guard = accepted(CallableRecursionGuard::new_admitted(&control));
    let hasher = FxBuildHasher::default();
    let colliding: Vec<_> = (0..)
        .map(key)
        .filter(|key| hasher.hash_one(key) & 255 == 0)
        .take(112)
        .collect();
    let mut owners: Vec<_> = colliding
        .iter()
        .map(|key| Some(insert_exact(&guard, *key, &control)))
        .collect();
    let full = state(&guard, CallableGuardTable::Exact);
    assert_eq!(full.full_capacity, 112);
    // These concrete keys start in the same bucket. Removing entries from the full groups
    // leaves tombstones, reducing reported capacity without reducing the backing allocation.
    let removed: Vec<_> = guard
        .active
        .seen
        .borrow()
        .iter()
        .copied()
        .take(57)
        .collect();
    for key in removed {
        let Some(index) = colliding.iter().position(|candidate| *candidate == key) else {
            panic!("missing entry owner");
        };
        drop(owners[index].take());
    }
    let snapshot = SetSnapshot::read(
        (&guard.active.seen).into(),
        accepted(guard.admitted_storage::<usize>()),
        CallableGuardTable::Exact,
    );
    assert_eq!(snapshot.len, snapshot.capacity);
    assert!(snapshot.capacity < snapshot.state.full_capacity);
    assert_eq!(snapshot.state.full_capacity, full.full_capacity);
    let Some(query) = guard.active.seen.borrow().iter().next().copied() else {
        panic!("missing retained entry");
    };
    assert!(accepted(guard.contains_exact_with(query, &control)));
    assert!(
        matches!(control.events.borrow().last(), Some(Event::Work(CallableGuardStorageWork::Lookup { slots, .. })) if Some(*slots) == backing_slots(full.full_capacity))
    );
    let mut duplicate = guard.begin_scope();
    let prepared = accepted(duplicate.prepare_exact_insert(query, &control));
    assert!(matches!(
        control.events.borrow().as_slice(),
        [.., Event::Work(CallableGuardStorageWork::Insert {
            rehash: CallableGuardRehash::InPlace,
            ..
        }), Event::Resource(bytes)] if *bytes > 0
    ));
    assert!(!committed(prepared.try_commit()));
    assert_eq!(guard.active.seen.borrow().capacity(), full.full_capacity);
    assert_eq!(state(&guard, CallableGuardTable::Exact), snapshot.state);
    assert!(duplicate.key.is_none());
    drop(duplicate);
    while let Some(owner) = owners.pop() {
        drop(owner);
    }
    assert_eq!(state(&guard, CallableGuardTable::Exact).key_weight_sum, 0);
}

#[test]
fn dependencies_have_no_scope_cleanup_charge() {
    let control = Recording::default();
    let guard = accepted(CallableRecursionGuard::new_admitted(&control));
    for value in 0..16 {
        assert!(committed(
            accepted(guard.prepare_dependency_insert(key(value), &control)).try_commit()
        ));
    }
    assert!(
        control
            .events
            .borrow()
            .iter()
            .filter_map(|event| match event {
                Event::Work(CallableGuardStorageWork::Insert {
                    table: CallableGuardTable::Dependencies,
                    cleanup_units,
                    ..
                }) => Some(*cleanup_units),
                _ => None,
            })
            .all(|units| units == 0)
    );
    assert_eq!(
        state(&guard, CallableGuardTable::Dependencies).key_weight_sum,
        16 * accepted(key(0).weight().ok_or("weight overflow"))
    );
    assert_eq!(guard.cache.dependencies.borrow().len(), 16);
}

#[test]
fn inline_key_payload_and_checked_overflow_are_quoted() {
    let ty = todo_type!("callable storage payload");
    let exact = (CallableExpansion::Bindings, ty);
    let identity = (CallableExpansion::Bindings, TypeIdentity::Other(ty));
    assert_eq!(
        exact.weight(),
        Some(1 + 2 * size_of::<ExactKey<'_>>() + 2 * ty.inline_payload_bytes())
    );
    assert_eq!(
        identity.weight(),
        Some(1 + 2 * size_of::<IdentityKey<'_>>() + 2 * ty.inline_payload_bytes())
    );
    assert!(backing_slots(usize::MAX).is_none());
    for state in [
        TableState {
            full_capacity: usize::MAX,
            ..TableState::default()
        },
        TableState {
            key_weight_sum: usize::MAX,
            ..TableState::default()
        },
        TableState {
            funded_slots: usize::MAX,
            ..TableState::default()
        },
    ] {
        assert!(
            InsertPlan::checked(
                CallableGuardTable::Exact,
                SetSnapshot {
                    layout: SetLayout::Spilled,
                    state,
                    len: 0,
                    capacity: 0
                },
                key(0)
            )
            .is_none()
        );
    }
    assert!(
        InsertPlan::checked(
            CallableGuardTable::Exact,
            SetSnapshot {
                layout: SetLayout::Spilled,
                state: TableState::default(),
                len: 0,
                capacity: 1
            },
            key(0)
        )
        .is_none()
    );
}

#[test]
fn stale_preparation_is_requoted_without_refunding_consumed_work() {
    let control = Recording::default();
    let guard = accepted(CallableRecursionGuard::new_admitted(&control));
    let outer = insert_exact(&guard, key(0), &control);
    control.remaining.set(Some(1_000_000));
    let mut scope = guard.begin_scope();
    let prepared = accepted(scope.prepare_exact_insert(key(1), &control));
    let after_first_quote = control.remaining.get();
    let child = insert_exact(&guard, key(2), &control);
    let Err(stale) = prepared.try_commit() else {
        panic!("intervening insertion must invalidate preparation");
    };
    assert_eq!(stale.scope.key.as_ref().map(|entry| entry.key), None);
    drop(stale);
    drop(child);
    assert!(committed(
        accepted(scope.prepare_exact_insert(key(1), &control)).try_commit()
    ));
    assert!(control.remaining.get() < after_first_quote);
    assert_eq!(guard.active.seen.borrow().len(), 2);
    drop(scope);
    drop(outer);
    assert_eq!(state(&guard, CallableGuardTable::Exact).key_weight_sum, 0);
}

struct Reenter<'a, 'db> {
    guard: &'a CallableRecursionGuard<'db>,
    fired: Cell<bool>,
}

impl CallableGuardStorageControl for Reenter<'_, '_> {
    type Error = usize;

    fn admit(&self, _: CallableGuardStorageWork) -> Result<(), usize> {
        if !self.fired.replace(true) {
            let child = insert_exact(self.guard, key(9), &Recording::default());
            let nested = insert_exact(self.guard, key(10), &Recording::default());
            drop(nested);
            drop(child);
        }
        Ok(())
    }
}

#[test]
fn admission_callbacks_hold_no_collection_borrows_and_revalidate_history() {
    let guard = accepted(CallableRecursionGuard::new_admitted(&Recording::default()));
    let control = Reenter {
        guard: &guard,
        fired: Cell::new(false),
    };
    let mut scope = guard.begin_scope();
    assert!(matches!(
        scope.prepare_exact_insert(key(1), &control),
        Err(CallableGuardStorageError::StalePreparation)
    ));
    assert!(scope.key.is_none());
    assert!(guard.active.seen.borrow().is_empty());
    assert!(state(&guard, CallableGuardTable::Exact).full_capacity > 0);
    assert!(committed(
        accepted(scope.prepare_exact_insert(key(1), &control)).try_commit()
    ));
}

#[test]
fn lookups_revalidate_empty_and_singleton_storage_after_admission() {
    for active in [false, true] {
        let guard = accepted(CallableRecursionGuard::new_admitted(&Recording::default()));
        let outer = active.then(|| insert_exact(&guard, key(1), &Recording::default()));
        let control = Reenter {
            guard: &guard,
            fired: Cell::new(false),
        };
        assert_eq!(
            guard.contains_exact_with(key(1), &control),
            Err(CallableGuardStorageError::StalePreparation)
        );
        assert_eq!(guard.active.seen.borrow().len(), usize::from(active));
        assert!(guard.active.seen.borrow().capacity() > 0);
        assert_eq!(
            accepted(guard.contains_exact_with(key(1), &control)),
            active
        );
        drop(outer);
        assert!(guard.active.seen.borrow().is_empty());
    }
}

#[derive(Clone, Copy, Debug)]
enum Operation {
    Exact,
    Identity,
    Definition,
    Dependency,
    Lookup,
    Snapshot,
    Anchor,
    Dispatch,
}

fn perform<'db>(
    operation: Operation,
    guard: &CallableRecursionGuard<'db>,
    definition: DefinitionKey<'db>,
    anchor: Anchor<'db>,
    origin: DescriptorOrigin<'db>,
    control: &Recording,
) -> Result<(), CallableGuardStorageError<usize>> {
    let mut scope = guard.begin_scope();
    match operation {
        Operation::Exact => {
            committed(scope.prepare_exact_insert(key(1), control)?.try_commit());
        }
        Operation::Identity => {
            committed(
                scope
                    .prepare_identity_insert(
                        (CallableExpansion::Bindings, TypeIdentity::Other(key(1).1)),
                        control,
                    )?
                    .try_commit(),
            );
        }
        Operation::Definition => {
            committed(
                scope
                    .prepare_definition_insert(definition, control)?
                    .try_commit(),
            );
        }
        Operation::Dependency => {
            committed(
                guard
                    .prepare_dependency_insert(key(1), control)?
                    .try_commit(),
            );
        }
        Operation::Lookup => {
            guard.contains_exact_with(key(1), control)?;
        }
        Operation::Snapshot => {
            guard.active_definition_uses_with(control)?;
        }
        Operation::Anchor => {
            committed(scope.prepare_anchor_push(anchor, control)?.try_commit());
        }
        Operation::Dispatch => {
            let mut dispatch = guard.begin_dependency_scope();
            committed(
                guard
                    .prepare_dependency_replace(&mut dispatch, origin, control)?
                    .try_commit(),
            );
        }
    }
    Ok(())
}

#[test]
fn refusal_before_each_storage_admission_leaves_that_operation_uncommitted() {
    let db = setup_db();
    let env = db.program_environment();
    let (definition, anchor, origin) = fixture(&db, &env);
    for at in 0..2 {
        let control = Recording {
            refuse_at: Some(at),
            ..Recording::default()
        };
        assert!(
            matches!(CallableRecursionGuard::new_admitted(&control), Err(CallableGuardStorageError::Refused(index)) if index == at)
        );
    }
    for operation in [
        Operation::Exact,
        Operation::Identity,
        Operation::Definition,
        Operation::Dependency,
        Operation::Lookup,
        Operation::Snapshot,
        Operation::Anchor,
        Operation::Dispatch,
    ] {
        let probe_guard = accepted(CallableRecursionGuard::new_admitted(&Recording::default()));
        let control = Recording::default();
        accepted(perform(
            operation,
            &probe_guard,
            definition,
            anchor,
            origin,
            &control,
        ));
        let events = control.events.into_inner();
        for at in 0..events.len() {
            let guard = accepted(CallableRecursionGuard::new_admitted(&Recording::default()));
            let control = Recording {
                refuse_at: Some(at),
                ..Recording::default()
            };
            assert_eq!(
                perform(operation, &guard, definition, anchor, origin, &control),
                Err(CallableGuardStorageError::Refused(at)),
                "{operation:?}, {at}"
            );
            assert_eq!(&*control.events.borrow(), &events[..=at]);
            assert_eq!(
                accepted(guard.admitted_storage::<usize>()).borrow().tables,
                [TableState::default(); 4]
            );
            assert_eq!(guard.active.seen.borrow().capacity(), 0);
            assert_eq!(guard.identities.seen.borrow().capacity(), 0);
            assert_eq!(guard.growth.active.seen.borrow().capacity(), 0);
            assert_eq!(guard.cache.dependencies.borrow().capacity(), 0);
            assert_eq!(guard.growth.anchors.borrow().capacity(), 0);
            assert_eq!(guard.growth.dispatch.get(), DescriptorOrigin::default());
        }
        let ordinary = CallableRecursionGuard::new();
        let control = Recording::default();
        assert_eq!(
            perform(operation, &ordinary, definition, anchor, origin, &control),
            Err(CallableGuardStorageError::UntrackedGuard)
        );
        assert!(control.events.borrow().is_empty());
    }
}

#[test]
fn each_partial_scope_cleans_up_after_unwinding() {
    let db = setup_db();
    let env = db.program_environment();
    let (definition, anchor, origin) = fixture(&db, &env);
    for stop_after in 0..5 {
        let control = Recording::default();
        let guard = accepted(CallableRecursionGuard::new_admitted(&control));
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            let mut scope = guard.begin_scope();
            let mut dispatch = guard.begin_dependency_scope();
            for stage in 0..=stop_after {
                match stage {
                    0 => {
                        committed(
                            accepted(scope.prepare_definition_insert(definition, &control))
                                .try_commit(),
                        );
                    }
                    1 => {
                        committed(
                            accepted(scope.prepare_anchor_push(anchor, &control)).try_commit(),
                        );
                    }
                    2 => {
                        committed(
                            accepted(scope.prepare_identity_insert(
                                (CallableExpansion::Bindings, TypeIdentity::Other(key(1).1)),
                                &control,
                            ))
                            .try_commit(),
                        );
                    }
                    3 => {
                        committed(
                            accepted(scope.prepare_exact_insert(key(1), &control)).try_commit(),
                        );
                    }
                    4 => {
                        committed(
                            accepted(guard.prepare_dependency_replace(
                                &mut dispatch,
                                origin,
                                &control,
                            ))
                            .try_commit(),
                        );
                    }
                    _ => panic!("unknown partial-entry stage"),
                }
            }
            std::panic::panic_any(stop_after);
        }));
        assert_eq!(
            outcome
                .err()
                .and_then(|payload| payload.downcast_ref::<i32>().copied()),
            Some(stop_after)
        );
        assert!(guard.active.seen.borrow().is_empty());
        assert!(guard.identities.seen.borrow().is_empty());
        assert!(guard.growth.active.seen.borrow().is_empty());
        assert!(guard.growth.anchors.borrow().is_empty());
        assert_eq!(guard.growth.dispatch.get(), DescriptorOrigin::default());
        assert!(
            accepted(guard.admitted_storage::<usize>())
                .borrow()
                .tables
                .iter()
                .all(|table| table.key_weight_sum == 0)
        );
    }
}

#[test]
fn definition_snapshots_and_dispatch_scopes_preserve_ownership() {
    let db = setup_db();
    let env = db.program_environment();
    let (definition, anchor, origin) = fixture(&db, &env);
    let control = Recording::default();
    let guard = accepted(CallableRecursionGuard::new_admitted(&control));
    let mut outer = guard.begin_scope();
    assert!(committed(
        accepted(outer.prepare_definition_insert(definition, &control)).try_commit()
    ));
    committed(accepted(outer.prepare_anchor_push(anchor, &control)).try_commit());
    assert_eq!(
        accepted(guard.active_definition_uses_with(&control)),
        vec![definition]
    );
    let before = state(&guard, CallableGuardTable::DefinitionDispatch);
    for at in 0..2 {
        let refused = Recording {
            refuse_at: Some(at),
            ..Recording::default()
        };
        assert_eq!(
            guard.active_definition_uses_with(&refused),
            Err(CallableGuardStorageError::Refused(at))
        );
        assert_eq!(
            state(&guard, CallableGuardTable::DefinitionDispatch),
            before
        );
        assert_eq!(guard.growth.active.seen.borrow().len(), 1);
    }
    let mut inner = guard.begin_scope();
    assert!(!committed(
        accepted(inner.prepare_definition_insert(definition, &control)).try_commit()
    ));
    assert!(inner.dispatch_reference.is_none());
    let mut dispatch = guard.begin_dependency_scope();
    committed(
        accepted(guard.prepare_dependency_replace(&mut dispatch, origin, &control)).try_commit(),
    );
    assert_eq!(guard.growth.dispatch.get(), origin);
    let mut inherited = guard.begin_dependency_scope();
    committed(
        accepted(guard.prepare_dependency_replace(
            &mut inherited,
            DescriptorOrigin::default(),
            &control,
        ))
        .try_commit(),
    );
    assert!(inherited.previous.is_none());
    drop(inherited);
    assert_eq!(guard.growth.dispatch.get(), origin);
    let another = accepted(CallableRecursionGuard::new_admitted(&control));
    let mut foreign = another.begin_dependency_scope();
    assert!(matches!(
        guard.prepare_dependency_replace(&mut foreign, origin, &control),
        Err(CallableGuardStorageError::DifferentGuard)
    ));
    drop(inner);
    assert_eq!(guard.growth.active.seen.borrow().len(), 1);
    drop(dispatch);
    assert_eq!(guard.growth.dispatch.get(), DescriptorOrigin::default());
    drop(outer);
    assert!(guard.growth.anchors.borrow().is_empty());
    assert_eq!(
        state(&guard, CallableGuardTable::DefinitionDispatch).key_weight_sum,
        0
    );
}

#[test]
fn dispatch_byte_refusal_preserves_state_and_retries_with_scalar_work() {
    let db = setup_db();
    let env = db.program_environment();
    let (_, _, origin) = fixture(&db, &env);
    let guard = accepted(CallableRecursionGuard::new_admitted(&Recording::default()));
    let mut scope = guard.begin_dependency_scope();
    let control = Recording {
        remaining: Cell::new(Some(16)),
        remaining_bytes: Cell::new(Some(0)),
        ..Recording::default()
    };
    assert!(matches!(
        guard.prepare_dependency_replace(&mut scope, origin, &control),
        Err(CallableGuardStorageError::Refused(_))
    ));
    assert!(matches!(
        control.events.borrow().last(),
        Some(Event::Resource(_))
    ));
    assert_eq!(guard.growth.dispatch.get(), DescriptorOrigin::default());
    assert!(scope.previous.is_none());
    let work_after_refusal = control.remaining.get();
    control.remaining_bytes.set(Some(1_024));
    committed(
        accepted(guard.prepare_dependency_replace(&mut scope, origin, &control)).try_commit(),
    );
    assert_eq!(guard.growth.dispatch.get(), origin);
    assert_eq!(scope.previous, Some(DescriptorOrigin::default()));
    assert!(control.remaining.get() < work_after_refusal);
    assert!(control.remaining_bytes.get() < Some(1_024));
    drop(scope);
    assert_eq!(guard.growth.dispatch.get(), DescriptorOrigin::default());
}

#[test]
fn spare_capacity_anchor_byte_refusal_preserves_state_and_retries() {
    let db = setup_db();
    let env = db.program_environment();
    let (_, anchor, _) = fixture(&db, &env);
    let guard = accepted(CallableRecursionGuard::new_admitted(&Recording::default()));
    let mut outer = guard.begin_scope();
    committed(accepted(outer.prepare_anchor_push(anchor, &Recording::default())).try_commit());
    let capacity = guard.growth.anchors.borrow().capacity();
    assert!(capacity > guard.growth.anchors.borrow().len());
    let mut inner = guard.begin_scope();
    let control = Recording {
        remaining: Cell::new(Some(1_000)),
        remaining_bytes: Cell::new(Some(0)),
        ..Recording::default()
    };
    assert!(matches!(
        inner.prepare_anchor_push(anchor, &control),
        Err(CallableGuardStorageError::Refused(_))
    ));
    assert!(matches!(
        control.events.borrow().last(),
        Some(Event::Resource(_))
    ));
    assert_eq!(&*guard.growth.anchors.borrow(), &[anchor]);
    assert_eq!(guard.growth.anchors.borrow().capacity(), capacity);
    assert!(!inner.anchor_introduced);
    let work_after_refusal = control.remaining.get();
    control.remaining_bytes.set(Some(1_024));
    committed(accepted(inner.prepare_anchor_push(anchor, &control)).try_commit());
    assert_eq!(&*guard.growth.anchors.borrow(), &[anchor, anchor]);
    assert_eq!(guard.growth.anchors.borrow().capacity(), capacity);
    assert!(inner.anchor_introduced);
    assert!(control.remaining.get() < work_after_refusal);
    assert!(control.remaining_bytes.get() < Some(1_024));
    drop(inner);
    assert_eq!(&*guard.growth.anchors.borrow(), &[anchor]);
    drop(outer);
    assert!(guard.growth.anchors.borrow().is_empty());
}

#[derive(Default)]
struct DisposalJournal {
    drops: RefCell<Vec<&'static str>>,
    pending: RefCell<Option<Demand<()>>>,
    continued: Cell<bool>,
}

struct GuardOwner {
    guard: CallableRecursionGuard<'static>,
    journal: Rc<DisposalJournal>,
}

impl Drop for GuardOwner {
    fn drop(&mut self) {
        assert!(self.guard.active.seen.borrow().is_empty());
        assert_eq!(
            state(&self.guard, CallableGuardTable::Exact).key_weight_sum,
            0
        );
        self.journal.drops.borrow_mut().push("owner");
    }
}

struct TrackedScope<'guard> {
    scope: Option<CallableVisitScope<'guard, 'static>>,
    journal: Rc<DisposalJournal>,
    name: &'static str,
}

impl Drop for TrackedScope<'_> {
    fn drop(&mut self) {
        drop(self.scope.take());
        self.journal.drops.borrow_mut().push(self.name);
    }
}

struct QueuedChild(Rc<GuardOwner>);

impl Drop for QueuedChild {
    fn drop(&mut self) {
        assert_eq!(self.0.guard.active.seen.borrow().len(), 2);
        self.0.journal.drops.borrow_mut().push("child");
    }
}

struct EndpointControl<'a, 'run, 'db> {
    endpoint: &'a TaskEndpoint<'run, 'db>,
}

impl CallableGuardStorageControl for EndpointControl<'_, '_, '_> {
    type Error = RunError;

    fn admit(&self, work: CallableGuardStorageWork) -> RunResult<()> {
        let quote = work
            .quote()
            .ok_or(RunError::Refused(Incomplete::Allowance))?;
        self.endpoint.admit_work(quote.work_units)?;
        if quote.requested_payload_bytes != 0 {
            self.endpoint.admit(ExecutionWork::Resource {
                requested_bytes: quote.requested_payload_bytes,
            })?;
        }
        self.endpoint.check_completion()
    }
}

struct FundedExecution {
    work: Cell<usize>,
    resources: Cell<usize>,
}

impl ExecutionAdmission for FundedExecution {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        match work {
            ExecutionWork::Work { units } => self.work.set(
                self.work
                    .get()
                    .checked_sub(units)
                    .ok_or(RunError::Refused(Incomplete::Allowance))?,
            ),
            ExecutionWork::Resource { requested_bytes } => self.resources.set(
                self.resources
                    .get()
                    .checked_sub(requested_bytes)
                    .ok_or(RunError::Refused(Incomplete::Allowance))?,
            ),
            _ => {}
        }
        Ok(())
    }
}

fn runtime_storage_error(error: CallableGuardStorageError<RunError>) -> RunError {
    match error {
        CallableGuardStorageError::Refused(error) => error,
        _ => RunError::Contract("unexpected callable storage preparation failure"),
    }
}

#[test]
fn completion_rejection_drains_children_before_partial_scopes_and_owner() {
    let db = setup_db();
    let revision = salsa::plumbing::current_revision(&db);
    let admission = FundedExecution {
        work: Cell::new(1_000_000),
        resources: Cell::new(1_000_000),
    };
    let mut after_rejection = None;
    for reject_completion in [true, false] {
        let journal = Rc::new(DisposalJournal::default());
        let state = journal.clone();
        let outcome = try_with_attempt(&db, 1_000_000, || {
            RegistryBuilder::new(&db, &admission)?
                .seal()?
                .run(|endpoint| async move {
                    let control = EndpointControl {
                        endpoint: &endpoint,
                    };
                    let guard = endpoint
                        .local_call(|| {
                            CallableRecursionGuard::new_admitted(&control)
                                .map_err(runtime_storage_error)
                        })
                        .await;
                    let owner = Rc::new(GuardOwner {
                        guard,
                        journal: state.clone(),
                    });
                    let mut outer = TrackedScope {
                        scope: Some(owner.guard.begin_scope()),
                        journal: state.clone(),
                        name: "outer",
                    };
                    let Some(outer_scope) = outer.scope.as_mut() else {
                        panic!("missing outer scope")
                    };
                    endpoint
                        .local_call(|| {
                            let prepared = outer_scope
                                .prepare_exact_insert(key(1), &control)
                                .map_err(runtime_storage_error)?;
                            assert!(committed(prepared.try_commit()));
                            Ok(())
                        })
                        .await;
                    let mut inner = TrackedScope {
                        scope: Some(owner.guard.begin_scope()),
                        journal: state.clone(),
                        name: "inner",
                    };
                    let Some(inner_scope) = inner.scope.as_mut() else {
                        panic!("missing inner scope")
                    };
                    endpoint
                        .local_call(|| {
                            let prepared = inner_scope
                                .prepare_exact_insert(key(2), &control)
                                .map_err(runtime_storage_error)?;
                            assert!(committed(prepared.try_commit()));
                            if reject_completion {
                                // A local callback cannot complete with an outstanding child demand.
                                // Queueing one rejects completion after commit while the enclosing
                                // future still owns both scopes.
                                let child = QueuedChild(owner.clone());
                                let pending = endpoint.demand(move || async move {
                                    let _child = child;
                                    std::future::pending::<RunResult<()>>().await
                                })?;
                                *state.pending.borrow_mut() = Some(pending);
                            }
                            Ok(())
                        })
                        .await;
                    state.continued.set(true);
                    Ok(())
                })
        });
        if reject_completion {
            assert!(matches!(
                outcome,
                Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
            ));
            assert!(!journal.continued.get());
            assert_eq!(
                &*journal.drops.borrow(),
                &["child", "inner", "outer", "owner"]
            );
            assert!(journal.pending.borrow_mut().take().is_some());
            after_rejection = Some(admission.work.get());
        } else {
            assert!(matches!(outcome, Ok(AttemptOutcome::Complete(Ok(())))));
            assert!(journal.continued.get());
            assert_eq!(&*journal.drops.borrow(), &["inner", "outer", "owner"]);
            assert!(Some(admission.work.get()) < after_rejection);
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}
