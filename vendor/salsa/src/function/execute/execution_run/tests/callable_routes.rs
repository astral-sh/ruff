use std::cell::{Cell, RefCell};
use std::future::{Future, ready};
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
#[cfg(not(feature = "shuttle"))]
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

use super::super::registration::{
    CallableRoute, CallableRouteProvider, RegisteredRun, RegistryBuilder, TaskEndpoint,
};
use super::super::{ExecutionAdmission, ExecutionWork, RunError, RunResult, callback};
use super::{observation, validation_trace};
#[cfg(not(feature = "shuttle"))]
use crate::attempt_probe::transfer_test_support::{
    self as transfer_trace, Action, Kind, TraceConfig,
};
use crate::attempt_probe::{
    self, AttemptOutcome, AttemptSupport, Incomplete, MemoReuse, try_with_attempt,
};
use crate::function::maybe_changed_after::VerifyResult;
use crate::function::{ClaimResult, Configuration, FunctionIngredient, IngredientImpl, Reentrancy};
use crate::plumbing::AsId;
use crate::prepared_source_probe::{self, Read, Stamp, Status};
#[cfg(not(feature = "shuttle"))]
use crate::runtime::WaitResult;
use crate::zalsa::ZalsaDatabase;
use crate::{Cancelled, Cycle, Database, DatabaseKeyIndex, EventKind, Id, Revision, Setter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Recorded {
    Body(usize),
    Initial(usize),
    Recovery(usize, bool),
    Iteration(DatabaseKeyIndex),
    Finalized(DatabaseKeyIndex),
}

#[crate::db]
trait Db: Database {
    fn record(&self, event: Recorded);
}

#[crate::db]
#[derive(Clone)]
struct TestDb {
    storage: crate::Storage<Self>,
    recorded: Arc<Mutex<Vec<Recorded>>>,
    interned: Arc<Mutex<Vec<DatabaseKeyIndex>>>,
}

impl Default for TestDb {
    fn default() -> Self {
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let events = recorded.clone();
        let interned = Arc::new(Mutex::new(Vec::new()));
        let intern_events = interned.clone();
        Self {
            storage: crate::Storage::new(Some(Box::new(move |event| {
                if let EventKind::DidInternValue { key, .. } = event.kind {
                    intern_events.lock().unwrap().push(key);
                    return;
                }
                let event = match event.kind {
                    EventKind::WillIterateCycle { database_key, .. } => {
                        Recorded::Iteration(database_key)
                    }
                    EventKind::DidFinalizeCycle { database_key, .. } => {
                        Recorded::Finalized(database_key)
                    }
                    _ => return,
                };
                events.lock().unwrap().push(event);
            }))),
            recorded,
            interned,
        }
    }
}

#[crate::db]
impl Database for TestDb {}
#[crate::db]
impl Db for TestDb {
    fn record(&self, event: Recorded) {
        self.recorded.lock().unwrap().push(event);
    }
}

#[crate::input]
struct Input {
    #[returns(copy)]
    enabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, crate::SalsaValue)]
struct Flag {
    value: bool,
}

fn a_value(flag: Flag, enabled: bool) -> bool {
    flag.value && enabled
}
fn b_value(value: Option<bool>) -> Flag {
    Flag {
        value: value.unwrap_or(true),
    }
}
fn c_value(value: bool) -> Option<bool> {
    Some(value)
}

#[crate::tracked(returns(ref), attempt = ReturnOnly, cycle_initial = initial_a, cycle_fn = recover_a)]
fn a(db: &dyn Db, input: Input) -> bool {
    db.record(Recorded::Body(0));
    a_value(*b(db, input), input.enabled(db))
}
#[crate::tracked(returns(ref), attempt = ReturnOnly, cycle_initial = initial_b, cycle_fn = recover_b)]
fn b(db: &dyn Db, input: Input) -> Flag {
    db.record(Recorded::Body(1));
    b_value(*c(db, input))
}
#[crate::tracked(returns(ref), attempt = ReturnOnly, cycle_initial = initial_c, cycle_fn = recover_c)]
fn c(db: &dyn Db, input: Input) -> Option<bool> {
    db.record(Recorded::Body(2));
    c_value(*a(db, input))
}

fn initial_a(db: &dyn Db, _id: Id, _input: Input) -> bool {
    db.record(Recorded::Initial(0));
    true
}
fn initial_b(db: &dyn Db, _id: Id, _input: Input) -> Flag {
    db.record(Recorded::Initial(1));
    Flag { value: true }
}
fn initial_c(db: &dyn Db, _id: Id, _input: Input) -> Option<bool> {
    db.record(Recorded::Initial(2));
    Some(true)
}
fn recover_a(db: &dyn Db, _cycle: &Cycle<'_>, last: &bool, value: bool, _input: Input) -> bool {
    db.record(Recorded::Recovery(0, *last != value));
    value
}
fn recover_b(db: &dyn Db, _cycle: &Cycle<'_>, last: &Flag, value: Flag, _input: Input) -> Flag {
    db.record(Recorded::Recovery(1, *last != value));
    value
}
fn recover_c(
    db: &dyn Db,
    _cycle: &Cycle<'_>,
    last: &Option<bool>,
    value: Option<bool>,
    _input: Input,
) -> Option<bool> {
    db.record(Recorded::Recovery(2, *last != value));
    value
}

#[derive(Default)]
struct Admission {
    work: RefCell<Vec<ExecutionWork>>,
    refuse_next_resource: Cell<bool>,
}
impl ExecutionAdmission for Admission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.work.borrow_mut().push(work);
        if matches!(work, ExecutionWork::Resource { .. })
            && self.refuse_next_resource.replace(false)
        {
            return Err(RunError::Refused(Incomplete::Allowance));
        }
        Ok(())
    }
}

#[derive(Default)]
struct Owners {
    identity: Cell<u32>,
    drops: [Cell<usize>; 3],
    journal: RefCell<Vec<&'static str>>,
    premature_provider_drop: Cell<bool>,
    recovery_fault: Cell<bool>,
    native: Arc<()>,
    child_polled: Cell<bool>,
    recovery_drop: RefCell<Option<RecoverySnapshot>>,
    body_control: RefCell<Option<BodyControl>>,
}
impl Owners {
    fn new() -> Self {
        let owners = Self::default();
        owners.identity.set(17);
        owners
    }
    fn assert_alive(&self) {
        assert_eq!(self.identity.get(), 17);
        assert_eq!(self.drops.each_ref().map(Cell::get), [0; 3]);
    }
    fn assert_dropped(&self) {
        assert_eq!(self.identity.get(), 17);
        assert_eq!(self.drops.each_ref().map(Cell::get), [1; 3]);
    }
}

#[derive(Default)]
struct BodyControl {
    refuse_after_changed_recovery: bool,
    armed: bool,
    changed_recovery: Option<usize>,
    refusal: Option<BodyRefusal>,
    debits: Vec<(usize, usize, usize)>,
    completed_reads: Vec<(usize, usize)>,
    read_allowances: Vec<usize>,
    support: Option<AttemptSupport>,
}

struct BodyRefusal {
    provider: usize,
    remaining: usize,
    providers: [usize; 3],
    frame: Option<(DatabaseKeyIndex, bool)>,
    depths: (usize, usize),
    #[cfg(not(feature = "shuttle"))]
    sync: Option<transfer_trace::SyncSnapshot>,
    run_active: bool,
    completed_reads: Vec<(usize, usize)>,
}

struct ProviderGuard<'run> {
    owners: &'run Owners,
    index: usize,
    identity: *const Cell<u32>,
}
impl<'run> ProviderGuard<'run> {
    fn new(owners: &'run Owners, index: usize) -> Self {
        Self {
            owners,
            index,
            identity: std::ptr::from_ref(&owners.identity),
        }
    }
    fn check(&self) {
        assert_eq!(std::ptr::from_ref(&self.owners.identity), self.identity);
        self.owners.assert_alive();
    }

    fn before_body(&self, db: &dyn Db) -> RunResult<Option<usize>> {
        let mut control = self.owners.body_control.borrow_mut();
        let Some(control) = control.as_mut() else {
            return Ok(None);
        };
        self.check();
        let support = attempt_probe::current_query().unwrap();
        if let Some(original) = &control.support {
            assert!(original.same_owner(&support));
        } else {
            control.support = Some(support);
        }
        let remaining = attempt_probe::remaining_allowance_for_diagnostics(db).unwrap();
        if control.armed {
            control.armed = false;
            assert!(control.refusal.is_none());
            let frame = db
                .zalsa_local()
                .try_with_query_stack(|stack| {
                    stack
                        .last()
                        .map(|frame| (frame.database_key_index, frame.attempt_incomplete()))
                })
                .flatten();
            #[cfg(not(feature = "shuttle"))]
            let sync = frame.and_then(|(key, _)| {
                db.zalsa()
                    .lookup_ingredient(key.ingredient_index())
                    .as_function()
                    .unwrap()
                    .sync_table()
                    .test_transfer_state(key.key_index())
            });
            control.refusal = Some(BodyRefusal {
                provider: self.index,
                remaining,
                providers: self.owners.drops.each_ref().map(Cell::get),
                frame,
                depths: attempt_probe::stack_depths(),
                #[cfg(not(feature = "shuttle"))]
                sync,
                run_active: super::super::RUN_ACTIVE.with(Cell::get),
                completed_reads: control.completed_reads.clone(),
            });
            return Err(RunError::Refused(Incomplete::Allowance));
        }
        Ok(Some(remaining))
    }

