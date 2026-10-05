//! Owner-boundary controls use native memo slots and claims. Reentry calls the ordinary
//! fetch path under an active caller; it does not use the registered admission driver.

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::super::{CommitTarget, ExecutionStep, QueryExecution, TargetWrite};
use super::{
    Consumer, HeadObservation, HeadTraversal, Participant, ParticipantProgress, ParticipantWork,
};
use crate::attempt_probe::transfer_test_support::{
    self as trace, Action, Kind, TraceConfig, TransferTrace,
};
use crate::attempt_probe::{AttemptOutcome, MemoReuse, try_with_attempt};
use crate::cycle::{CycleHeads, IterationStamp};
use crate::function::maybe_changed_after::validation::{MemoValidity, Verification};
use crate::function::memo::{Memo, MemoHeader, SelectedMemo};
use crate::function::{
    ClaimGuard, ClaimResult, Configuration, IngredientImpl, Reentrancy, SyncOwner,
};
use crate::plumbing::AsId;
use crate::runtime::WaitResult;
use crate::zalsa::{Zalsa, ZalsaDatabase};
use crate::zalsa_local::QueryRevisions;
use crate::{Cycle, Database, DatabaseKeyIndex, Id};

// Vector counters observe requested capacities. The root counter records a
// reserve call and its conservative bound because ThinVec capacity is private.
// Neither measures allocation size classes or work inside the allocator.
#[derive(Clone, Copy, Debug, Default)]
struct StorageWork {
    requested_bytes: usize,
    relocated_records: usize,
    vector_requests: usize,
    root_requests: usize,
    root_request_bound: usize,
    evidence_scans: usize,
}

thread_local! {
    static STORAGE_WORK: Cell<Option<StorageWork>> = const { Cell::new(None) };
}

fn update_storage_work(update: impl FnOnce(&mut StorageWork)) {
    STORAGE_WORK.with(|slot| {
        if let Some(mut work) = slot.get() {
            update(&mut work);
            slot.set(Some(work));
        }
    });
}

pub(super) fn record_reservation(bytes: usize, capacity: usize) {
    if bytes != 0 {
        update_storage_work(|work| {
            work.requested_bytes += bytes;
            work.relocated_records += capacity;
            work.vector_requests += 1;
        });
    }
}

pub(super) fn record_root_reservation(root: usize, additional: usize) {
    update_storage_work(|work| {
        work.root_request_bound +=
            (root + additional) * size_of::<crate::cycle::CycleHead>() + 2 * size_of::<usize>();
        work.root_requests += 1;
    });
}

pub(super) fn record_evidence_scan() {
    update_storage_work(|work| work.evidence_scans += 1);
}

fn storage_work<T>(operation: impl FnOnce() -> T) -> (T, StorageWork) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            STORAGE_WORK.set(None);
        }
    }
    assert!(STORAGE_WORK.replace(Some(StorageWork::default())).is_none());
    let _reset = Reset;
    let value = operation();
    (value, STORAGE_WORK.get().unwrap())
}

#[crate::db]
#[derive(Clone, Default)]
struct Db {
    storage: crate::Storage<Self>,
}

#[crate::db]
impl Database for Db {}

#[crate::input]
struct Input {
    #[returns(copy)]
    value: u32,
}

#[crate::tracked(returns(copy), cycle_initial = initial, cycle_fn = recover)]
fn query(db: &dyn Database, input: Input) -> u32 {
    input.value(db)
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn owned_head(db: &dyn Database, input: Input) -> u32 {
    let support = crate::attempt_probe::current().unwrap();
    db.zalsa_local()
        .report_attempt_read(db.zalsa(), &support, false);
    input.value(db)
}

fn initial(_db: &dyn Database, _id: Id, _input: Input) -> u32 {
    0
}

fn recover(_db: &dyn Database, _cycle: &Cycle<'_>, _last: &u32, value: u32, _input: Input) -> u32 {
    value
}

fn claim<'db, C: Configuration<DbView = dyn Database>>(
    db: &'db dyn Database,
    ingredient: &'db IngredientImpl<C>,
    id: Id,
) -> ClaimGuard<'db> {
    match ingredient
        .sync_table
        .try_claim(db.zalsa(), db.zalsa_local(), id, Reentrancy::Deny)
    {
        ClaimResult::Claimed(claim) => claim,
        _ => panic!("fixture query is already claimed"),
    }
}

fn heads(zalsa: &Zalsa, keys: &[DatabaseKeyIndex]) -> CycleHeads {
    let mut heads = CycleHeads::default();
    let iteration = IterationStamp::initial(zalsa.runtime().cancellation_count());
    for &key in keys {
        heads.insert(key, iteration);
    }
    heads
}

fn seed<'db, C: Configuration<DbView = dyn Database>>(
    db: &'db dyn Database,
    ingredient: &'db IngredientImpl<C>,
    claim: &ClaimGuard<'db>,
    value: C::Output<'db>,
    heads: CycleHeads,
) -> &'db Memo<C> {
    let key = claim.database_key_index();
    let iteration = IterationStamp::initial(db.zalsa().runtime().cancellation_count());
    let mut revisions = QueryRevisions::fixpoint_initial(db.zalsa(), key, iteration);
    revisions.set_cycle_heads(heads, iteration);
    ingredient.insert_memo(
        db.zalsa(),
        key.key_index(),
        Memo::new(Some(value), db.zalsa().current_revision(), revisions),
        ingredient.memo_ingredient_index(db.zalsa(), key.key_index()),
    )
}

fn owner<'db, C: Configuration<DbView = dyn Database>>(
    db: &'db dyn Database,
    ingredient: &'db IngredientImpl<C>,
    claim: ClaimGuard<'db>,
    memo: &'db Memo<C>,
) -> Participant<'db, C> {
    Participant::cached(
        ingredient,
        db,
        claim,
        memo,
        Consumer::Validation,
        memo.header.revisions.cycle_heads().clone(),
    )
}

