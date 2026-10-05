//! Admission callbacks retain the real frame-free participant or active traversal owner.
//! Fixture setup installs claimed provisional memos; native reads supply cycle edges.

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::super::{
    Driver, Endpoint, ExecutionAdmission, ExecutionWork, Provider, RunError, RunOperation,
    RunResult, retire_participant_owned,
};
use crate::attempt_probe::transfer_test_support::{self as trace, Action, Kind, TraceConfig};
use crate::attempt_probe::{
    self, AttemptOutcome, AttemptSupport, Incomplete, MemoReuse, try_with_attempt,
};
use crate::cycle::{CycleHeads, IterationStamp};
use crate::function::execute::participant::{Consumer, Participant, ParticipantProgress};
use crate::function::memo::{Memo, SelectedMemo};
use crate::function::{
    ClaimGuard, ClaimResult, Configuration, IngredientImpl, Reentrancy, SyncOwner,
};
use crate::plumbing::AsId;
use crate::runtime::WaitResult;
use crate::zalsa::ZalsaDatabase;
use crate::zalsa_local::QueryRevisions;
use crate::{Cycle, Database, DatabaseKeyIndex, Id, Revision};

#[crate::db]
#[derive(Default)]
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

#[crate::tracked(returns(copy), attempt = ReturnOnly, cycle_initial = initial, cycle_fn = recover)]
fn query(db: &dyn Database, input: Input) -> u32 {
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

fn seed<'db, C: Configuration<DbView = dyn Database>>(
    db: &'db dyn Database,
    ingredient: &'db IngredientImpl<C>,
    claim: &ClaimGuard<'db>,
    value: C::Output<'db>,
    keys: &[DatabaseKeyIndex],
) -> &'db Memo<C> {
    let iteration = IterationStamp::initial(db.zalsa().runtime().cancellation_count());
    let mut heads = CycleHeads::default();
    for &key in keys {
        heads.insert(key, iteration);
    }
    let key = claim.database_key_index();
    let mut revisions = QueryRevisions::fixpoint_initial(db.zalsa(), key, iteration);
    revisions.set_cycle_heads(heads, iteration);
    ingredient.insert_memo(
        db.zalsa(),
        key.key_index(),
        Memo::new(Some(value), db.zalsa().current_revision(), revisions),
        ingredient.memo_ingredient_index(db.zalsa(), key.key_index()),
    )
}

fn enter_operation<'db, C: Configuration>(
    _ingredient: &IngredientImpl<C>,
    endpoint: &Endpoint<'_, 'db>,
) -> RunResult<RunOperation<'db>> {
    RunOperation::enter::<C>(endpoint.context.clone())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mutation {
    Replace,
    Prune,
    GrowGraph,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Finish {
    Reexecute,
    Refuse,
    Panic,
    PendingLocalPanic,
}

#[derive(Debug)]
struct NativePanic(Arc<()>);

struct Admission<'db> {
    db: &'db Db,
    source: Input,
    head: Input,
    caller: Input,
    additional: Input,
    mutation: Mutation,
    finish: Finish,
    target_resource: usize,
    armed: Cell<bool>,
    resources: Cell<usize>,
    fired: Cell<bool>,
    requoted: Cell<bool>,
    reexecuted: Cell<bool>,
    source_serial: Cell<Option<usize>>,
    old_identity: Cell<Option<usize>>,
    current_identity: Cell<Option<usize>>,
    support: RefCell<Option<AttemptSupport>>,
    panic_identity: Arc<()>,
}

impl Admission<'_> {
    fn assert_source_owned(&self) {
        let ingredient = query::fn_ingredient_(self.db, self.db.zalsa());
        let state = ingredient
            .sync_table
            .test_transfer_state(self.source.as_id())
            .unwrap();
        assert!(
            matches!(state.owner, SyncOwner::Thread(thread) if thread == std::thread::current().id())
        );
        assert!(!state.claimed_twice);
    }
}