    fn body_debited(&self, db: &dyn Db, before: Option<usize>) {
        if let Some(before) = before {
            let after = attempt_probe::remaining_allowance_for_diagnostics(db).unwrap();
            self.owners
                .body_control
                .borrow_mut()
                .as_mut()
                .unwrap()
                .debits
                .push((self.index, before, after));
        }
    }

    fn read_completed(&self, db: &dyn Db, child: usize) {
        if let Some(control) = self.owners.body_control.borrow_mut().as_mut() {
            control.completed_reads.push((self.index, child));
            control
                .read_allowances
                .push(attempt_probe::remaining_allowance_for_diagnostics(db).unwrap());
        }
    }

    fn recovered(&self, changed: bool) {
        if let Some(control) = self.owners.body_control.borrow_mut().as_mut()
            && changed
            && control.refuse_after_changed_recovery
        {
            control.refuse_after_changed_recovery = false;
            control.changed_recovery = Some(self.index);
            control.armed = true;
        }
    }
}
impl Drop for ProviderGuard<'_> {
    fn drop(&mut self) {
        self.owners.drops[self.index].set(self.owners.drops[self.index].get() + 1);
        self.owners.journal.borrow_mut().push("provider");
    }
}

struct AProvider<'run, 'db: 'run, B: Configuration> {
    next: CallableRoute<'run, 'db, B>,
    guard: ProviderGuard<'run>,
}
struct BProvider<'run, 'db: 'run, C: Configuration> {
    next: CallableRoute<'run, 'db, C>,
    guard: ProviderGuard<'run>,
}
struct CProvider<'run, 'db: 'run, A: Configuration> {
    next: CallableRoute<'run, 'db, A>,
    guard: ProviderGuard<'run>,
}

#[derive(Debug)]
struct RecoverySnapshot {
    providers: [usize; 3],
    identity: u32,
    storage_available: bool,
    frame: Option<(DatabaseKeyIndex, bool)>,
    depths: (usize, usize),
    claim_held: bool,
    panicking: bool,
    memo_final: Option<bool>,
}

struct RecoveryGuard<'run, 'db, C: Configuration> {
    owners: &'run Owners,
    db: &'db dyn Db,
    ingredient: &'db IngredientImpl<C>,
    input: Input,
}
impl<C: Configuration> Drop for RecoveryGuard<'_, '_, C> {
    fn drop(&mut self) {
        let snapshot = RecoverySnapshot {
            providers: self.owners.drops.each_ref().map(Cell::get),
            identity: self.owners.identity.get(),
            storage_available: self.owners.journal.try_borrow_mut().is_ok(),
            frame: self
                .db
                .zalsa_local()
                .try_with_query_stack(|stack| {
                    stack
                        .last()
                        .map(|frame| (frame.database_key_index, frame.attempt_incomplete()))
                })
                .flatten(),
            depths: attempt_probe::stack_depths(),
            claim_held: matches!(
                self.ingredient.sync_table.peek_claim(
                    self.db.zalsa(),
                    self.input.as_id(),
                    Reentrancy::Deny
                ),
                ClaimResult::Cycle { .. }
            ),
            panicking: std::thread::panicking(),
            memo_final: self
                .ingredient
                .memo(self.db.zalsa(), self.input.as_id())
                .map(|memo| !memo.header().may_be_provisional()),
        };
        *self.owners.recovery_drop.borrow_mut() = Some(snapshot);
        self.owners.journal.borrow_mut().push("recovery-child");
    }
}

// The armed recovery fault observes cleanup before delegating the real generated callback.
macro_rules! cycle_callbacks {
    () => {
        fn initial<'call>(
            &'call self,
            _endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Db,
            id: Id,
            input: Input,
        ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'call
        where
            'run: 'call,
        {
            self.guard.check();
            ready(Ok(C::cycle_initial(db, id, input)))
        }
        fn recover<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Db,
            cycle: &'call Cycle<'call>,
            last: &'call C::Output<'db>,
            value: C::Output<'db>,
            input: Input,
        ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'call
        where
            'run: 'call,
        {
            self.guard.check();
            if self.guard.index == 0
                && *last != value
                && self.guard.owners.recovery_fault.replace(false)
            {
                let owners = self.guard.owners;
                let guard = RecoveryGuard {
                    owners,
                    db,
                    ingredient: a::fn_ingredient_(db, db.zalsa()),
                    input,
                };
                let _demand = match endpoint.demand(move || async move {
                    let _guard = guard;
                    owners.child_polled.set(true);
                    Ok(())
                }) {
                    Ok(demand) => demand,
                    Err(error) => return ready(Err(error)),
                };
                panic_any(NativeFailure(owners.native.clone()));
            }
            let recovered = C::recover_from_cycle(db, cycle, last, value, input);
            self.guard.recovered(*last != recovered);
            ready(Ok(recovered))
        }
    };
}

impl<'run, 'db: 'run, C, B> CallableRouteProvider<'run, 'db, C> for AProvider<'run, 'db, B>
where
    C: for<'a> Configuration<DbView = dyn Db, Input<'a> = Input, Output<'a> = bool>,
    B: for<'a> Configuration<DbView = dyn Db, Input<'a> = Input, Output<'a> = Flag>,
{
    // Input conversion constructs an Input handle; output equality compares one bool.
    fixture_native_value!(callable, 'run, 'db, C, 1);

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        input: Input,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        endpoint
            .local_call(|| {
                let before = self.guard.before_body(db)?;
                endpoint.admit_work(1)?;
                self.guard.body_debited(db, before);
                self.guard.check();
                db.record(Recorded::Body(0));
                Ok(())
            })
            .await;
        let flag: &'db Flag = endpoint
            .child_call(|| async { endpoint.fetch_ref(&self.next, input.as_id())?.await })
            .await;
        self.guard.read_completed(db, 1);
        let enabled = endpoint.local_call(|| Ok(input.enabled(db))).await;
        self.guard.check();
        Ok(a_value(*flag, enabled))
    }
    cycle_callbacks!();
}
impl<'run, 'db: 'run, C, Next> CallableRouteProvider<'run, 'db, C> for BProvider<'run, 'db, Next>
where
    C: for<'a> Configuration<DbView = dyn Db, Input<'a> = Input, Output<'a> = Flag>,
    Next: for<'a> Configuration<DbView = dyn Db, Input<'a> = Input, Output<'a> = Option<bool>>,
{
    // Input conversion constructs an Input handle; Flag equality compares its bool.
    fixture_native_value!(callable, 'run, 'db, C, 1);

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        input: Input,
    ) -> RunResult<Flag>
    where
        'run: 'call,
    {
        endpoint
            .local_call(|| {
                let before = self.guard.before_body(db)?;
                endpoint.admit_work(1)?;
                self.guard.body_debited(db, before);
                self.guard.check();
                db.record(Recorded::Body(1));
                Ok(())
            })
            .await;
        let value: &'db Option<bool> = endpoint
            .child_call(|| async { endpoint.fetch_ref(&self.next, input.as_id())?.await })
            .await;
        self.guard.read_completed(db, 2);
        self.guard.check();
        Ok(b_value(*value))
    }
    cycle_callbacks!();
}
impl<'run, 'db: 'run, C, A> CallableRouteProvider<'run, 'db, C> for CProvider<'run, 'db, A>
where
    C: for<'a> Configuration<DbView = dyn Db, Input<'a> = Input, Output<'a> = Option<bool>>,
    A: for<'a> Configuration<DbView = dyn Db, Input<'a> = Input, Output<'a> = bool>,
{
    // Input conversion constructs an Input handle; Option<bool> equality is scalar.
    fixture_native_value!(callable, 'run, 'db, C, 2);

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        input: Input,
    ) -> RunResult<Option<bool>>
    where
        'run: 'call,
    {
        endpoint
            .local_call(|| {
                let before = self.guard.before_body(db)?;
                endpoint.admit_work(1)?;
                self.guard.body_debited(db, before);
                self.guard.check();
                db.record(Recorded::Body(2));
                Ok(())
            })
            .await;
        let value: &'db bool = endpoint
            .child_call(|| async { endpoint.fetch_ref(&self.next, input.as_id())?.await })
            .await;
        self.guard.read_completed(db, 0);
        self.guard.check();
        Ok(c_value(*value))
    }
    cycle_callbacks!();
}

struct Routes<'run, 'db: 'run, A: Configuration, B: Configuration, C: Configuration> {
    a: CallableRoute<'run, 'db, A>,
    b: CallableRoute<'run, 'db, B>,
    c: CallableRoute<'run, 'db, C>,
}

fn register<'run, 'db: 'run, A, B, C>(
    db: &'db dyn Db,
    admission: &'run Admission,
    owners: &'run Owners,
    ingredients: (
        &'db IngredientImpl<A>,
        &'db IngredientImpl<B>,
        &'db IngredientImpl<C>,
    ),
) -> RunResult<(RegisteredRun<'run, 'db>, Routes<'run, 'db, A, B, C>)>
where
    A: for<'a> Configuration<DbView = dyn Db, Input<'a> = Input, Output<'a> = bool>,
    B: for<'a> Configuration<DbView = dyn Db, Input<'a> = Input, Output<'a> = Flag>,
    C: for<'a> Configuration<DbView = dyn Db, Input<'a> = Input, Output<'a> = Option<bool>>,
{
    let mut registry = RegistryBuilder::new(db, admission)?;
    let routes = bind_cycle(&mut registry, db, owners, ingredients)?;
    Ok((registry.seal()?, routes))
}