fn pending<C: Configuration>(progress: ParticipantProgress<'_, C>) -> Participant<'_, C> {
    match progress {
        ParticipantProgress::Pending(owner) => owner,
        _ => panic!("head expansion unexpectedly retired its source"),
    }
}

fn expand<C: Configuration>(owner: Participant<'_, C>) -> Participant<'_, C> {
    let work = owner.work().unwrap();
    assert!(matches!(work, ParticipantWork::Expand(..)));
    pending(owner.advance(work))
}

fn stored<'db, C: Configuration<DbView = dyn Database>>(
    db: &'db dyn Database,
    ingredient: &IngredientImpl<C>,
    id: Id,
) -> &'db Memo<C> {
    ingredient
        .get_memo_from_table_for(
            db.zalsa(),
            id,
            ingredient.memo_ingredient_index(db.zalsa(), id),
        )
        .unwrap()
}

fn collect<T>(f: impl FnOnce() -> T) -> (T, TransferTrace) {
    let result = trace::collect(
        TraceConfig {
            worker: 0,
            ordinal: Arc::new(AtomicUsize::new(0)),
        },
        f,
    );
    assert!(!result.1.broken);
    result
}

fn count(observations: &TransferTrace, kind: Kind, key: DatabaseKeyIndex) -> usize {
    observations
        .records
        .iter()
        .filter(|record| record.event.kind == kind && record.event.key == Some(key))
        .count()
}

fn reenter(db: &Db, input: Input, caller: DatabaseKeyIndex) -> (u32, TransferTrace) {
    let active = db.zalsa_local().push_query(caller);
    let result = collect(|| query(db, input));
    drop(active);
    result
}

fn assert_claim<C: Configuration<DbView = dyn Database>>(
    db: &dyn Database,
    ingredient: &IngredientImpl<C>,
    id: Id,
    serial: usize,
    owner: &Participant<'_, C>,
) {
    assert_eq!(
        owner.execution.as_ref().unwrap().claim_guard.test_serial(),
        serial
    );
    let state = ingredient.sync_table.test_transfer_state(id).unwrap();
    assert!(
        matches!(state.owner, SyncOwner::Thread(thread) if thread == std::thread::current().id())
    );
    assert!(!state.claimed_twice);
    assert!(db.zalsa_local().active_query().is_none());
}

#[test]
fn selected_cold_seed_replacement_requires_a_new_quote() {
    let db = Db::default();
    let source = Input::new(&db, 11);
    let head = Input::new(&db, 22);
    let leaf = Input::new(&db, 33);
    assert_eq!(query(&db, leaf), 33);
    let ingredient = query::fn_ingredient_(&db, db.zalsa());
    let source_key = ingredient.database_key_index(source.as_id());
    let head_key = ingredient.database_key_index(head.as_id());
    let leaf_key = ingredient.database_key_index(leaf.as_id());
    let head_claim = claim(&db, ingredient, head.as_id());
    let old_head = seed(
        &db,
        ingredient,
        &head_claim,
        7,
        heads(db.zalsa(), &[leaf_key]),
    );
    let source_claim = claim(&db, ingredient, source.as_id());
    let serial = source_claim.test_serial();
    let source_memo = seed(
        &db,
        ingredient,
        &source_claim,
        9,
        heads(db.zalsa(), &[head_key]),
    );
    let owner = expand(owner(&db, ingredient, source_claim, source_memo));
    let work = owner.work().unwrap();
    let ParticipantWork::Expand(selected, _) = &work else {
        panic!("expected a selected head")
    };
    assert!(std::ptr::eq(
        selected.selected.unwrap().1.unwrap(),
        &old_head.header
    ));

    let (value, observations) = reenter(&db, head, leaf_key);
    let replacement = stored(&db, ingredient, head.as_id());
    assert_eq!(value, 0);
    assert!(!std::ptr::eq(old_head, replacement));
    assert_eq!(count(&observations, Kind::ColdInitial, head_key), 1);
    assert_eq!(count(&observations, Kind::InitialInserted, head_key), 1);

    let owner = pending(owner.advance(work));
    assert_eq!(owner.traversal.next, 0);
    assert!(owner.traversal.observations.is_empty());
    assert_claim(&db, ingredient, source.as_id(), serial, &owner);
    let requote = owner.work().unwrap();
    let ParticipantWork::Expand(selected, _) = &requote else {
        panic!("expected a replacement quote")
    };
    assert!(std::ptr::eq(
        selected.selected.unwrap().1.unwrap(),
        &replacement.header
    ));
    let owner = pending(owner.advance(requote));
    assert!(owner.traversal.complete());
    assert_eq!(owner.traversal.observations.len(), 1);
    assert!(std::ptr::eq(
        owner.traversal.observations[0].header,
        &replacement.header
    ));
    assert_eq!(count(&observations, Kind::Terminal, source_key), 0);
    owner.abort();
    assert!(!head_claim.drop());
}

#[test]
fn source_reentry_reexecutes_after_replacement_or_head_pruning() {
    for includes_source in [false, true] {
        let db = Db::default();
        let source = Input::new(&db, 11);
        let caller = Input::new(&db, 22);
        let ingredient = query::fn_ingredient_(&db, db.zalsa());
        let source_key = ingredient.database_key_index(source.as_id());
        let caller_key = ingredient.database_key_index(caller.as_id());
        let source_claim = claim(&db, ingredient, source.as_id());
        let serial = source_claim.test_serial();
        let source_heads = if includes_source {
            vec![source_key, caller_key]
        } else {
            vec![caller_key]
        };
        let original = seed(
            &db,
            ingredient,
            &source_claim,
            7,
            heads(db.zalsa(), &source_heads),
        );
        let owner = owner(&db, ingredient, source_claim, original);
        let work = owner.work().unwrap();
        let (value, observations) = reenter(&db, source, caller_key);
        let current = stored(&db, ingredient, source.as_id());
        assert_eq!(std::ptr::eq(original, current), includes_source);
        assert_eq!(value, if includes_source { 7 } else { 0 });
        assert_eq!(
            current
                .header
                .revisions
                .cycle_heads()
                .iter()
                .map(|head| head.database_key_index)
                .collect::<Vec<_>>(),
            [source_key]
        );
        assert_eq!(
            count(&observations, Kind::InitialInserted, source_key),
            usize::from(!includes_source)
        );
        assert_eq!(
            count(&observations, Kind::ColdSelected, source_key),
            usize::from(includes_source)
        );
        let ParticipantProgress::Execute(execution) = owner.advance(work) else {
            panic!("changed source must be reexecuted")
        };
        assert_eq!(execution.claim_guard.test_serial(), serial);
        assert!(std::ptr::eq(execution.previous.as_ref().and_then(|previous| previous.semantic()).unwrap(), current));
        assert_eq!(count(&observations, Kind::Terminal, source_key), 0);
        execution.claim_guard.abort();
        assert!(
            ingredient
                .sync_table
                .test_transfer_state(source.as_id())
                .is_none()
        );
    }
}