impl ExecutionAdmission for Admission<'_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if !self.armed.get() || !matches!(work, ExecutionWork::Resource { .. }) {
            return Ok(());
        }
        let ordinal = self.resources.get();
        self.resources.set(ordinal + 1);
        self.assert_source_owned();
        assert!(self.db.zalsa_local().active_query().is_none());
        assert_eq!(attempt_probe::stack_depths().0, 1);
        let ingredient = query::fn_ingredient_(self.db, self.db.zalsa());
        let source_key = ingredient.database_key_index(self.source.as_id());
        let head_key = ingredient.database_key_index(self.head.as_id());

        if self.mutation == Mutation::GrowGraph {
            match ordinal {
                0 | 1 => return Ok(()),
                2 => {
                    assert!(!self.fired.replace(true));
                    // The root and head observations are complete. This valid transfer
                    // grows the graph after the participant has quoted its own transfer.
                    let additional = claim(self.db, ingredient, self.additional.as_id());
                    let quote = self.db.zalsa().runtime().retirement_quote(1).unwrap();
                    assert!(matches!(
                        additional.retire_participant(head_key, false, &quote),
                        Ok(false)
                    ));
                    let graph = self.db.zalsa().runtime().test_transfer_graph_snapshot();
                    assert!(!graph.transferred.overflow && !graph.reverse.overflow);
                    let child_key = ingredient.database_key_index(self.additional.as_id());
                    assert_eq!(
                        graph
                            .transferred
                            .entries
                            .into_iter()
                            .flatten()
                            .collect::<Vec<_>>(),
                        [(child_key, std::thread::current().id(), head_key)]
                    );
                    self.assert_source_owned();
                    return Ok(());
                }
                3 => {
                    assert!(self.fired.get());
                    assert!(!self.requoted.replace(true));
                    self.assert_source_owned();
                    return Err(RunError::Refused(Incomplete::Allowance));
                }
                _ => panic!("requote refusal did not retire the participant"),
            }
        }

        if ordinal < self.target_resource {
            return Ok(());
        }
        assert_eq!(ordinal, self.target_resource);
        assert!(!self.fired.replace(true));
        let before = ingredient
            .get_memo_from_table_for(
                self.db.zalsa(),
                self.source.as_id(),
                ingredient.memo_ingredient_index(self.db.zalsa(), self.source.as_id()),
            )
            .unwrap();
        assert_eq!(
            Some(std::ptr::from_ref(before).addr()),
            self.old_identity.get()
        );
        // The driver owns no query frame at this boundary. A native caller claim/frame
        // supplies the real read recipient for the callback's synchronous cold fetch.
        let caller_claim = claim(self.db, ingredient, self.caller.as_id());
        let caller = self
            .db
            .zalsa_local()
            .push_query(ingredient.database_key_index(self.caller.as_id()));
        let value = query(self.db, self.source);
        drop(caller);
        assert!(!caller_claim.drop());
        let current = ingredient
            .get_memo_from_table_for(
                self.db.zalsa(),
                self.source.as_id(),
                ingredient.memo_ingredient_index(self.db.zalsa(), self.source.as_id()),
            )
            .unwrap();
        assert_eq!(
            value,
            if self.mutation == Mutation::Prune {
                7
            } else {
                0
            }
        );
        assert_eq!(
            std::ptr::eq(before, current),
            self.mutation == Mutation::Prune
        );
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
        assert!(
            current
                .header
                .revisions
                .attempt_support()
                .unwrap()
                .same_owner(self.support.borrow().as_ref().unwrap())
        );
        self.current_identity
            .set(Some(std::ptr::from_ref(current).addr()));
        self.assert_source_owned();
        match self.finish {
            Finish::Reexecute => Ok(()),
            Finish::Refuse => Err(RunError::Refused(Incomplete::Allowance)),
            Finish::Panic => panic_any(NativePanic(self.panic_identity.clone())),
            Finish::PendingLocalPanic => {
                self.db.cancellation_token().cancel();
                assert!(!self.db.zalsa_local().should_trigger_local_cancellation());
                panic_any(NativePanic(self.panic_identity.clone()));
            }
        }
    }
}

fn run_case(mutation: Mutation, finish: Finish) {
    run_case_at(mutation, finish, 0);
}

fn run_case_at(mutation: Mutation, finish: Finish, target_resource: usize) {
    let db = Db::default();
    crate::attach(&db, || {
        run_case_attached(&db, mutation, finish, target_resource);
    });
    assert!(!db.cancellation_token().is_cancelled());
    assert!(!db.zalsa_local().should_trigger_local_cancellation());
}