fn bind_cycle<'run, 'db: 'run, A, B, C>(
    registry: &mut RegistryBuilder<'run, 'db>,
    db: &'db dyn Db,
    owners: &'run Owners,
    ingredients: (
        &'db IngredientImpl<A>,
        &'db IngredientImpl<B>,
        &'db IngredientImpl<C>,
    ),
) -> RunResult<Routes<'run, 'db, A, B, C>>
where
    A: for<'a> Configuration<DbView = dyn Db, Input<'a> = Input, Output<'a> = bool>,
    B: for<'a> Configuration<DbView = dyn Db, Input<'a> = Input, Output<'a> = Flag>,
    C: for<'a> Configuration<DbView = dyn Db, Input<'a> = Input, Output<'a> = Option<bool>>,
{
    let a = registry.reserve_callable(db, ingredients.0)?;
    let b = registry.reserve_callable(db, ingredients.1)?;
    let c = registry.reserve_callable(db, ingredients.2)?;
    registry.bind_callable(
        &a,
        AProvider {
            next: b.clone(),
            guard: ProviderGuard::new(owners, 0),
        },
    )?;
    registry.bind_callable(
        &b,
        BProvider {
            next: c.clone(),
            guard: ProviderGuard::new(owners, 1),
        },
    )?;
    registry.bind_callable(
        &c,
        CProvider {
            next: a.clone(),
            guard: ProviderGuard::new(owners, 2),
        },
    )?;
    Ok(Routes { a, b, c })
}

fn keys(db: &TestDb, input: Input) -> [DatabaseKeyIndex; 3] {
    [
        a::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
        b::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
        c::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
    ]
}

fn query_edges(reads: &[Read], keys: &[DatabaseKeyIndex]) -> Vec<(usize, usize)> {
    for read in reads {
        assert!(keys.contains(&read.key), "unexpected query read: {read:?}");
        assert!(read.parent.is_none_or(|parent| keys.contains(&parent)));
    }
    let mut edges: Vec<_> = reads
        .iter()
        .filter_map(|read| {
            let parent = keys.iter().position(|key| Some(*key) == read.parent)?;
            let child = keys.iter().position(|key| *key == read.key)?;
            Some((parent, child))
        })
        .collect();
    edges.sort_unstable();
    edges.dedup();
    edges
}

fn ordered_reads(
    reads: &[Read],
    keys: &[DatabaseKeyIndex; 3],
) -> Vec<(Option<usize>, usize, Status)> {
    reads
        .iter()
        .map(|read| {
            let index = |key| keys.iter().position(|known| *known == key).unwrap();
            (read.parent.map(index), index(read.key), read.status)
        })
        .collect()
}

#[derive(Debug, Eq, PartialEq)]
enum NormalizedRecorded {
    Body(usize),
    Initial(usize),
    Recovery(usize, bool),
    Iteration(usize),
    Finalized(usize),
}

fn ordered_events(events: &[Recorded], keys: &[DatabaseKeyIndex; 3]) -> Vec<NormalizedRecorded> {
    events
        .iter()
        .map(|event| match *event {
            Recorded::Body(index) => NormalizedRecorded::Body(index),
            Recorded::Initial(index) => NormalizedRecorded::Initial(index),
            Recorded::Recovery(index, changed) => NormalizedRecorded::Recovery(index, changed),
            Recorded::Iteration(key) => {
                NormalizedRecorded::Iteration(keys.iter().position(|known| *known == key).unwrap())
            }
            Recorded::Finalized(key) => {
                NormalizedRecorded::Finalized(keys.iter().position(|known| *known == key).unwrap())
            }
        })
        .collect()
}

fn report_work(phase: &str, work: &[ExecutionWork]) {
    for (index, work) in work.iter().enumerate() {
        println!("CALLABLE_WORK\t{phase}\t{index}\t{work:?}");
    }
}