#[test]
fn a_new_source_iteration_reexecutes_with_unchanged_heads() {
    let db = Db::default();
    let source = Input::new(&db, 11);
    let head = Input::new(&db, 22);
    let ingredient = query::fn_ingredient_(&db, db.zalsa());
    let source_key = ingredient.database_key_index(source.as_id());
    let head_key = ingredient.database_key_index(head.as_id());
    let source_claim = claim(&db, ingredient, source.as_id());
    let serial = source_claim.test_serial();
    let memo = seed(
        &db,
        ingredient,
        &source_claim,
        7,
        heads(db.zalsa(), &[head_key]),
    );
    let owner = owner(&db, ingredient, source_claim, memo);
    let work = owner.work().unwrap();
    let previous = memo.header.revisions.iteration();
    memo.header
        .revisions
        .prepare_iteration_count(source_key, previous.increment_iteration().unwrap())
        .unwrap()
        .unwrap()
        .publish();
    assert_eq!(
        memo.header
            .revisions
            .cycle_heads()
            .iter()
            .next()
            .unwrap()
            .iteration
            .load(),
        previous
    );
    assert!(std::ptr::eq(stored(&db, ingredient, source.as_id()), memo));
    let ParticipantProgress::Execute(execution) = owner.advance(work) else {
        panic!("a new source iteration must be reexecuted")
    };
    assert_eq!(execution.claim_guard.test_serial(), serial);
    assert!(std::ptr::eq(
        execution
            .previous
            .as_ref()
            .and_then(|previous| previous.semantic())
            .unwrap(),
        memo
    ));
    execution.claim_guard.abort();
}

#[derive(Clone, Copy, Debug)]
enum Change {
    Heads,
    HeadStamp,
    Iteration,
    Finality,
}

#[test]
fn observed_head_changes_restart_the_traversal_in_place() {
    for change in [
        Change::Heads,
        Change::HeadStamp,
        Change::Iteration,
        Change::Finality,
    ] {
        let db = Db::default();
        let source = Input::new(&db, 11);
        let head = Input::new(&db, 22);
        let leaf = Input::new(&db, 33);
        let other = Input::new(&db, 44);
        assert_eq!(query(&db, leaf), 33);
        assert_eq!(query(&db, other), 44);
        let ingredient = query::fn_ingredient_(&db, db.zalsa());
        let source_key = ingredient.database_key_index(source.as_id());
        let head_key = ingredient.database_key_index(head.as_id());
        let leaf_key = ingredient.database_key_index(leaf.as_id());
        let other_key = ingredient.database_key_index(other.as_id());
        let head_claim = claim(&db, ingredient, head.as_id());
        let child_keys = if matches!(change, Change::Heads) {
            [head_key, leaf_key]
        } else {
            [leaf_key, other_key]
        };
        let observed = seed(
            &db,
            ingredient,
            &head_claim,
            7,
            heads(db.zalsa(), &child_keys),
        );
        let source_claim = claim(&db, ingredient, source.as_id());
        let serial = source_claim.test_serial();
        let source_memo = seed(
            &db,
            ingredient,
            &source_claim,
            9,
            heads(db.zalsa(), &[head_key]),
        );
        let owner = expand(expand(owner(&db, ingredient, source_claim, source_memo)));
        assert_eq!(owner.traversal.observations.len(), 1);
        let work = owner.work().unwrap();
        let iteration = observed.header.revisions.iteration();
        let next = iteration.increment_iteration().unwrap();
        match change {
            Change::Heads => {
                let (value, observations) = reenter(&db, head, other_key);
                assert_eq!(value, 7);
                assert_eq!(count(&observations, Kind::ColdSelected, head_key), 1);
                assert_eq!(observed.header.revisions.cycle_heads().storage_len(), 2);
                assert_eq!(
                    observed
                        .header
                        .revisions
                        .cycle_heads()
                        .iter()
                        .map(|head| head.database_key_index)
                        .collect::<Vec<_>>(),
                    [head_key]
                );
            }
            Change::HeadStamp => {
                observed
                    .header
                    .revisions
                    .cycle_heads()
                    .prepare_iteration_store(leaf_key, next)
                    .unwrap()
                    .unwrap()
                    .publish();
                assert_eq!(observed.header.revisions.iteration(), iteration);
            }
            Change::Iteration => {
                observed
                    .header
                    .revisions
                    .prepare_iteration_count(head_key, next)
                    .unwrap()
                    .unwrap()
                    .publish();
                assert!(
                    observed
                        .header
                        .revisions
                        .cycle_heads()
                        .iter()
                        .all(|head| head.iteration.load() == iteration)
                );
            }
            Change::Finality => {
                let memo = db
                    .zalsa()
                    .lookup_ingredient(head_key.ingredient_index())
                    .as_function()
                    .unwrap()
                    .memo(db.zalsa(), head.as_id())
                    .unwrap();
                let target = CommitTarget {
                    key: head_key,
                    selected: Some((memo, observed.header.verified_at.load(), iteration)),
                    write: TargetWrite::Finalize,
                };
                assert!(target.is_current(db.zalsa()));
                target.publish();
                assert!(!observed.header.may_be_provisional());
            }
        }
        assert!(std::ptr::eq(
            stored(&db, ingredient, head.as_id()),
            observed
        ));
        let owner = pending(owner.advance(work));
        assert!(!owner.traversal.started, "{change:?}");
        assert!(owner.traversal.heads.is_empty(), "{change:?}");
        assert!(owner.traversal.observations.is_empty(), "{change:?}");
        assert_eq!(owner.traversal.next, 0);
        assert_claim(&db, ingredient, source.as_id(), serial, &owner);
        assert!(std::ptr::eq(
            stored(&db, ingredient, source.as_id()),
            source_memo
        ));
        assert_eq!(owner.key(), Some(source_key));
        owner.abort();
        assert!(!head_claim.drop());
    }
}