fn run_case_attached(db: &Db, mutation: Mutation, finish: Finish, target_resource: usize) {
    let source = Input::new(db, 11);
    let head = Input::new(db, 22);
    let caller = Input::new(db, 33);
    let additional = Input::new(db, 44);
    let ingredient = query::fn_ingredient_(db, db.zalsa());
    let source_key = ingredient.database_key_index(source.as_id());
    let head_key = ingredient.database_key_index(head.as_id());
    let admission = Admission {
        db,
        source,
        head,
        caller,
        additional,
        mutation,
        finish,
        target_resource,
        armed: Cell::new(false),
        resources: Cell::new(0),
        fired: Cell::new(false),
        requoted: Cell::new(false),
        reexecuted: Cell::new(false),
        source_serial: Cell::new(None),
        old_identity: Cell::new(None),
        current_identity: Cell::new(None),
        support: RefCell::new(None),
        panic_identity: Arc::new(()),
    };
    let db_ref = db;
    let admission_ref = &admission;
    let (outcome, observations) = trace::collect(
        TraceConfig {
            worker: 0,
            ordinal: Arc::new(AtomicUsize::new(0)),
        },
        || {
            catch_unwind(AssertUnwindSafe(|| {
                try_with_attempt(db, 100_000, || {
                    Driver::run_with_admission(db, &admission, |endpoint| async move {
                        let operation = enter_operation(ingredient, &endpoint)?;
                        let head_claim = claim(db_ref, ingredient, head.as_id());
                        seed(db_ref, ingredient, &head_claim, 5, &[head_key]);
                        let source_claim = claim(db_ref, ingredient, source.as_id());
                        admission_ref
                            .source_serial
                            .set(Some(source_claim.test_serial()));
                        let root_heads = if mutation == Mutation::Prune {
                            vec![source_key, head_key]
                        } else {
                            vec![head_key]
                        };
                        let memo = seed(db_ref, ingredient, &source_claim, 7, &root_heads);
                        admission_ref
                            .old_identity
                            .set(Some(std::ptr::from_ref(memo).addr()));
                        *admission_ref.support.borrow_mut() =
                            Some(memo.header.revisions.attempt_support().unwrap().clone());
                        let owner = Participant::cached(
                            ingredient,
                            db_ref,
                            source_claim,
                            memo,
                            Consumer::Validation,
                            memo.header.revisions.cycle_heads().clone(),
                        );
                        admission_ref.armed.set(true);
                        let progress = retire_participant_owned(&endpoint, &operation, owner).await;
                        let ParticipantProgress::Execute(execution) = progress else {
                            panic!("a changed participant must not select a provisional return");
                        };
                        assert_eq!(finish, Finish::Reexecute);
                        assert_eq!(
                            Some(execution.claim_guard.test_serial()),
                            admission_ref.source_serial.get()
                        );
                        let current = ingredient
                            .get_memo_from_table_for(
                                db_ref.zalsa(),
                                source.as_id(),
                                ingredient.memo_ingredient_index(db_ref.zalsa(), source.as_id()),
                            )
                            .unwrap();
                        assert_eq!(
                            Some(std::ptr::from_ref(current).addr()),
                            admission_ref.current_identity.get()
                        );
                        assert!(std::ptr::eq(execution.previous.as_ref().and_then(|previous| previous.semantic()).unwrap(), current));
                        admission_ref.assert_source_owned();
                        assert!(operation.is_current());
                        assert!(!admission_ref.reexecuted.replace(true));
                        execution.claim_guard.abort();
                        assert!(!head_claim.drop());
                        Ok(())
                    })
                })
            }))
        },
    );
    assert!(!observations.broken);
    assert!(admission.fired.get());
    match finish {
        Finish::Reexecute => assert_eq!(outcome.unwrap(), Ok(AttemptOutcome::Complete(Ok(())))),
        Finish::Refuse => assert_eq!(
            outcome.unwrap(),
            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
        ),
        Finish::Panic | Finish::PendingLocalPanic => {
            let payload = outcome.unwrap_err();
            let native = payload
                .downcast_ref::<NativePanic>()
                .expect("the original native panic leaves the callback driver");
            assert!(Arc::ptr_eq(&native.0, &admission.panic_identity));
        }
    }
    assert_eq!(admission.reexecuted.get(), finish == Finish::Reexecute);
    let source_records: Vec<_> = observations
        .records
        .iter()
        .filter(|record| record.event.key == Some(source_key))
        .collect();
    assert_eq!(
        source_records
            .iter()
            .filter(|record| record.event.kind == Kind::Claim)
            .count(),
        1
    );
    let terminals: Vec<_> = source_records
        .iter()
        .filter(|record| record.event.kind == Kind::Terminal)
        .collect();
    assert_eq!(terminals.len(), 1);
    assert_eq!(terminals[0].event.serial, admission.source_serial.get());
    assert_eq!(
        terminals[0].event.action,
        Some(
            if matches!(finish, Finish::Panic | Finish::PendingLocalPanic) {
                Action::Panic
            } else {
                Action::Abort
            }
        )
    );
    assert!(matches!(
        (finish, terminals[0].event.wait),
        (Finish::Panic, Some(WaitResult::Panicked))
            | (
                Finish::Reexecute | Finish::Refuse | Finish::PendingLocalPanic,
                Some(WaitResult::Cancelled)
            )
    ));
    assert!(!source_records.iter().any(|record| matches!(
        record.event.kind,
        Kind::TransferBegin | Kind::TransferEnd | Kind::Refetch
    )));
    assert_eq!(
        source_records
            .iter()
            .filter(|record| record.event.kind == Kind::Poisoned)
            .count(),
        usize::from(matches!(finish, Finish::Panic | Finish::PendingLocalPanic))
    );
    let current = ingredient
        .get_memo_from_table_for(
            db.zalsa(),
            source.as_id(),
            ingredient.memo_ingredient_index(db.zalsa(), source.as_id()),
        )
        .unwrap();
    assert!(current.header.may_be_provisional());
    assert_eq!(current.header.attempt_reuse(db.zalsa()), MemoReuse::Stale);
    assert!(
        current
            .header
            .revisions
            .attempt_support()
            .unwrap()
            .same_owner(admission.support.borrow().as_ref().unwrap())
    );
    if matches!(finish, Finish::Panic | Finish::PendingLocalPanic) {
        assert!(current.value().is_none());
        assert_ne!(
            Some(std::ptr::from_ref(current).addr()),
            admission.current_identity.get()
        );
    } else if mutation != Mutation::GrowGraph {
        assert_eq!(
            Some(std::ptr::from_ref(current).addr()),
            admission.current_identity.get()
        );
        assert_eq!(
            current.value(),
            Some(if mutation == Mutation::Prune { &7 } else { &0 })
        );
    }
    if finish != Finish::Reexecute {
        assert!(current.header.has_incomplete_attempt());
    }
    if finish == Finish::Refuse {
        assert_eq!(
            admission.support.borrow().as_ref().unwrap().reason(),
            Some(Incomplete::Allowance)
        );
    }
    if mutation == Mutation::GrowGraph {
        assert_eq!(admission.resources.get(), 4);
        assert!(admission.requoted.get());
        assert_eq!(
            Some(std::ptr::from_ref(current).addr()),
            admission.old_identity.get()
        );
        assert_eq!(current.value(), Some(&7));
    } else {
        assert_eq!(admission.resources.get(), target_resource + 1);
        assert_eq!(
            source_records
                .iter()
                .filter(|record| record.event.kind == Kind::ColdInitial)
                .count(),
            usize::from(mutation == Mutation::Replace)
        );
        assert_eq!(
            source_records
                .iter()
                .filter(|record| record.event.kind == Kind::InitialInserted)
                .count(),
            usize::from(mutation == Mutation::Replace)
        );
        assert_eq!(
            source_records
                .iter()
                .filter(|record| record.event.kind == Kind::ColdSelected)
                .count(),
            usize::from(mutation == Mutation::Prune)
        );
    }
    assert!(
        ingredient
            .sync_table
            .test_transfer_state(source.as_id())
            .is_none()
    );
    assert!(
        ingredient
            .sync_table
            .test_transfer_state(head.as_id())
            .is_none()
    );
    assert!(
        ingredient
            .sync_table
            .test_transfer_state(caller.as_id())
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
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(attempt_probe::current().is_none());
    assert_eq!(db.zalsa().attempt_operations.load(Ordering::SeqCst), 0);
    if finish == Finish::PendingLocalPanic {
        assert!(db.cancellation_token().is_cancelled());
        assert!(db.zalsa_local().should_trigger_local_cancellation());
        let poison = source_records
            .iter()
            .find(|record| record.event.kind == Kind::Poisoned)
            .unwrap();
        assert!(poison.ordinal < terminals[0].ordinal);
    }
}

#[test]
fn admission_cold_reentry_returns_the_original_claim_for_reexecution() {
    for mutation in [Mutation::Replace, Mutation::Prune] {
        run_case(mutation, Finish::Reexecute);
    }
}

#[test]
fn admission_refusal_and_panic_clean_the_current_replacement() {
    for finish in [Finish::Refuse, Finish::Panic] {
        run_case(Mutation::Replace, finish);
    }
}

#[test]
fn admission_transfer_growth_requotes_before_source_commit() {
    run_case(Mutation::GrowGraph, Finish::Refuse);
}

#[test]
fn later_head_and_retirement_admissions_revalidate_the_source() {
    for target_resource in [1, 2] {
        run_case_at(Mutation::Replace, Finish::Reexecute, target_resource);
    }
}

#[test]
fn root_admission_panic_preserves_pending_local_cleanup() {
    run_case(Mutation::Replace, Finish::PendingLocalPanic);
}

struct FinishAdmission<'db> {
    db: &'db Db,
    source: Input,
    head: Input,
    mutate: bool,
    armed: Cell<bool>,
    resources: Cell<usize>,
    pending_resource: Cell<Option<usize>>,
    admitted_work: RefCell<Vec<usize>>,
    observations: RefCell<Vec<ExecutionWork>>,
    calls: Cell<[usize; 4]>,
    head_identity: Cell<usize>,
    revision: Cell<Option<Revision>>,
    iteration: Cell<Option<IterationStamp>>,
    original_stamp: Cell<Option<IterationStamp>>,
    support: RefCell<Option<AttemptSupport>>,
}