#[derive(Debug, Eq, PartialEq)]
enum Dependency {
    Query(usize),
    Input(&'static str),
}

fn memo_inputs<C: Configuration>(
    db: &TestDb,
    ingredient: &IngredientImpl<C>,
    input: Input,
    keys: &[DatabaseKeyIndex],
) -> Vec<Dependency> {
    let header = ingredient.memo(db.zalsa(), input.as_id()).unwrap().header();
    assert!(!header.may_be_provisional());
    assert!(header.origin().outputs().next().is_none());
    header
        .origin()
        .inputs()
        .map(|key| {
            if let Some(index) = keys.iter().position(|known| *known == key) {
                Dependency::Query(index)
            } else {
                let ingredient = db.zalsa().lookup_ingredient(key.ingredient_index());
                assert!(ingredient.as_function().is_none());
                assert_eq!(key.key_index(), input.as_id());
                Dependency::Input(ingredient.debug_name())
            }
        })
        .collect()
}

fn assert_idle(db: &TestDb) {
    assert!(db.zalsa_local().active_query().is_none());
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(!super::super::RUN_ACTIVE.with(Cell::get));
}

fn assert_cycle(recorded: &[Recorded], keys: &[DatabaseKeyIndex; 3]) {
    assert!(
        recorded
            .iter()
            .any(|event| matches!(event, Recorded::Initial(_)))
    );
    assert!(
        recorded
            .iter()
            .any(|event| matches!(event, Recorded::Recovery(_, true)))
    );
    assert!(
        recorded
            .iter()
            .any(|event| matches!(event, Recorded::Iteration(key) if keys.contains(key)))
    );
    assert!(
        recorded
            .iter()
            .any(|event| matches!(event, Recorded::Finalized(key) if keys.contains(key)))
    );
}

#[test]
fn heterogeneous_callable_cycle_preserves_typed_memos_and_provider_lifetimes() {
    let ordinary = TestDb::default();
    let ordinary_input = Input::new(&ordinary, false);
    let ordinary_reads = prepared_source_probe::capture(&ordinary, || {
        let scalar: &bool = a(&ordinary, ordinary_input);
        let flag: &Flag = b(&ordinary, ordinary_input);
        let optional: &Option<bool> = c(&ordinary, ordinary_input);
        assert_eq!(
            (*scalar, *flag, *optional),
            (false, Flag { value: false }, Some(false))
        );
    })
    .unwrap();
    ordinary_reads.check_root_reads().unwrap();
    let ordinary_keys = keys(&ordinary, ordinary_input);
    assert_cycle(&ordinary.recorded.lock().unwrap(), &ordinary_keys);
    assert_eq!(
        query_edges(&ordinary_reads.reads, &ordinary_keys),
        [(0, 1), (1, 2), (2, 0)]
    );

    let db = TestDb::default();
    let input = Input::new(&db, false);
    let observed_keys = keys(&db, input);
    let admission = Admission::default();
    for hot in [false, true] {
        let owners = Owners::default();
        owners.identity.set(17);
        let before = db.recorded.lock().unwrap().len();
        let registration_start = admission.work.borrow().len();
        let execution_start = Cell::new(registration_start);
        let captured = prepared_source_probe::capture(&db, || {
            try_with_attempt(&db, 100_000, || {
                let (run, routes) = register(
                    &db,
                    &admission,
                    &owners,
                    (
                        a::fn_ingredient_(&db, db.zalsa()),
                        b::fn_ingredient_(&db, db.zalsa()),
                        c::fn_ingredient_(&db, db.zalsa()),
                    ),
                )?;
                execution_start.set(admission.work.borrow().len());
                let retained = (routes.a.clone(), routes.b.clone(), routes.c.clone());
                let value = run.run(move |endpoint| async move {
                    let scalar: &bool = endpoint
                        .child_call(|| async {
                            endpoint.fetch_ref(&routes.a, input.as_id())?.await
                        })
                        .await;
                    let flag: &Flag = endpoint
                        .child_call(|| async {
                            endpoint.fetch_ref(&routes.b, input.as_id())?.await
                        })
                        .await;
                    let optional: &Option<bool> = endpoint
                        .child_call(|| async {
                            endpoint.fetch_ref(&routes.c, input.as_id())?.await
                        })
                        .await;
                    assert_eq!(
                        (*scalar, *flag, *optional),
                        (false, Flag { value: false }, Some(false))
                    );
                    Ok(flag)
                })?;
                owners.assert_dropped();
                assert_eq!(*value, Flag { value: false });
                assert_eq!(
                    [
                        retained.0.database_key(input.as_id()),
                        retained.1.database_key(input.as_id()),
                        retained.2.database_key(input.as_id())
                    ],
                    observed_keys
                );
                drop(retained);
                Ok::<_, RunError>(value)
            })
        })
        .unwrap();
        assert_eq!(
            captured.value,
            Ok(AttemptOutcome::Complete(Ok(&Flag { value: false })))
        );
        captured.check_root_reads().unwrap();
        owners.assert_dropped();
        assert_idle(&db);
        let work = admission.work.borrow();
        report_work(
            if hot {
                "hot-registration"
            } else {
                "cold-registration"
            },
            &work[registration_start..execution_start.get()],
        );
        report_work(
            if hot {
                "hot-execution"
            } else {
                "cold-execution"
            },
            &work[execution_start.get()..],
        );
        if hot {
            assert_eq!(db.recorded.lock().unwrap().len(), before);
            assert!(query_edges(&captured.reads, &observed_keys).is_empty());
        } else {
            assert_cycle(&db.recorded.lock().unwrap(), &observed_keys);
            assert_eq!(
                query_edges(&captured.reads, &observed_keys),
                query_edges(&ordinary_reads.reads, &ordinary_keys)
            );
            let actual_reads = ordered_reads(&captured.reads, &observed_keys);
            let expected_reads = ordered_reads(&ordinary_reads.reads, &ordinary_keys);
            let actual_events = ordered_events(&db.recorded.lock().unwrap(), &observed_keys);
            let expected_events =
                ordered_events(&ordinary.recorded.lock().unwrap(), &ordinary_keys);
            println!("CALLABLE_TRACE\tcontrolled-reads\t{actual_reads:?}");
            println!("CALLABLE_TRACE\tordinary-reads\t{expected_reads:?}");
            println!("CALLABLE_TRACE\tcontrolled-events\t{actual_events:?}");
            println!("CALLABLE_TRACE\tordinary-events\t{expected_events:?}");
            assert_eq!(actual_reads, expected_reads);
            assert_eq!(actual_events, expected_events);
        }
    }
    assert_eq!(
        memo_inputs(
            &db,
            a::fn_ingredient_(&db, db.zalsa()),
            input,
            &observed_keys
        ),
        memo_inputs(
            &ordinary,
            a::fn_ingredient_(&ordinary, ordinary.zalsa()),
            ordinary_input,
            &ordinary_keys
        )
    );
    assert_eq!(
        memo_inputs(
            &db,
            b::fn_ingredient_(&db, db.zalsa()),
            input,
            &observed_keys
        ),
        memo_inputs(
            &ordinary,
            b::fn_ingredient_(&ordinary, ordinary.zalsa()),
            ordinary_input,
            &ordinary_keys
        )
    );
    assert_eq!(
        memo_inputs(
            &db,
            c::fn_ingredient_(&db, db.zalsa()),
            input,
            &observed_keys
        ),
        memo_inputs(
            &ordinary,
            c::fn_ingredient_(&ordinary, ordinary.zalsa()),
            ordinary_input,
            &ordinary_keys
        )
    );
    assert!(
        admission
            .work
            .borrow()
            .iter()
            .any(|work| matches!(work, ExecutionWork::Work { units: 1 }))
    );
}

#[test]
fn sealed_callable_cycle_releases_providers_without_execution() {
    let db = TestDb::default();
    let admission = Admission::default();
    let owners = Owners::default();
    owners.identity.set(17);
    let outcome = try_with_attempt(&db, 100_000, || {
        let (run, routes) = register(
            &db,
            &admission,
            &owners,
            (
                a::fn_ingredient_(&db, db.zalsa()),
                b::fn_ingredient_(&db, db.zalsa()),
                c::fn_ingredient_(&db, db.zalsa()),
            ),
        )?;
        owners.assert_alive();
        drop(run);
        owners.assert_dropped();
        drop(routes);
        Ok::<_, RunError>(())
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
    assert!(db.recorded.lock().unwrap().is_empty());
    assert_idle(&db);
}

#[derive(Clone, Copy, Debug)]
enum Exit {
    Success,
    Refusal,
    Native,
}

struct TaskGuard<'run> {
    owners: &'run Owners,
    name: &'static str,
}
impl Drop for TaskGuard<'_> {
    fn drop(&mut self) {
        self.owners.journal.borrow_mut().push(self.name);
        if self.owners.drops.each_ref().map(Cell::get) != [0; 3] {
            self.owners.premature_provider_drop.set(true);
        }
    }
}

#[derive(Debug)]
struct NativeFailure(Arc<()>);

#[test]
fn registered_run_retains_providers_after_root_endpoint_drop() {
    for exit in [Exit::Success, Exit::Refusal, Exit::Native] {
        let db = TestDb::default();
        let admission = Admission::default();
        let owners = Owners::default();
        owners.identity.set(17);
        let polled = Cell::new(false);
        let native = Arc::new(());
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(&db, 100_000, || {
                let (run, routes) = register(
                    &db,
                    &admission,
                    &owners,
                    (
                        a::fn_ingredient_(&db, db.zalsa()),
                        b::fn_ingredient_(&db, db.zalsa()),
                        c::fn_ingredient_(&db, db.zalsa()),
                    ),
                )?;
                let owner_ref = &owners;
                let polled_ref = &polled;
                let native = native.clone();
                let result = run.run(move |endpoint| async move {
                    let _root = TaskGuard {
                        owners: owner_ref,
                        name: "root",
                    };
                    let inner = endpoint.inner.clone();
                    Ok(callback::child_call(&inner, move || async move {
                        let child = TaskGuard {
                            owners: owner_ref,
                            name: "child",
                        };
                        let demand = endpoint.demand(move || async move {
                            let _child = child;
                            owner_ref.assert_alive();
                            polled_ref.set(true);
                            Ok(())
                        })?;
                        drop(endpoint);
                        match exit {
                            Exit::Success => demand.await,
                            Exit::Refusal => Err(RunError::Refused(Incomplete::Allowance)),
                            Exit::Native => panic_any(NativeFailure(native)),
                        }
                    })
                    .await)
                });
                if matches!(exit, Exit::Refusal) {
                    assert_eq!(result, Err(RunError::Refused(Incomplete::Allowance)));
                }
                owners.assert_dropped();
                drop(routes);
                result
            })
        }));
        match exit {
            Exit::Success => assert_eq!(outcome.unwrap(), Ok(AttemptOutcome::Complete(Ok(())))),
            Exit::Refusal => assert_eq!(
                outcome.unwrap(),
                Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
            ),
            Exit::Native => {
                let payload = outcome.unwrap_err();
                let marker = payload.downcast_ref::<NativeFailure>().unwrap();
                assert!(Arc::ptr_eq(&marker.0, &native));
            }
        }
        owners.assert_dropped();
        assert!(!owners.premature_provider_drop.get());
        assert_eq!(polled.get(), matches!(exit, Exit::Success));
        assert_eq!(
            &*owners.journal.borrow(),
            &["child", "root", "provider", "provider", "provider"]
        );
        assert!(db.recorded.lock().unwrap().is_empty());
        assert_idle(&db);
    }
}

#[crate::tracked(returns(copy), attempt = CompleteOnly)]
fn complete_only(db: &dyn Db, input: Input) -> bool {
    db.record(Recorded::Body(3));
    input.enabled(db)
}

macro_rules! assert_contract {
    ($result:expr, $message:literal) => {
        assert!(matches!($result, Err(RunError::Contract($message))));
    };
}