#[test]
fn a_zero_entry_head_still_quotes_observation_storage() {
    let db = Db::default();
    let source = Input::new(&db, 11);
    let head = Input::new(&db, 22);
    assert_eq!(query(&db, head), 22);
    let ingredient = query::fn_ingredient_(&db, db.zalsa());
    let head_key = ingredient.database_key_index(head.as_id());
    let source_claim = claim(&db, ingredient, source.as_id());
    let memo = seed(
        &db,
        ingredient,
        &source_claim,
        7,
        heads(db.zalsa(), &[head_key]),
    );
    let owner = expand(owner(&db, ingredient, source_claim, memo));
    assert_eq!(owner.traversal.observations.capacity(), 0);
    let work = owner.work().unwrap();
    let ParticipantWork::Expand(selected, _) = &work else {
        panic!("expected final head observation")
    };
    assert_eq!(selected.entries, 0);
    assert_eq!(selected.observations_capacity, 1);
    assert!(work.bytes() >= size_of::<HeadObservation<'_>>());
    assert!(work.units() > 0);
    let owner = pending(owner.advance(work));
    assert_eq!(owner.traversal.observations.len(), 1);
    assert_eq!(owner.traversal.observed_entries, 0);
    assert!(owner.traversal.complete());
    owner.abort();
}

#[derive(Debug)]
struct UnwindMarker;

#[test]
fn unwind_poisons_the_current_cold_seed_once() {
    let db = Db::default();
    let source = Input::new(&db, 11);
    let caller = Input::new(&db, 22);
    let ingredient = query::fn_ingredient_(&db, db.zalsa());
    let source_key = ingredient.database_key_index(source.as_id());
    let caller_key = ingredient.database_key_index(caller.as_id());
    let source_claim = claim(&db, ingredient, source.as_id());
    let original = seed(
        &db,
        ingredient,
        &source_claim,
        7,
        heads(db.zalsa(), &[caller_key]),
    );
    let owner = owner(&db, ingredient, source_claim, original);
    let _work = owner.work().unwrap();
    let (_, observations) = reenter(&db, source, caller_key);
    assert_eq!(count(&observations, Kind::InitialInserted, source_key), 1);
    let replacement = stored(&db, ingredient, source.as_id());
    assert!(!std::ptr::eq(original, replacement));
    let (result, observations) = collect(|| {
        catch_unwind(AssertUnwindSafe(move || {
            let _owner = owner;
            panic_any(UnwindMarker);
        }))
    });
    assert!(result.unwrap_err().is::<UnwindMarker>());
    let poisoned = stored(&db, ingredient, source.as_id());
    assert!(!std::ptr::eq(replacement, poisoned));
    assert!(poisoned.value().is_none());
    assert!(poisoned.header.may_be_provisional());
    assert_eq!(original.value(), Some(&7));
    assert_eq!(replacement.value(), Some(&0));
    assert_eq!(count(&observations, Kind::Poisoned, source_key), 1);
    assert_eq!(count(&observations, Kind::Terminal, source_key), 1);
    assert!(
        ingredient
            .sync_table
            .test_transfer_state(source.as_id())
            .is_none()
    );
    assert!(db.zalsa_local().active_query().is_none());
}

#[test]
fn pending_local_owner_panic_poisons_before_cancelled_waiter_release() {
    let db = Db::default();
    let source = Input::new(&db, 11);
    let ingredient = query::fn_ingredient_(&db, db.zalsa());
    let source_key = ingredient.database_key_index(source.as_id());
    let identity = Arc::new(());
    let ordinal = Arc::new(AtomicUsize::new(0));
    let ((result, serial, waiter_trace), observations) = trace::collect(
        TraceConfig {
            worker: 0,
            ordinal: ordinal.clone(),
        },
        || {
            let source_claim = claim(&db, ingredient, source.as_id());
            let serial = source_claim.test_serial();
            let memo = seed(
                &db,
                ingredient,
                &source_claim,
                7,
                heads(db.zalsa(), &[source_key]),
            );
            assert!(memo.header.revisions.attempt_support().is_none());
            let token = db.cancellation_token();
            token.cancel();
            assert!(db.zalsa_local().should_trigger_local_cancellation());
            let owner = owner(&db, ingredient, source_claim, memo);
            let _work = owner.work().unwrap();
            assert!(token.is_cancelled());
            assert!(!db.zalsa_local().should_trigger_local_cancellation());

            let waiter_db = db.clone();
            let (registered, registration) = mpsc::channel();
            let waiter = thread::spawn(move || {
                let (completed, observations) =
                    trace::collect(TraceConfig { worker: 1, ordinal }, || {
                        assert!(!waiter_db.cancellation_token().is_cancelled());
                        let ingredient = query::fn_ingredient_(&waiter_db, waiter_db.zalsa());
                        let ClaimResult::Running(running) = ingredient.sync_table.try_claim(
                            waiter_db.zalsa(),
                            waiter_db.zalsa_local(),
                            source.as_id(),
                            Reentrancy::Deny,
                        ) else {
                            panic!("waiter must encounter the retained participant claim");
                        };
                        // Running holds the graph lock until block_on installs the edge.
                        // The receiver's snapshot therefore proves registration before panic.
                        registered.send(thread::current().id()).unwrap();
                        running.block_on(waiter_db.zalsa())
                    });
                assert!(!completed);
                assert!(waiter_db.zalsa_local().active_query().is_none());
                observations
            });
            let waiter_id = registration.recv_timeout(Duration::from_secs(5)).unwrap();
            let graph = db.zalsa().runtime().test_transfer_graph_snapshot();
            assert!(!graph.edges.overflow && !graph.dependents.overflow);
            assert_eq!(
                graph
                    .edges
                    .entries
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>(),
                [(waiter_id, thread::current().id())]
            );
            let dependents: Vec<_> = graph.dependents.entries.into_iter().flatten().collect();
            assert_eq!(dependents.len(), 1);
            assert_eq!(dependents[0].0, source_key);
            assert!(!dependents[0].1.overflow);
            assert_eq!(
                dependents[0]
                    .1
                    .entries
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>(),
                [waiter_id]
            );
            let payload = identity.clone();
            let result = catch_unwind(AssertUnwindSafe(move || {
                let _owner = owner;
                panic_any(payload);
            }));
            assert!(token.is_cancelled());
            assert!(db.zalsa_local().should_trigger_local_cancellation());
            (result, serial, waiter.join().unwrap())
        },
    );
    assert!(Arc::ptr_eq(
        result.unwrap_err().downcast_ref::<Arc<()>>().unwrap(),
        &identity
    ));
    assert!(!observations.broken && !waiter_trace.broken);
    assert_eq!(count(&observations, Kind::Claim, source_key), 1);
    assert_eq!(count(&observations, Kind::Poisoned, source_key), 1);
    assert_eq!(count(&observations, Kind::Terminal, source_key), 1);
    let poison = observations
        .records
        .iter()
        .find(|record| record.event.kind == Kind::Poisoned && record.event.key == Some(source_key))
        .unwrap();
    let terminal = observations
        .records
        .iter()
        .find(|record| record.event.kind == Kind::Terminal && record.event.key == Some(source_key))
        .unwrap();
    assert!(poison.ordinal < terminal.ordinal);
    assert_eq!(terminal.event.serial, Some(serial));
    assert_eq!(terminal.event.action, Some(Action::Panic));
    // Cancelled here requires the pending Local request to be visible again at release.
    assert!(matches!(terminal.event.wait, Some(WaitResult::Cancelled)));
    let consumed: Vec<_> = waiter_trace
        .records
        .iter()
        .filter(|record| {
            record.event.kind == Kind::WaitConsumed && record.event.key == Some(source_key)
        })
        .collect();
    assert_eq!(consumed.len(), 1);
    assert!(matches!(
        consumed[0].event.wait,
        Some(WaitResult::Cancelled)
    ));
    assert!(terminal.ordinal < consumed[0].ordinal);
    let poisoned = stored(&db, ingredient, source.as_id());
    assert!(poisoned.value().is_none());
    assert!(poisoned.header.may_be_provisional());
    assert!(poisoned.header.revisions.attempt_support().is_none());
    assert_eq!(
        poisoned.header.verified_at.load(),
        db.zalsa().current_revision()
    );
    assert!(
        ingredient
            .sync_table
            .test_transfer_state(source.as_id())
            .is_none()
    );
    let graph = db.zalsa().runtime().test_transfer_graph_snapshot();
    assert!(graph.edges.is_empty() && graph.dependents.is_empty() && graph.pending.is_empty());
    assert!(graph.transferred.is_empty() && graph.reverse.is_empty());
    assert!(
        !graph.edges.overflow
            && !graph.dependents.overflow
            && !graph.pending.overflow
            && !graph.transferred.overflow
            && !graph.reverse.overflow
    );
    assert!(db.zalsa_local().active_query().is_none());
    assert!(db.zalsa_local().should_trigger_local_cancellation());
    db.zalsa_local().uncancel();
    assert!(!db.cancellation_token().is_cancelled());
    assert!(!db.zalsa_local().should_trigger_local_cancellation());
}

fn publish_final_body<'db, C: Configuration<DbView = dyn Database>>(
    db: &'db dyn Database,
    ingredient: &'db IngredientImpl<C>,
    claim: &ClaimGuard<'db>,
) -> &'db Memo<C> {
    // Keep the source claim while a separate completion installs the accepted result.
    // The real body and query frame supply final revisions, including its input reads.
    let key = claim.database_key_index();
    let active = db.zalsa_local().push_query(key);
    let value = C::execute(db, C::id_to_input(db.zalsa(), key.key_index()));
    let completed = active.pop(IterationStamp::default());
    ingredient.insert_memo(
        db.zalsa(),
        key.key_index(),
        Memo::new(
            Some(value),
            db.zalsa().current_revision(),
            completed.revisions,
        ),
        ingredient.memo_ingredient_index(db.zalsa(), key.key_index()),
    )
}