impl FinishAdmission<'_> {
    fn assert_owner(&self) {
        let ingredient = query::fn_ingredient_(self.db, self.db.zalsa());
        let source = ingredient.database_key_index(self.source.as_id());
        let head = ingredient.database_key_index(self.head.as_id());
        assert_eq!(
            self.db.zalsa_local().try_with_query_stack(|stack| {
                stack
                    .iter()
                    .map(|frame| frame.database_key_index)
                    .collect::<Vec<_>>()
            }),
            Some(vec![head, source]),
            "terminal revalidation must restart beneath the retained source frame"
        );
        assert_eq!(attempt_probe::stack_depths().0, 2);
        assert_eq!(self.calls.get(), [1, 0, 0, 0]);
        assert!(
            attempt_probe::current()
                .unwrap()
                .same_owner(self.support.borrow().as_ref().unwrap())
        );
        assert!(self.support.borrow().as_ref().unwrap().reason().is_none());
        for input in [self.head, self.source] {
            let state = ingredient
                .sync_table
                .test_transfer_state(input.as_id())
                .unwrap();
            assert!(
                matches!(state.owner, SyncOwner::Thread(thread) if thread == std::thread::current().id())
            );
            assert!(!state.claimed_twice);
        }
        assert!(
            ingredient
                .get_memo_from_table_for(
                    self.db.zalsa(),
                    self.source.as_id(),
                    ingredient.memo_ingredient_index(self.db.zalsa(), self.source.as_id()),
                )
                .is_none(),
            "traversal restart must precede source publication"
        );
    }

    fn assert_head(&self, stamp: IterationStamp) {
        let ingredient = query::fn_ingredient_(self.db, self.db.zalsa());
        let memo = ingredient
            .get_memo_from_table_for(
                self.db.zalsa(),
                self.head.as_id(),
                ingredient.memo_ingredient_index(self.db.zalsa(), self.head.as_id()),
            )
            .unwrap();
        assert_eq!(std::ptr::from_ref(memo).addr(), self.head_identity.get());
        assert_eq!(memo.value(), Some(&7));
        assert_eq!(Some(memo.header.verified_at.load()), self.revision.get());
        assert_eq!(
            Some(memo.header.revisions.iteration()),
            self.iteration.get()
        );
        assert!(memo.header.may_be_provisional());
        assert_eq!(
            memo.header.attempt_reuse(self.db.zalsa()),
            MemoReuse::Ordinary
        );
        assert!(
            memo.header
                .revisions
                .attempt_support()
                .unwrap()
                .same_owner(self.support.borrow().as_ref().unwrap())
        );
        assert_eq!(memo.header.revisions.cycle_heads().storage_len(), 1);
        assert_eq!(
            memo.header
                .revisions
                .cycle_heads()
                .iter()
                .map(|head| (head.database_key_index, head.iteration.load()))
                .collect::<Vec<_>>(),
            [(ingredient.database_key_index(self.head.as_id()), stamp)]
        );
    }
}