#[test]
fn callable_registration_rejects_foreign_duplicate_and_unbound_routes() {
    let db = TestDb::default();
    let foreign_db = TestDb::default();
    let input = Input::new(&db, false);
    let admission = Admission::default();
    let owners = Owners::new();
    let rejected = Owners::new();
    let duplicate = Owners::new();
    let unbound = Owners::new();
    let outcome = try_with_attempt(&db, 100_000, || {
        let ai = a::fn_ingredient_(&db, db.zalsa());
        let bi = b::fn_ingredient_(&db, db.zalsa());
        let ci = c::fn_ingredient_(&db, db.zalsa());
        for mode in 0..3 {
            let mut registry = RegistryBuilder::new(&db, &admission)?;
            if mode == 1 {
                let _route = registry.reserve(&db as &dyn Db, ai)?;
            } else {
                let _route = registry.reserve_callable(&db as &dyn Db, ai)?;
            }
            if mode == 2 {
                assert_contract!(
                    registry.reserve(&db as &dyn Db, ai),
                    "query route already reserved"
                );
            } else {
                assert_contract!(
                    registry.reserve_callable(&db as &dyn Db, ai),
                    "query route already reserved"
                );
            }
        }
        let mut foreign = RegistryBuilder::new(&db, &admission)?;
        let foreign_a = foreign.reserve_callable(&db as &dyn Db, ai)?;
        let foreign_b = foreign.reserve_callable(&db as &dyn Db, bi)?;
        let mut registry = RegistryBuilder::new(&db, &admission)?;
        assert_contract!(
            registry.reserve(
                &db as &dyn Db,
                complete_only::fn_ingredient_(&db, db.zalsa())
            ),
            "only return-only routes can be registered"
        );
        assert_contract!(
            registry.reserve_callable(
                &foreign_db as &dyn Db,
                a::fn_ingredient_(&foreign_db, foreign_db.zalsa())
            ),
            "route has a foreign database or ingredient"
        );
        assert_contract!(
            registry.reserve_callable(
                &db as &dyn Db,
                a::fn_ingredient_(&foreign_db, foreign_db.zalsa())
            ),
            "route has a foreign database or ingredient"
        );
        assert_contract!(
            registry.bind_callable(
                &foreign_a,
                AProvider {
                    next: foreign_b.clone(),
                    guard: ProviderGuard::new(&rejected, 0),
                }
            ),
            "foreign registration token"
        );
        assert_eq!(rejected.drops.each_ref().map(Cell::get), [1, 0, 0]);
        let ar = registry.reserve_callable(&db as &dyn Db, ai)?;
        let br = registry.reserve_callable(&db as &dyn Db, bi)?;
        let cr = registry.reserve_callable(&db as &dyn Db, ci)?;
        registry.bind_callable(
            &ar,
            AProvider {
                next: br.clone(),
                guard: ProviderGuard::new(&owners, 0),
            },
        )?;
        registry.bind_callable(
            &br,
            BProvider {
                next: cr.clone(),
                guard: ProviderGuard::new(&owners, 1),
            },
        )?;
        registry.bind_callable(
            &cr,
            CProvider {
                next: ar.clone(),
                guard: ProviderGuard::new(&owners, 2),
            },
        )?;
        assert_contract!(
            registry.bind_callable(
                &ar,
                AProvider {
                    next: br.clone(),
                    guard: ProviderGuard::new(&duplicate, 0),
                }
            ),
            "query route already bound or missing"
        );
        assert_eq!(duplicate.drops.each_ref().map(Cell::get), [1, 0, 0]);
        owners.assert_alive();
        assert!(db.recorded.lock().unwrap().is_empty());
        let revision = db.zalsa().current_revision();
        let run = registry.seal()?;
        let admission_ref = &admission;
        run.run(move |endpoint| async move {
            endpoint
                .local_call(|| {
                    let before = admission_ref.work.borrow().len();
                    assert_contract!(
                        endpoint.fetch_ref(&foreign_a, input.as_id()),
                        "foreign query route"
                    );
                    assert_contract!(
                        endpoint.validate_callable(&foreign_a, input.as_id(), revision),
                        "foreign query route"
                    );
                    assert_eq!(admission_ref.work.borrow().len(), before);
                    Ok(())
                })
                .await;
            let value = endpoint
                .child_call(|| async { endpoint.fetch_ref(&ar, input.as_id())?.await })
                .await;
            assert!(!*value);
            Ok(())
        })?;
        owners.assert_dropped();
        drop(foreign);

        let mut registry = RegistryBuilder::new(&db, &admission)?;
        let ar = registry.reserve_callable(&db as &dyn Db, ai)?;
        let br = registry.reserve_callable(&db as &dyn Db, bi)?;
        registry.bind_callable(
            &ar,
            AProvider {
                next: br.clone(),
                guard: ProviderGuard::new(&unbound, 0),
            },
        )?;
        let before = db.recorded.lock().unwrap().len();
        assert_contract!(registry.seal(), "query route remains unbound");
        assert_eq!(unbound.drops.each_ref().map(Cell::get), [1, 0, 0]);
        assert_eq!(db.recorded.lock().unwrap().len(), before);
        drop((ar, br));
        Ok::<_, RunError>(())
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
    assert_idle(&db);
}

type Values = (bool, Flag, Option<bool>);

fn run_cycle(
    db: &TestDb,
    input: Input,
    owners: &Owners,
    allowance: usize,
    raw_error: &Cell<Option<RunError>>,
    validate_after: Option<Revision>,
) -> Result<AttemptOutcome<RunResult<Values>>, attempt_probe::StartError> {
    let admission = Admission::default();
    try_with_attempt(db, allowance, || {
        let (run, routes) = register(
            db,
            &admission,
            owners,
            (
                a::fn_ingredient_(db, db.zalsa()),
                b::fn_ingredient_(db, db.zalsa()),
                c::fn_ingredient_(db, db.zalsa()),
            ),
        )?;
        let result = run.run(move |endpoint| async move {
            if let Some(revision) = validate_after {
                let validation = endpoint
                    .child_call(|| async {
                        endpoint
                            .validate_callable(&routes.a, input.as_id(), revision)?
                            .await
                    })
                    .await;
                assert!(matches!(validation, VerifyResult::Changed));
            }
            let a = endpoint
                .child_call(|| async { endpoint.fetch_ref(&routes.a, input.as_id())?.await })
                .await;
            let b = endpoint
                .child_call(|| async { endpoint.fetch_ref(&routes.b, input.as_id())?.await })
                .await;
            let c = endpoint
                .child_call(|| async { endpoint.fetch_ref(&routes.c, input.as_id())?.await })
                .await;
            Ok((*a, *b, *c))
        });
        raw_error.set(result.as_ref().err().copied());
        result
    })
}

fn assert_storage<C: Configuration>(
    db: &TestDb,
    ingredient: &IngredientImpl<C>,
    input: Input,
    failed: bool,
) {
    assert!(matches!(
        ingredient
            .sync_table
            .peek_claim(db.zalsa(), input.as_id(), Reentrancy::Deny),
        ClaimResult::Claimed(())
    ));
    if let Some(memo) = ingredient.memo(db.zalsa(), input.as_id()) {
        let header = memo.header();
        if failed {
            assert!(header.may_be_provisional());
            assert!(!header.can_seed_attempt(db.zalsa()));
        } else {
            assert!(!header.may_be_provisional());
            assert_eq!(header.verified_at.load(), db.zalsa().current_revision());
        }
    } else {
        assert!(failed);
    }
}

fn assert_cycle_storage(db: &TestDb, input: Input, failed: bool) {
    assert_storage(db, a::fn_ingredient_(db, db.zalsa()), input, failed);
    assert_storage(db, b::fn_ingredient_(db, db.zalsa()), input, failed);
    assert_storage(db, c::fn_ingredient_(db, db.zalsa()), input, failed);
    assert_idle(db);
}

fn assert_dependencies_match(db: &TestDb, input: Input, ordinary: &TestDb, ordinary_input: Input) {
    let actual = keys(db, input);
    let expected = keys(ordinary, ordinary_input);
    assert_eq!(
        memo_inputs(db, a::fn_ingredient_(db, db.zalsa()), input, &actual),
        memo_inputs(
            ordinary,
            a::fn_ingredient_(ordinary, ordinary.zalsa()),
            ordinary_input,
            &expected
        )
    );
    assert_eq!(
        memo_inputs(db, b::fn_ingredient_(db, db.zalsa()), input, &actual),
        memo_inputs(
            ordinary,
            b::fn_ingredient_(ordinary, ordinary.zalsa()),
            ordinary_input,
            &expected
        )
    );
    assert_eq!(
        memo_inputs(db, c::fn_ingredient_(db, db.zalsa()), input, &actual),
        memo_inputs(
            ordinary,
            c::fn_ingredient_(ordinary, ordinary.zalsa()),
            ordinary_input,
            &expected
        )
    );
}

fn assert_same_revision_retries(db: &TestDb, input: Input, enabled: bool) {
    let stamp = Stamp::current(db);
    let ordinary = TestDb::default();
    let ordinary_input = Input::new(&ordinary, enabled);
    let expected = (enabled, Flag { value: enabled }, Some(enabled));
    assert_eq!(
        (
            *a(&ordinary, ordinary_input),
            *b(&ordinary, ordinary_input),
            *c(&ordinary, ordinary_input)
        ),
        expected
    );
    for hot in [false, true] {
        let owners = Owners::new();
        let error = Cell::new(None);
        let before = db.recorded.lock().unwrap().len();
        assert_eq!(
            run_cycle(db, input, &owners, 100_000, &error, None),
            Ok(AttemptOutcome::Complete(Ok(expected)))
        );
        assert_eq!(error.get(), None);
        owners.assert_dropped();
        assert_cycle_storage(db, input, false);
        assert_dependencies_match(db, input, &ordinary, ordinary_input);
        assert_eq!(Stamp::current(db), stamp);
        if hot {
            assert_eq!(db.recorded.lock().unwrap().len(), before);
        }
    }
}

#[test]
fn callable_binding_admission_refusal_keeps_the_registered_prefix_owned() {
    let db = TestDb::default();
    let input = Input::new(&db, false);
    let stamp = Stamp::current(&db);
    let admission = Admission::default();
    let owners = Owners::new();
    let retried = Owners::new();
    let outcome = try_with_attempt(&db, 100_000, || {
        let mut registry = RegistryBuilder::new(&db, &admission)?;
        let ar = registry.reserve_callable(&db as &dyn Db, a::fn_ingredient_(&db, db.zalsa()))?;
        let br = registry.reserve_callable(&db as &dyn Db, b::fn_ingredient_(&db, db.zalsa()))?;
        let cr = registry.reserve_callable(&db as &dyn Db, c::fn_ingredient_(&db, db.zalsa()))?;
        registry.bind_callable(
            &ar,
            AProvider {
                next: br.clone(),
                guard: ProviderGuard::new(&owners, 0),
            },
        )?;
        admission.refuse_next_resource.set(true);
        assert_eq!(
            registry.bind_callable(
                &br,
                BProvider {
                    next: cr.clone(),
                    guard: ProviderGuard::new(&owners, 1)
                }
            ),
            Err(RunError::Refused(Incomplete::Allowance))
        );
        assert_eq!(owners.drops.each_ref().map(Cell::get), [0, 1, 0]);
        let before = admission.work.borrow().len();
        assert_eq!(
            registry.bind_callable(
                &br,
                BProvider {
                    next: cr.clone(),
                    guard: ProviderGuard::new(&retried, 1)
                }
            ),
            Err(RunError::Refused(Incomplete::Allowance))
        );
        assert_eq!(retried.drops.each_ref().map(Cell::get), [0, 1, 0]);
        assert_eq!(admission.work.borrow().len(), before);
        assert!(matches!(
            registry.seal(),
            Err(RunError::Refused(Incomplete::Allowance))
        ));
        assert_eq!(admission.work.borrow().len(), before);
        assert_eq!(owners.drops.each_ref().map(Cell::get), [1, 1, 0]);
        assert!(db.recorded.lock().unwrap().is_empty());
        Ok::<_, RunError>(())
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(Stamp::current(&db), stamp);
    assert_cycle_storage(&db, input, true);
    assert_same_revision_retries(&db, input, false);
}

#[cfg(not(feature = "shuttle"))]
fn assert_refused_claims(
    trace: &transfer_trace::TransferTrace,
    keys: &[DatabaseKeyIndex; 3],
    participants_transferred: bool,
) {
    assert!(!trace.broken);
    for (index, key) in keys.iter().enumerate() {
        let claims: Vec<_> = trace
            .records
            .iter()
            .filter(|record| record.event.kind == Kind::Claim && record.event.key == Some(*key))
            .collect();
        assert_eq!(claims.len(), 1);
        let terminals: Vec<_> = trace
            .records
            .iter()
            .filter(|record| {
                record.event.kind == Kind::Terminal && record.event.serial == claims[0].event.serial
            })
            .collect();
        assert_eq!(terminals.len(), 1);
        let (action, wait) = if participants_transferred && index != 0 {
            (Action::Drop, None)
        } else {
            (Action::Abort, Some(WaitResult::Cancelled))
        };
        assert_eq!(terminals[0].event.action, Some(action));
        assert!(matches!(
            (terminals[0].event.wait, wait),
            (None, None) | (Some(WaitResult::Cancelled), Some(WaitResult::Cancelled))
        ));
    }
    assert!(
        trace
            .records
            .iter()
            .all(|record| record.event.kind != Kind::Poisoned)
    );
}

fn assert_refused_support(db: &TestDb, input: Input, owners: &Owners) {
    fn assert_memo_support<C: Configuration>(
        db: &TestDb,
        ingredient: &IngredientImpl<C>,
        input: Input,
        support: &AttemptSupport,
    ) {
        if let Some(memo) = ingredient.memo(db.zalsa(), input.as_id()) {
            assert!(memo.has_value());
            let memo_support = memo.header().revisions.attempt_support().unwrap();
            assert!(memo_support.same_owner(support));
            assert!(memo_support.incomplete(true));
            assert_eq!(memo_support.reuse(db.zalsa(), true), MemoReuse::Stale);
        }
    }

    owners.assert_dropped();
    assert_eq!(
        &*owners.journal.borrow(),
        &["provider", "provider", "provider"]
    );
    assert!(!owners.premature_provider_drop.get());
    assert_cycle_storage(db, input, true);
    assert!(attempt_probe::current().is_none());
    assert!(!db.zalsa_local().should_trigger_local_cancellation());
    let support = owners
        .body_control
        .borrow()
        .as_ref()
        .unwrap()
        .support
        .clone()
        .unwrap();
    assert_eq!(support.reason(), Some(Incomplete::Allowance));
    assert!(!support.owns_current_session(db.zalsa()));
    assert_eq!(support.reuse(db.zalsa(), true), MemoReuse::Stale);
    assert_memo_support(db, a::fn_ingredient_(db, db.zalsa()), input, &support);
    assert_memo_support(db, b::fn_ingredient_(db, db.zalsa()), input, &support);
    assert_memo_support(db, c::fn_ingredient_(db, db.zalsa()), input, &support);
    #[cfg(not(feature = "shuttle"))]
    {
        let graph = db.zalsa().runtime().test_transfer_graph_snapshot();
        assert!(graph.edges.is_empty() && graph.dependents.is_empty() && graph.pending.is_empty());
        assert!(graph.transferred.is_empty() && graph.reverse.is_empty());
    }
    let independent = try_with_attempt(db, 1, || {
        let current = attempt_probe::current().unwrap();
        assert!(!current.same_owner(&support));
        assert_eq!(current.reason(), None);
        assert_eq!(support.reuse(db.zalsa(), true), MemoReuse::Stale);
        attempt_probe::charge(db, 1)
    });
    assert_eq!(independent, Ok(AttemptOutcome::Complete(Ok(()))));
    assert_idle(db);
}

#[test]
fn callable_cycle_shared_allowance_refuses_before_changed_recovery() {
    let allowance = {
        let db = TestDb::default();
        let input = Input::new(&db, false);
        let owners = Owners::new();
        *owners.body_control.borrow_mut() = Some(BodyControl::default());
        let error = Cell::new(None);
        assert_eq!(
            run_cycle(&db, input, &owners, 100_000, &error, None),
            Ok(AttemptOutcome::Complete(Ok((
                false,
                Flag { value: false },
                Some(false)
            ))))
        );
        let control = owners.body_control.borrow();
        let control = control.as_ref().unwrap();
        assert_eq!(control.completed_reads[0], (2, 0));
        100_000 - control.read_allowances[0]
    };
    let db = TestDb::default();
    let input = Input::new(&db, false);
    let stamp = Stamp::current(&db);
    let owners = Owners::new();
    *owners.body_control.borrow_mut() = Some(BodyControl::default());
    let error = Cell::new(None);
    let collect = || {
        observation::collect(|| {
            prepared_source_probe::capture(&db, || {
                // Fund the first completed provisional read, including dependency recording,
                // so the following recovery work exhausts the shared allowance.
                run_cycle(&db, input, &owners, allowance, &error, None)
            })
                .unwrap()
        })
    };
    #[cfg(not(feature = "shuttle"))]
    let ((captured, observed), trace) = transfer_trace::collect(
        TraceConfig {
            ordinal: Arc::new(AtomicUsize::new(0)),
            worker: 0,
        },
        collect,
    );
    #[cfg(feature = "shuttle")]
    let (captured, observed) = collect();
    assert_eq!(
        captured.value,
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(error.get(), Some(RunError::Refused(Incomplete::Allowance)));
    assert_eq!(
        &*db.recorded.lock().unwrap(),
        &[
            Recorded::Body(0),
            Recorded::Body(1),
            Recorded::Body(2),
            Recorded::Initial(0)
        ]
    );
    assert_eq!(query_edges(&captured.reads, &keys(&db, input)), [(2, 0)]);
    assert_eq!(captured.reads.len(), 1);
    assert_eq!(captured.reads[0].status, Status::Provisional);
    let control = owners.body_control.borrow();
    let control = control.as_ref().unwrap();
    assert_eq!(
        control.debits,
        [
            (0, allowance - 3, allowance - 4),
            (1, allowance - 7, allowance - 8),
            (2, allowance - 12, allowance - 13),
        ]
    );
    assert_eq!(control.completed_reads, [(2, 0)]);
    assert_eq!(control.read_allowances, [0]);
    assert!(!control.armed && control.refusal.is_none());
    assert_eq!(control.changed_recovery, None);
    assert!(observed.polls > 0);
    assert_eq!(observed.max_active_polls, 1);
    #[cfg(not(feature = "shuttle"))]
    assert_refused_claims(&trace, &keys(&db, input), false);
    assert_refused_support(&db, input, &owners);
    assert_eq!(Stamp::current(&db), stamp);
    assert_same_revision_retries(&db, input, false);
}

#[test]
fn callable_cycle_body_refusal_after_changed_recovery_retries_at_the_same_revision() {
    let db = TestDb::default();
    let input = Input::new(&db, false);
    let stamp = Stamp::current(&db);
    let owners = Owners::new();
    *owners.body_control.borrow_mut() = Some(BodyControl {
        refuse_after_changed_recovery: true,
        ..BodyControl::default()
    });
    let error = Cell::new(None);
    // This reserve funds the lifecycle boundary; the next body refuses explicitly while credit remains.
    let collect = || {
        observation::collect(|| {
            prepared_source_probe::capture(&db, || {
                run_cycle(&db, input, &owners, 100_000, &error, None)
            })
            .unwrap()
        })
    };
    #[cfg(not(feature = "shuttle"))]
    let ((captured, observed), trace) = transfer_trace::collect(
        TraceConfig {
            ordinal: Arc::new(AtomicUsize::new(0)),
            worker: 0,
        },
        collect,
    );
    #[cfg(feature = "shuttle")]
    let (captured, observed) = collect();
    assert_eq!(
        captured.value,
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(error.get(), Some(RunError::Refused(Incomplete::Allowance)));
    let recorded = db.recorded.lock().unwrap();
    assert_eq!(
        recorded
            .iter()
            .filter(|event| matches!(event, Recorded::Body(_)))
            .count(),
        3
    );
    assert!(
        recorded
            .iter()
            .any(|event| matches!(event, Recorded::Initial(_)))
    );
    assert!(
        recorded
            .iter()
            .any(|event| matches!(event, Recorded::Recovery(_, true)))
    );
    assert!(
        !recorded
            .iter()
            .any(|event| matches!(event, Recorded::Finalized(_)))
    );
    drop(recorded);
    let cycle_keys = keys(&db, input);
    assert_eq!(
        query_edges(&captured.reads, &cycle_keys),
        [(0, 1), (1, 2), (2, 0)]
    );
    assert_eq!(captured.reads.len(), 3);
    assert!(
        captured
            .reads
            .iter()
            .all(|read| read.status == Status::Provisional)
    );
    let control = owners.body_control.borrow();
    let control = control.as_ref().unwrap();
    assert_eq!(control.changed_recovery, Some(0));
    assert!(!control.refuse_after_changed_recovery && !control.armed);
    assert_eq!(control.debits.len(), 3);
    for (index, &(provider, before, after)) in control.debits.iter().enumerate() {
        assert_eq!(provider, index);
        assert_eq!(before.checked_sub(1), Some(after));
    }
    let refusal = control.refusal.as_ref().unwrap();
    assert_eq!(refusal.provider, 0);
    assert!(refusal.remaining > 0);
    assert_eq!(refusal.providers, [0; 3]);
    assert_eq!(refusal.frame, Some((cycle_keys[0], false)));
    assert!(refusal.depths.0 > 0);
    assert!(refusal.run_active);
    #[cfg(not(feature = "shuttle"))]
    assert!(matches!(refusal.sync.unwrap().owner,
        crate::function::SyncOwner::Thread(owner) if owner == std::thread::current().id()
    ));
    assert_eq!(refusal.completed_reads, [(2, 0), (1, 2), (0, 1)]);
    assert_eq!(control.completed_reads, refusal.completed_reads);
    assert!(observed.polls > 0);
    assert_eq!(observed.max_active_polls, 1);
    assert!(observed.events.iter().any(|event| matches!(event,
        observation::Event::Execute { key, .. } if *key == cycle_keys[0]
    )));
    #[cfg(not(feature = "shuttle"))]
    assert_refused_claims(&trace, &cycle_keys, true);
    assert_refused_support(&db, input, &owners);
    assert_eq!(Stamp::current(&db), stamp);
    assert_same_revision_retries(&db, input, false);
}

#[test]
fn callable_cycle_native_recovery_failure_retains_queued_child_and_provider() {
    let mut db = TestDb::default();
    let input = Input::new(&db, false);
    let stamp = Stamp::current(&db);
    let owners = Owners::new();
    owners.recovery_fault.set(true);
    let error = Cell::new(None);
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        run_cycle(&db, input, &owners, 100_000, &error, None)
    }));
    let payload = outcome.unwrap_err();
    assert!(Arc::ptr_eq(
        &payload.downcast_ref::<NativeFailure>().unwrap().0,
        &owners.native
    ));
    assert!(!owners.recovery_fault.get());
    assert!(!owners.child_polled.get());
    let snapshot = owners.recovery_drop.borrow();
    let snapshot = snapshot.as_ref().unwrap();
    assert_eq!(snapshot.providers, [0; 3]);
    assert_eq!(snapshot.identity, 17);
    assert!(snapshot.storage_available);
    // Native payloads start driver unwinding without marking the frame as an ordinary refusal.
    // The pending child is destroyed while that frame and its claim still belong to A.
    assert_eq!(snapshot.frame, Some((keys(&db, input)[0], false)));
    assert!(snapshot.depths.0 > 0);
    assert!(snapshot.claim_held);
    assert!(snapshot.panicking);
    assert_ne!(snapshot.memo_final, Some(true));
    assert_eq!(owners.journal.borrow().first(), Some(&"recovery-child"));
    owners.assert_dropped();
    assert_eq!(Stamp::current(&db), stamp);
    assert_cycle_storage(&db, input, true);
    assert!(
        !a::fn_ingredient_(&db, db.zalsa())
            .memo(db.zalsa(), input.as_id())
            .unwrap()
            .has_value()
    );

    // A native cycle panic poisons this revision even when a new provider does not inject it.
    let before = db.recorded.lock().unwrap().len();
    for _ in 0..2 {
        let retry_owners = Owners::new();
        let outcome = Cancelled::catch(AssertUnwindSafe(|| {
            run_cycle(&db, input, &retry_owners, 100_000, &error, None)
        }));
        assert!(matches!(outcome, Err(Cancelled::PropagatedPanic)));
        retry_owners.assert_dropped();
        assert_eq!(db.recorded.lock().unwrap().len(), before);
        assert_cycle_storage(&db, input, true);
        assert_eq!(Stamp::current(&db), stamp);
    }
    assert!(matches!(
        Cancelled::catch(AssertUnwindSafe(|| a(&db, input))),
        Err(Cancelled::PropagatedPanic)
    ));
    assert_eq!(db.recorded.lock().unwrap().len(), before);
    assert_cycle_storage(&db, input, true);
    assert_eq!(Stamp::current(&db), stamp);

    input.set_enabled(&mut db).to(true);
    assert_ne!(Stamp::current(&db), stamp);
    assert_same_revision_retries(&db, input, true);
}

#[crate::tracked(returns(ref), attempt = ReturnOnly)]
fn cycle_value(db: &dyn Db, input: Input) -> bool {
    db.record(Recorded::Body(3));
    *a(db, input)
}

struct CycleValueProvider<'run, 'db: 'run, A: Configuration> {
    cycle: CallableRoute<'run, 'db, A>,
}

impl<'run, 'db: 'run, C, A> CallableRouteProvider<'run, 'db, C> for CycleValueProvider<'run, 'db, A>
where
    C: for<'a> Configuration<DbView = dyn Db, Input<'a> = Input, Output<'a> = bool>,
    A: for<'a> Configuration<DbView = dyn Db, Input<'a> = Input, Output<'a> = bool>,
{
    // Input conversion constructs an Input handle; output equality compares one bool.
    fixture_native_value!(callable, 'run, 'db, C, 1);

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        input: Input,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                db.record(Recorded::Body(3));
                Ok(())
            })
            .await;
        Ok(*endpoint
            .child_call(|| async { endpoint.fetch_ref(&self.cycle, input.as_id())?.await })
            .await)
    }

    fn initial<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: Id,
        input: Input,
    ) -> impl Future<Output = RunResult<bool>> + 'call
    where
        'run: 'call,
    {
        ready(Ok(C::cycle_initial(db, id, input)))
    }

    fn recover<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call Cycle<'call>,
        last: &'call bool,
        value: bool,
        input: Input,
    ) -> impl Future<Output = RunResult<bool>> + 'call
    where
        'run: 'call,
    {
        ready(Ok(C::recover_from_cycle(db, cycle, last, value, input)))
    }
}