#[test]
fn unwind_preserves_a_separately_published_final_slot() {
    let db = Db::default();
    let source = Input::new(&db, 11);
    let head = Input::new(&db, 22);
    let ingredient = query::fn_ingredient_(&db, db.zalsa());
    let source_key = ingredient.database_key_index(source.as_id());
    let head_key = ingredient.database_key_index(head.as_id());
    let source_claim = claim(&db, ingredient, source.as_id());
    let original = seed(
        &db,
        ingredient,
        &source_claim,
        7,
        heads(db.zalsa(), &[head_key]),
    );
    let owner = owner(&db, ingredient, source_claim, original);
    let _work = owner.work().unwrap();
    let accepted = publish_final_body(
        &db,
        ingredient,
        &owner.execution.as_ref().unwrap().claim_guard,
    );
    assert!(!std::ptr::eq(original, accepted));
    assert!(!accepted.header.may_be_provisional());
    assert_eq!(accepted.value(), Some(&11));
    let (result, observations) = collect(|| {
        catch_unwind(AssertUnwindSafe(move || {
            let _owner = owner;
            panic_any(UnwindMarker);
        }))
    });
    assert!(result.unwrap_err().is::<UnwindMarker>());
    assert!(std::ptr::eq(
        stored(&db, ingredient, source.as_id()),
        accepted
    ));
    assert!(!accepted.header.may_be_provisional());
    assert_eq!(accepted.value(), Some(&11));
    assert_eq!(count(&observations, Kind::Poisoned, source_key), 0);
    assert_eq!(count(&observations, Kind::Terminal, source_key), 1);
    assert!(
        ingredient
            .sync_table
            .test_transfer_state(source.as_id())
            .is_none()
    );
    assert!(db.zalsa_local().active_query().is_none());
}