impl ExecutionAdmission for FinishAdmission<'_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if !self.armed.get() {
            return Ok(());
        }
        self.observations.borrow_mut().push(work);
        match work {
            ExecutionWork::Resource { requested_bytes } => {
                self.assert_owner();
                let ordinal = self.resources.get();
                assert!(self.pending_resource.replace(Some(ordinal)).is_none());
                self.resources.set(ordinal + 1);
                let pair_bytes = size_of::<(DatabaseKeyIndex, IterationStamp)>();
                match ordinal {
                    0 => assert_eq!(requested_bytes, pair_bytes),
                    1 => assert!(requested_bytes > 3 * pair_bytes),
                    2 | 3 | 5 => assert_eq!(requested_bytes, 0),
                    4 => assert_eq!(requested_bytes, 3 * pair_bytes),
                    _ => panic!("unexpected traversal action after terminal revalidation"),
                }
                let original = self.original_stamp.get().unwrap();
                let changed = original.increment_iteration().unwrap();
                self.assert_head(if self.mutate && ordinal > 2 {
                    changed
                } else {
                    original
                });
                if ordinal == 2 && self.mutate {
                    let ingredient = query::fn_ingredient_(self.db, self.db.zalsa());
                    let memo = ingredient
                        .get_memo_from_table_for(
                            self.db.zalsa(),
                            self.head.as_id(),
                            ingredient.memo_ingredient_index(self.db.zalsa(), self.head.as_id()),
                        )
                        .unwrap();
                    // Let this terminal admission succeed. Only production completion
                    // can now discard the stale observation and repeat the traversal.
                    memo.header
                        .revisions
                        .cycle_heads()
                        .prepare_iteration_store(
                            ingredient.database_key_index(self.head.as_id()),
                            changed,
                        )
                        .unwrap()
                        .unwrap()
                        .publish();
                    self.assert_head(changed);
                }
                if ordinal == 5 {
                    assert!(self.mutate);
                    assert_eq!(*self.admitted_work.borrow(), [12, 15, 12, 12, 15]);
                    return Err(RunError::Refused(Incomplete::Allowance));
                }
            }
            ExecutionWork::Work { units } => {
                if let Some(ordinal) = self.pending_resource.take() {
                    self.assert_owner();
                    assert_eq!(units, if ordinal == 1 || ordinal == 4 { 15 } else { 12 });
                    self.admitted_work.borrow_mut().push(units);
                    if ordinal == 2 && !self.mutate {
                        self.armed.set(false);
                    }
                } else {
                    // The body callback reports its one-unit completion after arming,
                    // before the first traversal resource admission.
                    assert_eq!(self.resources.get(), 0);
                    assert_eq!(units, 1);
                }
            }
            ExecutionWork::Poll => {}
            ExecutionWork::Task { .. } => panic!("a retained traversal must not run another body"),
        }
        Ok(())
    }
}