fn run_consuming_cycle(
    db: &TestDb,
    input: Input,
    owners: &Owners,
    validate_after: Option<Revision>,
) -> Result<AttemptOutcome<RunResult<Values>>, attempt_probe::StartError> {
    let admission = Admission::default();
    try_with_attempt(db, 100_000, || {
        let mut registry = RegistryBuilder::new(db, &admission)?;
        let routes = bind_cycle(
            &mut registry,
            db,
            owners,
            (
                a::fn_ingredient_(db, db.zalsa()),
                b::fn_ingredient_(db, db.zalsa()),
                c::fn_ingredient_(db, db.zalsa()),
            ),
        )?;
        let consumer = registry
            .reserve_callable(db as &dyn Db, cycle_value::fn_ingredient_(db, db.zalsa()))?;
        registry.bind_callable(
            &consumer,
            CycleValueProvider {
                cycle: routes.a.clone(),
            },
        )?;
        registry.seal()?.run(move |endpoint| async move {
            if let Some(revision) = validate_after {
                let result = endpoint
                    .child_call(|| async {
                        endpoint
                            .validate_callable(&consumer, input.as_id(), revision)?
                            .await
                    })
                    .await;
                assert!(matches!(result, VerifyResult::Changed));
            }
            let consumed = endpoint
                .child_call(|| async { endpoint.fetch_ref(&consumer, input.as_id())?.await })
                .await;
            let a = endpoint
                .child_call(|| async { endpoint.fetch_ref(&routes.a, input.as_id())?.await })
                .await;
            let b = endpoint
                .child_call(|| async { endpoint.fetch_ref(&routes.b, input.as_id())?.await })
                .await;
            let c = endpoint
                .child_call(|| async { endpoint.fetch_ref(&routes.c, input.as_id())?.await })
                .await;
            assert_eq!(*consumed, *a);
            Ok((*a, *b, *c))
        })
    })
}