fn report_layout<C: Configuration>(_ingredient: &IngredientImpl<C>) {
    for (name, bytes) in [
        ("Participant", size_of::<Participant<'_, C>>()),
        ("ParticipantWork", size_of::<ParticipantWork<'_>>()),
        ("HeadTraversal", size_of::<HeadTraversal<'_>>()),
        ("HeadObservation", size_of::<HeadObservation<'_>>()),
        ("QueryExecution", size_of::<QueryExecution<'_, C>>()),
        ("ExecutionStep", size_of::<ExecutionStep<'_, C>>()),
        ("Verification", size_of::<Verification<'_>>()),
        ("MemoValidity", size_of::<MemoValidity>()),
        ("SelectedMemo", size_of::<SelectedMemo<'_, C>>()),
        ("ClaimGuard", size_of::<ClaimGuard<'_>>()),
        ("MemoHeader", size_of::<MemoHeader>()),
    ] {
        eprintln!("PARTICIPANT_LAYOUT {name}={bytes}");
    }
}

#[test]
fn report_native_participant_owner_layouts() {
    let db = Db::default();
    report_layout(query::fn_ingredient_(&db, db.zalsa()));
}

#[test]
fn a_foreign_final_head_cannot_certify_cached_provisional_support() {
    let db = Db::default();
    let source = Input::new(&db, 11);
    let head = Input::new(&db, 22);
    assert_eq!(
        try_with_attempt(&db, 100_000, || owned_head(&db, head)),
        Ok(AttemptOutcome::Complete(22))
    );
    let source_ingredient = query::fn_ingredient_(&db, db.zalsa());
    let head_ingredient = owned_head::fn_ingredient_(&db, db.zalsa());
    let head_memo = stored(&db, head_ingredient, head.as_id());
    assert!(head_memo.header.revisions.attempt_support().is_some());
    assert_eq!(
        head_memo.header.attempt_reuse(db.zalsa()),
        MemoReuse::Ordinary
    );
    assert!(!head_memo.header.may_be_provisional());
    let head_claim = claim(&db, head_ingredient, head.as_id());
    let head_key = head_claim.database_key_index();
    let source_claim = claim(&db, source_ingredient, source.as_id());
    let source_key = source_claim.database_key_index();
    let serial = source_claim.test_serial();
    let mut source_heads = CycleHeads::default();
    source_heads.insert(head_key, head_memo.header.revisions.iteration());
    let memo = seed(&db, source_ingredient, &source_claim, 7, source_heads);
    assert_eq!(memo.header.attempt_reuse(db.zalsa()), MemoReuse::Ordinary);
    assert!(!memo.header.same_attempt_owner(&head_memo.header));
    let owner = expand(expand(owner(&db, source_ingredient, source_claim, memo)));
    assert!(owner.traversal.complete());
    let work = owner.work().unwrap();
    assert!(matches!(work, ParticipantWork::Transfer(_)));
    let (progress, observations) = collect(|| owner.advance(work));
    let ParticipantProgress::Execute(execution) = progress else {
        panic!("foreign head support must require reexecution under the original claim");
    };
    assert_eq!(execution.claim_guard.test_serial(), serial);
    assert_eq!(count(&observations, Kind::TransferBegin, source_key), 0);
    assert_eq!(count(&observations, Kind::Terminal, source_key), 0);
    execution.claim_guard.abort();
    assert!(!head_claim.drop());
}

#[test]
fn self_root_has_two_actions_and_no_finish_storage_request() {
    let db = Db::default();
    let source = Input::new(&db, 11);
    let ingredient = query::fn_ingredient_(&db, db.zalsa());
    let key = ingredient.database_key_index(source.as_id());
    let stamp = IterationStamp::initial(db.zalsa().runtime().cancellation_count());
    let mut traversal = HeadTraversal::new(heads(db.zalsa(), &[key]), key, stamp);
    let work = traversal.work(db.zalsa()).unwrap();
    assert_eq!(work.units, 12);
    assert_eq!(work.bytes, size_of::<(DatabaseKeyIndex, IterationStamp)>());
    let (_, requested) = storage_work(|| traversal.advance(db.zalsa(), work));
    assert_eq!(requested.vector_requests, 1);
    assert_eq!(
        requested.requested_bytes,
        size_of::<(DatabaseKeyIndex, IterationStamp)>()
    );
    assert_eq!(requested.relocated_records, 0);
    assert_eq!(traversal.root_prefix_len, 1);
    assert_eq!(traversal.next, 1);
    assert!(traversal.complete());
    assert!(traversal.work(db.zalsa()).is_none());
    assert_eq!(traversal.finish_work(), Some(9));
    assert_eq!(traversal.finish_bytes(), Some(0));
    let ((root, maximum, depends_on_self), requested) = storage_work(|| traversal.finish());
    assert_eq!(requested.vector_requests, 0);
    assert_eq!(requested.root_requests, 0);
    assert_eq!(requested.requested_bytes, 0);
    assert_eq!(
        root.iter()
            .map(|head| head.database_key_index)
            .collect::<Vec<_>>(),
        [key]
    );
    assert_eq!(maximum, stamp);
    assert!(depends_on_self);
}

#[test]
fn root_prefix_counts_live_keys_and_self_normalization_preserves_order() {
    let db = Db::default();
    let inputs = [
        Input::new(&db, 11),
        Input::new(&db, 22),
        Input::new(&db, 33),
    ];
    let ingredient = query::fn_ingredient_(&db, db.zalsa());
    let [first, me, last] = inputs.map(|input| ingredient.database_key_index(input.as_id()));
    let stamp = IterationStamp::initial(db.zalsa().runtime().cancellation_count());
    let later = stamp.increment_iteration().unwrap();
    let first_claim = claim(&db, ingredient, inputs[0].as_id());
    seed(&db, ingredient, &first_claim, 7, heads(db.zalsa(), &[me]));
    assert_eq!(query(&db, inputs[2]), 33);
    let mut root = heads(db.zalsa(), &[first, me, last]);
    root.update_iteration_count_mut(me, later);
    let mut traversal = HeadTraversal::new(root, me, stamp);
    let work = traversal.work(db.zalsa()).unwrap();
    assert_eq!(work.entries, 3);
    assert_eq!(work.units, 21);
    traversal.advance(db.zalsa(), work);
    assert_eq!(
        traversal.heads,
        [(first, stamp), (me, later), (last, stamp)]
    );
    assert_eq!(traversal.root_prefix_len, 3);
    assert_eq!(traversal.next, 0);
    let work = traversal.work(db.zalsa()).unwrap();
    traversal.advance(db.zalsa(), work);
    assert_eq!(traversal.next, 2);
    assert_eq!(
        traversal.work(db.zalsa()).unwrap().selected.unwrap().0,
        last
    );
    assert_eq!(traversal.max_iteration, later);
    assert!(traversal.depends_on_self);
    traversal.restart();
    assert_eq!(traversal.root_prefix_len, 0);
    assert_eq!(traversal.next, 0);
    assert_eq!(traversal.max_iteration, stamp);
    assert!(!traversal.depends_on_self);
    assert!(!first_claim.drop());

    let root = heads(db.zalsa(), &[first, me, last]);
    root.remove_all_except(me);
    let mut traversal = HeadTraversal::new(root, me, stamp);
    let work = traversal.work(db.zalsa()).unwrap();
    assert_eq!(work.entries, 3);
    assert_eq!(work.units, 21);
    traversal.advance(db.zalsa(), work);
    assert_eq!(traversal.root.storage_len(), 3);
    assert_eq!(traversal.root_prefix_len, 1);
    assert_eq!(traversal.heads, [(me, stamp)]);
    assert!(traversal.complete());
    assert_eq!(traversal.finish_work(), Some(9));
    assert_eq!(traversal.finish_bytes(), Some(0));
}

#[test]
fn duplicate_external_heads_do_not_grow_the_destination() {
    let db = Db::default();
    let source = Input::new(&db, 11);
    let head = Input::new(&db, 22);
    let ingredient = query::fn_ingredient_(&db, db.zalsa());
    let me = ingredient.database_key_index(source.as_id());
    let key = ingredient.database_key_index(head.as_id());
    let stamp = IterationStamp::initial(db.zalsa().runtime().cancellation_count());
    let head_claim = claim(&db, ingredient, head.as_id());
    seed(&db, ingredient, &head_claim, 7, heads(db.zalsa(), &[key]));
    let mut traversal = HeadTraversal::new(heads(db.zalsa(), &[key]), me, stamp);
    traversal.advance(db.zalsa(), traversal.work(db.zalsa()).unwrap());
    let capacity = traversal.heads.capacity();
    let work = traversal.work(db.zalsa()).unwrap();
    let expected_relocation = if capacity < 2 { capacity } else { 0 };
    assert_eq!(work.units, 14 + expected_relocation);
    let permitted_bytes = work.bytes;
    let (_, requested) = storage_work(|| traversal.advance(db.zalsa(), work));
    assert_eq!(traversal.heads.capacity(), capacity);
    assert_eq!(requested.vector_requests, 2);
    assert_eq!(requested.relocated_records, 0);
    assert_eq!(
        requested.requested_bytes,
        size_of::<HeadObservation<'_>>() + size_of::<(DatabaseKeyIndex, IterationStamp)>()
    );
    assert!(requested.requested_bytes <= permitted_bytes);
    assert!(traversal.complete());
    assert_eq!(traversal.root_prefix_len, traversal.heads.len());
    assert_eq!(traversal.observed_entries, 1);
    assert_eq!(traversal.observations[0].backing_len, 1);
    assert_eq!(traversal.finish_work(), Some(12));
    assert_eq!(traversal.finish_bytes(), Some(0));
    let (_, requested) = storage_work(|| traversal.finish());
    assert_eq!(requested.root_requests, 0);
    assert!(!head_claim.drop());
}

#[test]
fn discovered_suffix_reactivates_a_removed_root_entry() {
    let db = Db::default();
    let source = Input::new(&db, 11);
    let head = Input::new(&db, 22);
    let removed = Input::new(&db, 33);
    assert_eq!(query(&db, removed), 33);
    let ingredient = query::fn_ingredient_(&db, db.zalsa());
    let me = ingredient.database_key_index(source.as_id());
    let key = ingredient.database_key_index(head.as_id());
    let restored = ingredient.database_key_index(removed.as_id());
    let stamp = IterationStamp::initial(db.zalsa().runtime().cancellation_count());
    let later = stamp.increment_iteration().unwrap();
    let head_claim = claim(&db, ingredient, head.as_id());
    let mut discovered = heads(db.zalsa(), &[restored, me]);
    discovered.update_iteration_count_mut(restored, later);
    seed(&db, ingredient, &head_claim, 7, discovered);
    let root = heads(db.zalsa(), &[key, restored]);
    root.remove_all_except(key);
    let mut traversal = HeadTraversal::new(root, me, stamp);
    traversal.advance(db.zalsa(), traversal.work(db.zalsa()).unwrap());
    assert_eq!(traversal.root_prefix_len, 1);
    let work = traversal.work(db.zalsa()).unwrap();
    let permitted_bytes = work.bytes;
    let (_, requested) = storage_work(|| traversal.advance(db.zalsa(), work));
    assert!(requested.requested_bytes <= permitted_bytes);
    assert_eq!(
        traversal.heads,
        [(key, stamp), (restored, later), (me, stamp)]
    );
    assert_eq!(traversal.next, 1);
    traversal.advance(db.zalsa(), traversal.work(db.zalsa()).unwrap());
    assert!(traversal.complete());
    assert_eq!(traversal.next, 3);
    assert_eq!(traversal.root_prefix_len, 1);
    // R=2, S=2, O=2, B=2: 8 + (1+2+2) + 2 + 4 + 8 + 2 + 4.
    assert_eq!(traversal.finish_work(), Some(33));
    let permitted_bytes = traversal.finish_bytes().unwrap();
    let ((root, maximum, depends_on_self), requested) = storage_work(|| traversal.finish());
    assert_eq!(requested.root_requests, 1);
    assert_eq!(requested.root_request_bound, permitted_bytes);
    assert_eq!(root.storage_len(), 3);
    assert_eq!(
        root.iter()
            .map(|head| head.database_key_index)
            .collect::<Vec<_>>(),
        [key, restored, me]
    );
    assert_eq!(
        root.iter()
            .find(|head| head.database_key_index == restored)
            .unwrap()
            .iteration
            .load(),
        later
    );
    assert_eq!(maximum, later);
    assert!(depends_on_self);
    assert!(!head_claim.drop());
}

#[test]
fn retained_backing_bounds_reject_evidence_before_scanning() {
    let db = Db::default();
    let source = Input::new(&db, 11);
    let head = Input::new(&db, 22);
    let removed = Input::new(&db, 33);
    let ingredient = query::fn_ingredient_(&db, db.zalsa());
    let head_key = ingredient.database_key_index(head.as_id());
    let removed_key = ingredient.database_key_index(removed.as_id());
    let head_claim = claim(&db, ingredient, head.as_id());
    seed(
        &db,
        ingredient,
        &head_claim,
        5,
        heads(db.zalsa(), &[head_key]),
    );
    let source_claim = claim(&db, ingredient, source.as_id());
    let serial = source_claim.test_serial();
    let original = heads(db.zalsa(), &[head_key, removed_key]);
    original.remove_all_except(head_key);
    let memo = seed(&db, ingredient, &source_claim, 7, original);
    let mut owner = expand(expand(owner(&db, ingredient, source_claim, memo)));
    assert_eq!(owner.traversal.observations.len(), 1);
    assert!(owner.traversal.is_current(db.zalsa()));
    // A deliberately smaller receipt tests the guard; it does not reproduce
    // structural growth of a published header's backing storage.
    owner.traversal.observations[0].backing_len = 0;
    let (current, scans) = storage_work(|| owner.traversal.is_current(db.zalsa()));
    assert!(!current);
    assert_eq!(scans.evidence_scans, 0);
    owner.traversal.observations[0].backing_len = 1;
    owner.traversal.root = heads(db.zalsa(), &[head_key]);
    assert_eq!(memo.header.revisions.cycle_heads().storage_len(), 2);
    assert_eq!(owner.traversal.root.storage_len(), 1);
    let work = owner.work().unwrap();
    let (progress, scans) = storage_work(|| owner.advance(work));
    assert_eq!(scans.evidence_scans, 0);
    let ParticipantProgress::Execute(execution) = progress else {
        panic!("an insufficient root bound must retain its claim for execution");
    };
    assert_eq!(execution.claim_guard.test_serial(), serial);
    assert!(std::ptr::eq(
        execution
            .previous
            .as_ref()
            .and_then(|previous| previous.semantic())
            .unwrap(),
        memo
    ));
    execution.claim_guard.abort();
    assert!(!head_claim.drop());
}

#[test]
fn completed_duplicate_closure_revalidates_before_zero_suffix_finish() {
    let db = Db::default();
    let source = Input::new(&db, 11);
    let head = Input::new(&db, 22);
    let ingredient = query::fn_ingredient_(&db, db.zalsa());
    let me = ingredient.database_key_index(source.as_id());
    let key = ingredient.database_key_index(head.as_id());
    let stamp = IterationStamp::initial(db.zalsa().runtime().cancellation_count());
    let head_claim = claim(&db, ingredient, head.as_id());
    let memo = seed(&db, ingredient, &head_claim, 7, heads(db.zalsa(), &[key]));
    let mut traversal = HeadTraversal::new(heads(db.zalsa(), &[key]), me, stamp);
    traversal.advance(db.zalsa(), traversal.work(db.zalsa()).unwrap());
    traversal.advance(db.zalsa(), traversal.work(db.zalsa()).unwrap());
    assert!(traversal.complete());
    assert_eq!(traversal.finish_bytes(), Some(0));
    let _finish_quote = traversal.finish_work().unwrap();
    // The terminal admission can change an observed stamp without changing the
    // selected allocation or the unique root prefix.
    let later = stamp.increment_iteration().unwrap();
    memo.header
        .revisions
        .cycle_heads()
        .prepare_iteration_store(key, later)
        .unwrap()
        .unwrap()
        .publish();
    assert!(!traversal.is_current(db.zalsa()));
    traversal.restart();
    assert_eq!(traversal.root_prefix_len, 0);
    traversal.advance(db.zalsa(), traversal.work(db.zalsa()).unwrap());
    traversal.advance(db.zalsa(), traversal.work(db.zalsa()).unwrap());
    assert!(traversal.complete());
    assert!(traversal.is_current(db.zalsa()));
    assert_eq!(traversal.observations[0].heads, [(key, later)]);
    let ((root, maximum, depends_on_self), requests) = storage_work(|| traversal.finish());
    assert_eq!(requests.root_requests, 0);
    assert_eq!(
        root.iter()
            .map(|head| head.database_key_index)
            .collect::<Vec<_>>(),
        [key]
    );
    assert_eq!(maximum, later);
    assert!(!depends_on_self);
    assert!(!head_claim.drop());
}

#[test]
fn insufficient_selected_bounds_make_no_storage_request() {
    let db = Db::default();
    let source = Input::new(&db, 11);
    let head = Input::new(&db, 22);
    let ingredient = query::fn_ingredient_(&db, db.zalsa());
    let me = ingredient.database_key_index(source.as_id());
    let key = ingredient.database_key_index(head.as_id());
    let stamp = IterationStamp::initial(db.zalsa().runtime().cancellation_count());
    let head_claim = claim(&db, ingredient, head.as_id());
    seed(&db, ingredient, &head_claim, 7, heads(db.zalsa(), &[key]));
    let mut traversal = HeadTraversal::new(heads(db.zalsa(), &[key]), me, stamp);
    let mut work = traversal.work(db.zalsa()).unwrap();
    work.entries = 0;
    let (_, requested) = storage_work(|| traversal.advance(db.zalsa(), work));
    assert_eq!(requested.requested_bytes, 0);
    assert!(!traversal.started);
    assert!(traversal.heads.is_empty());
    traversal.advance(db.zalsa(), traversal.work(db.zalsa()).unwrap());
    let mut work = traversal.work(db.zalsa()).unwrap();
    work.entries = 0;
    let (_, requested) = storage_work(|| traversal.advance(db.zalsa(), work));
    assert_eq!(requested.requested_bytes, 0);
    assert_eq!(traversal.next, 0);
    assert!(traversal.observations.is_empty());
    assert_eq!(traversal.heads, [(key, stamp)]);
    assert!(!head_claim.drop());
}