struct FinishProvider<'run, 'db, C: Configuration> {
    ingredient: &'db IngredientImpl<C>,
    admission: &'run FinishAdmission<'db>,
}

impl<'run, 'db: 'run, C> Provider<'run, 'db, C> for FinishProvider<'run, 'db, C>
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Input, Output<'a> = u32>,
{
    // Input conversion constructs a handle; output equality compares u32.
    fixture_native_value!(provider, 'run, 'db, C, 1);

    type Output = u32;

    async fn body(
        &self,
        db: &'db dyn Database,
        input: Input,
        _endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<u32> {
        assert_eq!(input.as_id(), self.admission.source.as_id());
        let mut calls = self.admission.calls.get();
        calls[0] += 1;
        self.admission.calls.set(calls);
        assert_eq!(calls, [1, 0, 0, 0]);
        let value = query(db, self.admission.head);
        assert_eq!(value, 7);
        assert!(!self.admission.armed.replace(true));
        self.admission.assert_owner();
        self.admission
            .assert_head(self.admission.original_stamp.get().unwrap());
        Ok(value)
    }

    async fn initial(
        &self,
        _db: &'db dyn Database,
        _id: Id,
        _input: Input,
        _endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<u32> {
        let mut calls = self.admission.calls.get();
        calls[1] += 1;
        self.admission.calls.set(calls);
        Err(RunError::Contract(
            "terminal fixture unexpectedly requested an initial value",
        ))
    }

    async fn recover(
        &self,
        _db: &'db dyn Database,
        _cycle: &Cycle<'_>,
        _last: &u32,
        _value: u32,
        _input: Input,
        _endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<u32> {
        let mut calls = self.admission.calls.get();
        calls[2] += 1;
        self.admission.calls.set(calls);
        Err(RunError::Contract(
            "terminal fixture unexpectedly entered cycle recovery",
        ))
    }

    async fn complete(
        &self,
        db: &'db dyn Database,
        selected: Option<SelectedMemo<'db, C>>,
        _endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<u32> {
        assert!(
            !self.admission.mutate,
            "stale terminal evidence reached completion"
        );
        let mut calls = self.admission.calls.get();
        calls[3] += 1;
        self.admission.calls.set(calls);
        let selected = selected.ok_or(RunError::RequiresFetch)?;
        Ok(*self.ingredient.record_memo_read(
            db.zalsa(),
            db.zalsa_local(),
            self.admission.source.as_id(),
            &selected,
        ))
    }
}

fn run_terminal_finish(mutate: bool) {
    let db = Db::default();
    let source = Input::new(&db, 11);
    let head = Input::new(&db, 22);
    let ingredient = query::fn_ingredient_(&db, db.zalsa());
    let source_key = ingredient.database_key_index(source.as_id());
    let head_key = ingredient.database_key_index(head.as_id());
    let admission = FinishAdmission {
        db: &db,
        source,
        head,
        mutate,
        armed: Cell::new(false),
        resources: Cell::new(0),
        pending_resource: Cell::new(None),
        admitted_work: RefCell::new(Vec::new()),
        observations: RefCell::new(Vec::new()),
        calls: Cell::new([0; 4]),
        head_identity: Cell::new(0),
        revision: Cell::new(None),
        iteration: Cell::new(None),
        original_stamp: Cell::new(None),
        support: RefCell::new(None),
    };
    let db_ref = &db;
    let admission_ref = &admission;
    let (outcome, observations) = trace::collect(
        TraceConfig {
            worker: 0,
            ordinal: Arc::new(AtomicUsize::new(0)),
        },
        || {
            try_with_attempt(&db, 100_000, || {
                Driver::run_with_admission(&db, &admission, |endpoint| async move {
                    // Driver entry requires an empty stack. Its root future owns H while
                    // the real child task executes S and reads H's provisional value.
                    let head_operation = enter_operation(ingredient, &endpoint)?;
                    let head_claim = claim(db_ref, ingredient, head.as_id());
                    let memo = seed(db_ref, ingredient, &head_claim, 7, &[head_key]);
                    admission_ref
                        .head_identity
                        .set(std::ptr::from_ref(memo).addr());
                    admission_ref
                        .revision
                        .set(Some(memo.header.verified_at.load()));
                    admission_ref
                        .iteration
                        .set(Some(memo.header.revisions.iteration()));
                    admission_ref.original_stamp.set(Some(
                        memo.header
                            .revisions
                            .cycle_heads()
                            .iter()
                            .next()
                            .unwrap()
                            .iteration
                            .load(),
                    ));
                    *admission_ref.support.borrow_mut() =
                        Some(memo.header.revisions.attempt_support().unwrap().clone());
                    let head_frame = db_ref.zalsa_local().push_query(head_key);
                    let provider = FinishProvider {
                        ingredient,
                        admission: admission_ref,
                    };
                    let value = endpoint
                        .execute(ingredient, db_ref, source.as_id(), None, provider)?
                        .await?;
                    assert!(!mutate, "mutated finish returned instead of restarting");
                    assert_eq!(value, 7);
                    assert!(head_operation.is_current());
                    assert!(head_frame.is_current_at_depth(1));
                    let state = ingredient
                        .sync_table
                        .test_transfer_state(source.as_id())
                        .unwrap();
                    assert!(matches!(state.owner, SyncOwner::Transferred));
                    let graph = db_ref.zalsa().runtime().test_transfer_graph_snapshot();
                    assert_eq!(
                        graph
                            .transferred
                            .entries
                            .into_iter()
                            .flatten()
                            .collect::<Vec<_>>(),
                        [(source_key, std::thread::current().id(), head_key)]
                    );
                    drop(head_frame);
                    assert!(!head_claim.drop());
                    drop(head_operation);
                    Ok(value)
                })
            })
        },
    );
    assert!(!observations.broken);
    eprintln!(
        "TERMINAL_FINISH mutate={mutate} observations={:?}",
        admission.observations.borrow()
    );
    if mutate {
        assert_eq!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
        );
        assert_eq!(admission.resources.get(), 6);
        assert_eq!(admission.pending_resource.get(), Some(5));
        assert_eq!(*admission.admitted_work.borrow(), [12, 15, 12, 12, 15]);
        assert_eq!(admission.calls.get(), [1, 0, 0, 0]);
        assert_eq!(
            admission.support.borrow().as_ref().unwrap().reason(),
            Some(Incomplete::Allowance)
        );
    } else {
        assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(7))));
        assert_eq!(admission.resources.get(), 3);
        assert_eq!(admission.pending_resource.get(), None);
        assert_eq!(*admission.admitted_work.borrow(), [12, 15, 12]);
        assert_eq!(admission.calls.get(), [1, 0, 0, 1]);
        assert!(
            admission
                .support
                .borrow()
                .as_ref()
                .unwrap()
                .reason()
                .is_none()
        );
    }
    let records: Vec<_> = observations
        .records
        .iter()
        .filter(|record| record.event.key == Some(source_key))
        .collect();
    let claims: Vec<_> = records
        .iter()
        .filter(|record| record.event.kind == Kind::Claim)
        .collect();
    let terminals: Vec<_> = records
        .iter()
        .filter(|record| record.event.kind == Kind::Terminal)
        .collect();
    assert_eq!(claims.len(), 1);
    assert_eq!(terminals.len(), 1);
    assert_eq!(claims[0].event.serial, terminals[0].event.serial);
    assert!(claims[0].ordinal < terminals[0].ordinal);
    if mutate {
        assert_eq!(terminals[0].event.action, Some(Action::Abort));
        assert!(matches!(
            terminals[0].event.wait,
            Some(WaitResult::Cancelled)
        ));
        assert!(!records.iter().any(|record| matches!(
            record.event.kind,
            Kind::RootPublished
                | Kind::TargetPublished
                | Kind::TransferBegin
                | Kind::TransferEnd
                | Kind::Refetch
                | Kind::Poisoned
        )));
        assert!(
            ingredient
                .get_memo_from_table_for(
                    db.zalsa(),
                    source.as_id(),
                    ingredient.memo_ingredient_index(db.zalsa(), source.as_id())
                )
                .is_none()
        );
    } else {
        assert_eq!(
            records
                .iter()
                .filter(|record| record.event.kind == Kind::RootPublished)
                .count(),
            1
        );
        assert_eq!(
            records
                .iter()
                .filter(|record| record.event.kind == Kind::TransferBegin)
                .count(),
            1
        );
    }
    assert!(
        ingredient
            .sync_table
            .test_transfer_state(head.as_id())
            .is_none()
    );
    if mutate {
        assert!(
            ingredient
                .sync_table
                .test_transfer_state(source.as_id())
                .is_none()
        );
    } else {
        // A released transfer retains a stale sync marker until its next claim.
        // The empty owner graph must make that marker immediately claimable.
        assert!(matches!(
            ingredient
                .sync_table
                .peek_claim(db.zalsa(), source.as_id(), Reentrancy::Deny),
            ClaimResult::Claimed(())
        ));
    }
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
    assert!(!db.zalsa_local().should_trigger_local_cancellation());
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(attempt_probe::current().is_none());
    assert_eq!(db.zalsa().attempt_operations.load(Ordering::SeqCst), 0);
}

#[test]
fn zero_suffix_terminal_admission_restarts_the_production_traversal() {
    run_terminal_finish(true);
}

#[test]
fn unchanged_zero_suffix_terminal_completes_to_its_enclosing_owner() {
    run_terminal_finish(false);
}