#[test]
fn callable_validation_reexecutes_the_edited_heterogeneous_cycle() {
    let mut db = TestDb::default();
    let input = Input::new(&db, false);
    let initial_owners = Owners::new();
    assert_eq!(
        run_consuming_cycle(&db, input, &initial_owners, None),
        Ok(AttemptOutcome::Complete(Ok((
            false,
            Flag { value: false },
            Some(false)
        ))))
    );
    initial_owners.assert_dropped();
    let mut old_keys = keys(&db, input).to_vec();
    old_keys.push(cycle_value::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id()));
    let consumer_inputs = memo_inputs(
        &db,
        cycle_value::fn_ingredient_(&db, db.zalsa()),
        input,
        &old_keys,
    );
    println!("CALLABLE_INPUTS\tconsumer-before-edit\t{consumer_inputs:?}");
    println!(
        "CALLABLE_INPUTS\tcycle-before-edit\t{:?}",
        [
            memo_inputs(&db, a::fn_ingredient_(&db, db.zalsa()), input, &old_keys),
            memo_inputs(&db, b::fn_ingredient_(&db, db.zalsa()), input, &old_keys),
            memo_inputs(&db, c::fn_ingredient_(&db, db.zalsa()), input, &old_keys),
        ]
    );
    assert_eq!(consumer_inputs, [Dependency::Query(0)]);
    let old_revision = db.zalsa().current_revision();
    let old_changed = a::fn_ingredient_(&db, db.zalsa())
        .memo(db.zalsa(), input.as_id())
        .unwrap()
        .header()
        .revisions
        .changed_at;
    input.set_enabled(&mut db).to(true);
    assert_eq!(keys(&db, input).as_slice(), &old_keys[..3]);
    assert_eq!(
        cycle_value::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id()),
        old_keys[3]
    );

    let mut ordinary = TestDb::default();
    let ordinary_input = Input::new(&ordinary, false);
    assert!(!*cycle_value(&ordinary, ordinary_input));
    let mut ordinary_keys = keys(&ordinary, ordinary_input).to_vec();
    ordinary_keys.push(
        cycle_value::fn_ingredient_(&ordinary, ordinary.zalsa())
            .database_key_index(ordinary_input.as_id()),
    );
    assert_eq!(
        consumer_inputs,
        memo_inputs(
            &ordinary,
            cycle_value::fn_ingredient_(&ordinary, ordinary.zalsa()),
            ordinary_input,
            &ordinary_keys
        )
    );
    ordinary_input.set_enabled(&mut ordinary).to(true);
    let ordinary_reads = prepared_source_probe::capture(&ordinary, || {
        assert!(*cycle_value(&ordinary, ordinary_input));
        assert_eq!(
            (
                *a(&ordinary, ordinary_input),
                *b(&ordinary, ordinary_input),
                *c(&ordinary, ordinary_input)
            ),
            (true, Flag { value: true }, Some(true))
        );
    })
    .unwrap();
    ordinary_reads.check_root_reads().unwrap();
    let owners = Owners::new();
    let ((captured, trace), observed) = observation::collect(|| {
        validation_trace::collect(|| {
            prepared_source_probe::capture(&db, || {
                run_consuming_cycle(&db, input, &owners, Some(old_revision))
            })
            .unwrap()
        })
    });
    validation_trace::emit("callable-edited-cycle", &trace);
    assert_eq!(
        captured.value,
        Ok(AttemptOutcome::Complete(Ok((
            true,
            Flag { value: true },
            Some(true)
        ))))
    );
    captured.check_root_reads().unwrap();
    assert_eq!(trace.iter().filter(|event| matches!(event,
        validation_trace::TraceEvent::Request { owner, key, changed_after }
            if *owner == old_keys[3] && *key == old_keys[0] && *changed_after == old_revision
    )).count(), 1);
    assert_eq!(
        trace
            .iter()
            .filter(|event| matches!(event,
                validation_trace::TraceEvent::Reply { owner, key, result: VerifyResult::Changed }
                    if *owner == old_keys[3] && *key == old_keys[0]
            ))
            .count(),
        1
    );
    assert!(observed.events.iter().any(
        |event| matches!(event, observation::Event::Execute { key, .. } if *key == old_keys[0])
    ));
    assert_eq!(observed.max_active_polls, 1);
    assert_eq!(
        query_edges(&captured.reads, &old_keys),
        query_edges(&ordinary_reads.reads, &ordinary_keys)
    );
    owners.assert_dropped();
    assert_cycle_storage(&db, input, false);
    assert_storage(
        &db,
        cycle_value::fn_ingredient_(&db, db.zalsa()),
        input,
        false,
    );
    assert!(
        a::fn_ingredient_(&db, db.zalsa())
            .memo(db.zalsa(), input.as_id())
            .unwrap()
            .header()
            .revisions
            .changed_at
            > old_changed
    );
    assert_dependencies_match(&db, input, &ordinary, ordinary_input);
    assert_eq!(
        memo_inputs(
            &db,
            cycle_value::fn_ingredient_(&db, db.zalsa()),
            input,
            &old_keys
        ),
        memo_inputs(
            &ordinary,
            cycle_value::fn_ingredient_(&ordinary, ordinary.zalsa()),
            ordinary_input,
            &ordinary_keys
        )
    );
    let before = db.recorded.lock().unwrap().len();
    let owners = Owners::new();
    assert_eq!(
        run_consuming_cycle(&db, input, &owners, None),
        Ok(AttemptOutcome::Complete(Ok((
            true,
            Flag { value: true },
            Some(true)
        ))))
    );
    assert_eq!(db.recorded.lock().unwrap().len(), before);
    owners.assert_dropped();
    assert_cycle_storage(&db, input, false);
    assert_storage(
        &db,
        cycle_value::fn_ingredient_(&db, db.zalsa()),
        input,
        false,
    );
}

fn tuple_value(left: u32, right: u32) -> bool {
    left == right
}

#[crate::tracked(returns(ref), attempt = ReturnOnly)]
fn tuple_equal(db: &dyn Db, left: u32, right: u32) -> bool {
    db.record(Recorded::Body(3));
    tuple_value(left, right)
}

struct TupleProvider;
impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for TupleProvider
where
    C: for<'a> Configuration<DbView = dyn Db, Input<'a> = (u32, u32), Output<'a> = bool>,
{
    // The input clones two u32 fields; output equality compares one bool.
    fixture_native_value!(callable, 'run, 'db, C, 2);

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        (left, right): (u32, u32),
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                db.record(Recorded::Body(3));
                Ok(tuple_value(left, right))
            })
            .await)
    }

    fn initial<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: Id,
        input: (u32, u32),
    ) -> impl Future<Output = RunResult<bool>> + 'call
    where
        'run: 'call,
    {
        ready(Ok(C::cycle_initial(db, id, input)))
    }

    fn recover<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call Cycle<'call>,
        last: &'call bool,
        value: bool,
        input: (u32, u32),
    ) -> impl Future<Output = RunResult<bool>> + 'call
    where
        'run: 'call,
    {
        ready(Ok(C::recover_from_cycle(db, cycle, last, value, input)))
    }
}

#[test]
fn callable_fixed_keys_use_the_generated_interner_and_reject_foreign_tokens() {
    let db = TestDb::default();
    let admission = Admission::default();
    let ingredient = tuple_equal::fn_ingredient_(&db, db.zalsa());
    assert!(db.interned.lock().unwrap().is_empty());
    let outcome = try_with_attempt(&db, 100_000, || {
        let mut registry = RegistryBuilder::new(&db, &admission)?;
        let route = registry.reserve_callable(&db as &dyn Db, ingredient)?;
        let keys = registry.fixed_callable_query_keys(&route)?;
        registry.bind_callable(&route, TupleProvider)?;
        registry.seal()?.run(move |endpoint| async move {
            let id = endpoint.intern_query_key(&keys, (7, 7)).await;
            let first = endpoint
                .child_call(|| async { endpoint.fetch_ref(&route, id)?.await })
                .await;
            let repeated = endpoint.intern_query_key(&keys, (7, 7)).await;
            let second = endpoint
                .child_call(|| async { endpoint.fetch_ref(&route, repeated)?.await })
                .await;
            assert_eq!(id, repeated);
            assert!(std::ptr::eq(first, second));
            assert!(*first);
            Ok((id, first))
        })
    });
    let Ok(AttemptOutcome::Complete(Ok((id, selected)))) = outcome else {
        panic!("cold callable tuple query did not complete: {outcome:?}")
    };
    assert_eq!(&*db.recorded.lock().unwrap(), &[Recorded::Body(3)]);
    assert_eq!(
        &*db.interned.lock().unwrap(),
        &[tuple_equal::intern_ingredient_(db.zalsa()).database_key_index(id)]
    );
    let before_memo = std::ptr::from_ref(ingredient.memo(db.zalsa(), id).unwrap().header());
    assert!(std::ptr::eq(tuple_equal(&db, 7, 7), selected));
    assert_eq!(
        std::ptr::from_ref(ingredient.memo(db.zalsa(), id).unwrap().header()),
        before_memo
    );
    assert_eq!(&*db.recorded.lock().unwrap(), &[Recorded::Body(3)]);
    assert_idle(&db);

    let outcome = try_with_attempt(&db, 100_000, || {
        let mut foreign = RegistryBuilder::new(&db, &admission)?;
        let foreign_route = foreign.reserve_callable(&db as &dyn Db, ingredient)?;
        let foreign_keys = foreign.fixed_callable_query_keys(&foreign_route)?;
        let mut registry = RegistryBuilder::new(&db, &admission)?;
        assert_contract!(
            registry.fixed_callable_query_keys(&foreign_route),
            "fixed query key route is foreign"
        );
        let route = registry.reserve_callable(&db as &dyn Db, ingredient)?;
        registry.bind_callable(&route, TupleProvider)?;
        let result = registry.seal()?.run(move |endpoint| async move {
            let _id = endpoint.intern_query_key(&foreign_keys, (8, 8)).await;
            panic!("foreign key request returned")
        });
        assert_eq!(
            result,
            Err::<(), _>(RunError::Contract("fixed query key route is foreign"))
        );
        drop(foreign);
        result
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
    );
    assert_eq!(&*db.recorded.lock().unwrap(), &[Recorded::Body(3)]);
    assert_eq!(db.interned.lock().unwrap().len(), 1);
    assert_idle(&db);
}
