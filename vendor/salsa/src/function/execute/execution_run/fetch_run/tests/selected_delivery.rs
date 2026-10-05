use std::future::poll_fn;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::rc::Rc;
use std::sync::Arc;
use std::task::Poll;

use super::*;
use crate::active_query::read_storage::ReadState;
use crate::function::EvictionPolicy;
use crate::function::execute::execution_run::registration::ProviderBinding;
use crate::function::execute::execution_run::tests::validation_trace::{self, TraceEvent};
use crate::prepared_source_probe::Stamp;
use crate::{EventKind, Revision};

mod read_admission;

struct RecordingEviction;

impl EvictionPolicy for RecordingEviction {
    fn new(_capacity: usize) -> Self {
        Self
    }
    fn record_use(&self, id: Id) {
        on_eviction(id);
    }
    fn set_capacity(&mut self, _capacity: usize) {}
    fn for_each_evicted(&mut self, _cb: impl FnMut(Id)) {}
}

crate::plumbing::setup_tracked_fn! {
    attrs: [],
    vis: ,
    fn_name: eviction_leaf,
    db_lt: 'db,
    Db: crate::Database,
    db: db,
    input_ids: [input],
    input_tys: [Number],
    interned_input_tys: [Number],
    output_ty: u32,
    inner_fn: {
        fn eviction_inner(db: &dyn crate::Database, input: Number) -> u32 {
            eviction_body(db, input)
        }
    },
    cycle_recovery_fn: (salsa::plumbing::unexpected_cycle_recovery!),
    cycle_recovery_initial: (salsa::plumbing::unexpected_cycle_initial!),
    cycle_recovery_strategy: Panic,
    attempt_policy: ReturnOnly,
    is_specifiable: false,
    values_equal: {
        fn values_equal<'db>(old_value: &Self::Output<'db>, new_value: &Self::Output<'db>) -> bool {
            old_value == new_value
        }
    },
    needs_interner: false,
    heap_size_fn: ,
    eviction: RecordingEviction,
    lru: 0,
    return_mode: copy,
    persist: false,
    assert_interned_inputs_are_salsa_values: {
        crate::plumbing::assert_salsa_value::<Number>();
    },
    assert_output_is_salsa_value_or_static: {
        fn assert_eviction_output() { crate::plumbing::assert_salsa_value::<u32>(); }
        let _ = assert_eviction_output;
    },
    unused_names: [
        eviction_salsa, EvictionConfiguration, EvictionInternedData,
        EVICTION_FN_CACHE, EVICTION_INTERN_CACHE, eviction_inner,
    ]
}

fn eviction_body(db: &dyn Database, input: Number) -> u32 {
    BODY_CALLS.with(|calls| calls.set(calls.get() + 1));
    input.value(db)
}

#[crate::input]
struct ReadRequest {
    #[returns(copy)]
    target: Number,
    #[returns(copy)]
    decoy: Number,
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn read_caller(db: &dyn Database, call: ReadRequest) -> u32 {
    let _marker = Marker("caller.drop");
    assert_eq!(eviction_leaf(db, call.decoy(db)), 11);
    let value = eviction_leaf(db, call.target(db));
    note("delivered");
    value + 1
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Point {
    InitialCheck,
    Eviction,
    FinalCheck,
}

impl Point {
    fn name(self) -> &'static str {
        match self {
            Self::InitialCheck => "initial",
            Self::Eviction => "eviction",
            Self::FinalCheck => "final",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fault {
    Quiet,
    ChildOnly,
    ChildRefuse,
    ChildPanic,
}

fn faults() -> Vec<Fault> {
    let mut faults = vec![Fault::ChildOnly, Fault::ChildRefuse];
    if !cfg!(feature = "shuttle") {
        faults.push(Fault::ChildPanic);
    }
    faults
}

#[derive(Debug)]
struct Payload(Arc<()>);

#[derive(Clone, Debug, Eq, PartialEq)]
struct Snapshot {
    memo: usize,
    value: usize,
    verified: Revision,
    changed: Revision,
    operations: Vec<usize>,
    caller: Option<DatabaseKeyIndex>,
    depths: (usize, usize),
    target_claimed: bool,
    caller_claimed: bool,
    inputs: Vec<DatabaseKeyIndex>,
    read_state: Option<ReadState>,
    reason: Option<Incomplete>,
    panicking: bool,
}

struct Script {
    db: &'static HookDb,
    target: Number,
    key: DatabaseKeyIndex,
    caller: DatabaseKeyIndex,
    point: Point,
    fault: Fault,
    ordinary: bool,
    endpoint: Option<Endpoint<'static, 'static>>,
    caller_task: Option<Rc<Cell<bool>>>,
    awaiting: Option<DatabaseKeyIndex>,
    policy_calls: usize,
    decoy_calls: usize,
    fired: bool,
    identity: Arc<()>,
    notes: Vec<(&'static str, Snapshot)>,
    admissions: Vec<ExecutionWork>,
    cancellations: usize,
}

thread_local! {
    static SCRIPT: RefCell<Option<Script>> = const { RefCell::new(None) };
    static BODY_CALLS: Cell<usize> = const { Cell::new(0) };
}

struct Clear;
impl Drop for Clear {
    fn drop(&mut self) {
        SCRIPT.with_borrow_mut(|slot| *slot = None);
    }
}

#[crate::db]
#[derive(Clone)]
struct HookDb {
    storage: crate::Storage<Self>,
}
#[crate::db]
impl Database for HookDb {}

impl Default for HookDb {
    fn default() -> Self {
        Self {
            storage: crate::Storage::new(Some(Box::new(|event| {
                if matches!(event.kind, EventKind::WillCheckCancellation) {
                    SCRIPT.with_borrow_mut(|slot| {
                        if let Some(script) = slot {
                            script.cancellations += 1;
                        }
                    });
                    on_cancellation();
                }
            }))),
        }
    }
}

fn is_claimed(db: &dyn Database, key: DatabaseKeyIndex) -> bool {
    matches!(
        db.zalsa()
            .lookup_ingredient(key.ingredient_index())
            .as_function()
            .unwrap()
            .sync_table()
            .peek_claim(db.zalsa(), key.key_index(), Reentrancy::Deny),
        ClaimResult::Cycle { .. }
    )
}

fn snapshot() -> Option<Snapshot> {
    let (db, target, key, caller, endpoint) = SCRIPT.with_borrow(|slot| {
        let script = slot.as_ref()?;
        Some((
            script.db,
            script.target,
            script.key,
            script.caller,
            script.endpoint.clone(),
        ))
    })?;
    let selected = memo(db, eviction_leaf::fn_ingredient_(db, db.zalsa()), target);
    Some(Snapshot {
        memo: std::ptr::from_ref(selected).addr(),
        value: std::ptr::from_ref(selected.value().unwrap()).addr(),
        verified: selected.header.verified_at.load(),
        changed: selected.header.revisions.changed_at,
        operations: endpoint.as_ref().map_or_else(Vec::new, |endpoint| {
            endpoint.context.operations.borrow().clone()
        }),
        caller: db.zalsa_local().active_query().map(|(key, _)| key),
        depths: attempt_probe::stack_depths(),
        target_claimed: is_claimed(db, key),
        caller_claimed: is_claimed(db, caller),
        inputs: inputs(db),
        read_state: db
            .zalsa_local()
            .try_with_query_stack(|stack| stack.last().map(|query| query.read_state()))
            .expect("snapshot query stack is available"),
        reason: attempt_probe::current().and_then(|support| support.reason()),
        panicking: crate::sync::thread::panicking(),
    })
}

fn note(stage: &'static str) {
    if let Some(observed) = snapshot() {
        SCRIPT.with_borrow_mut(|slot| slot.as_mut().unwrap().notes.push((stage, observed)));
    }
}

struct Marker(&'static str);
impl Drop for Marker {
    fn drop(&mut self) {
        note(self.0);
    }
}

fn selected_boundary() -> bool {
    let Some((endpoint, caller_task, awaiting, key, caller)) = SCRIPT.with_borrow(|slot| {
        let script = slot.as_ref()?;
        if script.fired || script.ordinary {
            return None;
        }
        Some((
            script.endpoint.clone()?,
            script.caller_task.clone()?,
            script.awaiting?,
            script.key,
            script.caller,
        ))
    }) else {
        return false;
    };
    if awaiting != key {
        return false;
    }
    let Some(task) = endpoint
        .queue
        .active_poll
        .borrow()
        .as_ref()
        .map(|poll| poll.identity.wanted.clone())
    else {
        return false;
    };
    if Rc::ptr_eq(&task, &caller_task) {
        return false;
    }
    let operations = endpoint.context.operations.borrow();
    let Some(operation) = operations.last().copied() else {
        return false;
    };
    let Some((
        observed_key,
        TraceEvent::Outer {
            owner,
            query_depth,
            operation_depth,
            ..
        },
    )) = validation_trace::hot_selected(operation)
    else {
        return false;
    };
    observed_key == key
        && owner == Some(caller)
        && operation_depth == attempt_probe::stack_depths().0
        && endpoint
            .context
            .db
            .zalsa_local()
            .try_with_query_stack(|stack| stack.len())
            == Some(query_depth)
        && operations.len() == 2
        && endpoint
            .context
            .db
            .zalsa_local()
            .active_query()
            .map(|(key, _)| key)
            == Some(caller)
}

fn on_cancellation() {
    if !selected_boundary() {
        return;
    }
    let observed = snapshot().unwrap();
    let point = SCRIPT.with_borrow(|slot| {
        let script = slot.as_ref().unwrap();
        if script.policy_calls == 0 && !observed.inputs.contains(&script.key) {
            Some(Point::InitialCheck)
        } else if script.policy_calls == 1
            && observed
                .inputs
                .iter()
                .filter(|key| **key == script.key)
                .count()
                == 1
        {
            Some(Point::FinalCheck)
        } else {
            None
        }
    });
    if let Some(point) = point {
        at(point);
    }
}

fn on_eviction(id: Id) {
    let Some((target, caller, ordinary, db)) = SCRIPT.with_borrow(|slot| {
        let script = slot.as_ref()?;
        Some((
            script.target.as_id(),
            script.caller,
            script.ordinary,
            script.db,
        ))
    }) else {
        return;
    };
    if id != target {
        SCRIPT.with_borrow_mut(|slot| slot.as_mut().unwrap().decoy_calls += 1);
        return;
    }
    assert_eq!(
        db.zalsa_local().active_query().map(|(key, _)| key),
        Some(caller)
    );
    if !ordinary && !selected_boundary() {
        return;
    }
    SCRIPT.with_borrow_mut(|slot| slot.as_mut().unwrap().policy_calls += 1);
    at(Point::Eviction);
}

fn at(point: Point) {
    note(point.name());
    let action = SCRIPT.with_borrow_mut(|slot| {
        let script = slot.as_mut().unwrap();
        if script.fired || script.point != point || script.fault == Fault::Quiet {
            return None;
        }
        script.fired = true;
        Some((
            script.db,
            script.endpoint.clone(),
            script.fault,
            script.identity.clone(),
            script.ordinary,
        ))
    });
    let Some((db, endpoint, fault, identity, ordinary)) = action else {
        return;
    };
    if !ordinary {
        let endpoint = endpoint.unwrap();
        let child = Marker("child.drop");
        let _reply = endpoint
            .demand(move || {
                poll_fn(move |_| -> Poll<RunResult<()>> {
                    let _child = &child;
                    panic!("rejected selected-read child must not execute")
                })
            })
            .unwrap();
    }
    match fault {
        Fault::Quiet | Fault::ChildOnly => {}
        Fault::ChildRefuse => {
            attempt_probe::report_incomplete(db, Incomplete::Allowance);
        }
        Fault::ChildPanic => panic_any(Payload(identity)),
    }
}

struct LeafProvider;
impl<C> ExecutableRouteProvider<'static, 'static, C> for LeafProvider
where
    C: Configuration<DbView = dyn Database, Input<'static> = Number, Output<'static> = u32>,
{
    async fn native_value<'call>(
        &'static self,
        _context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        operation: NativeValueOperation<'call, 'static, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'static: 'call,
    {
        native_value_quote(operation)
    }

    async fn body(
        &'static self,
        _context: ProviderContext<'static, 'static, Self>,
        db: &'static dyn Database,
        input: Number,
    ) -> RunResult<u32> {
        Ok(eviction_body(db, input))
    }
    async fn initial(
        &'static self,
        _context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        _id: Id,
        _input: Number,
    ) -> RunResult<u32> {
        Err(RunError::Contract("finite leaf has no cycle initializer"))
    }
    async fn recover<'call>(
        &'static self,
        _context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call u32,
        _value: u32,
        _input: Number,
    ) -> RunResult<u32>
    where
        'static: 'call,
    {
        Err(RunError::Contract("finite leaf has no cycle recovery"))
    }
}

struct CallerProvider<L: Configuration> {
    leaf: Route<'static, L>,
    binding: ProviderBinding<'static, LeafProvider>,
}
impl<L, C> ExecutableRouteProvider<'static, 'static, C> for CallerProvider<L>
where
    L: Configuration<DbView = dyn Database, Input<'static> = Number, Output<'static> = u32>,
    C: Configuration<DbView = dyn Database, Input<'static> = ReadRequest, Output<'static> = u32>,
{
    async fn native_value<'call>(
        &'static self,
        _context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        operation: NativeValueOperation<'call, 'static, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'static: 'call,
    {
        // ReadRequest conversion constructs one generated handle; the result compares a u32.
        match operation {
            NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_))
            | NativeValueOperation::Comparison { .. } => Ok(NativeValueQuote {
                work: 1,
                requested_bytes: 0,
                cleanup_work: 0,
            }),
            NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => Err(
                RunError::Contract("selected-delivery fixture requires a ReadRequest handle"),
            ),
        }
    }

    async fn body(
        &'static self,
        context: ProviderContext<'static, 'static, Self>,
        db: &'static dyn Database,
        call: ReadRequest,
    ) -> RunResult<u32> {
        let target = call.target(db);
        let decoy = call.decoy(db);
        let _marker = Marker("caller.drop");
        let endpoint = context.endpoint().inner.clone();
        let task = endpoint
            .queue
            .active_poll
            .borrow()
            .as_ref()
            .unwrap()
            .identity
            .wanted
            .clone();
        SCRIPT.with_borrow_mut(|slot| {
            let script = slot.as_mut().unwrap();
            script.endpoint = Some(endpoint);
            script.caller_task = Some(task);
        });
        let leaf = context.endpoint().provider(self.binding.clone())?;
        for input in [decoy, target] {
            SCRIPT.with_borrow_mut(|slot| {
                slot.as_mut().unwrap().awaiting = Some(
                    eviction_leaf::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
                );
            });
            let value = leaf.fetch_ref(&self.leaf, input.as_id())?.await?;
            if input == decoy {
                assert_eq!(*value, 11);
            } else {
                note("delivered");
                return Ok(*value + 1);
            }
        }
        unreachable!()
    }
    async fn initial(
        &'static self,
        _context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        _id: Id,
        _input: ReadRequest,
    ) -> RunResult<u32> {
        Err(RunError::Contract("finite caller has no cycle initializer"))
    }
    async fn recover<'call>(
        &'static self,
        _context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call u32,
        _value: u32,
        _input: ReadRequest,
    ) -> RunResult<u32>
    where
        'static: 'call,
    {
        Err(RunError::Contract("finite caller has no cycle recovery"))
    }
}

struct Admission;
impl ExecutionAdmission for Admission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        SCRIPT.with_borrow_mut(|slot| {
            if let Some(script) = slot {
                script.admissions.push(work);
            }
        });
        Ok(())
    }
}
static ADMISSION: Admission = Admission;
static LEAF_PROVIDER: LeafProvider = LeafProvider;

struct Fixture {
    db: &'static HookDb,
    target: Number,
    calls: [ReadRequest; 3],
    key: DatabaseKeyIndex,
    selected: (usize, usize),
    stamp: Stamp,
}
impl Fixture {
    fn new() -> Self {
        assert!(SCRIPT.with_borrow(Option::is_none));
        BODY_CALLS.with(|calls| calls.set(0));
        let db = HookDb::default();
        let target = Number::new(&db, 7);
        let decoy = Number::new(&db, 11);
        let calls = std::array::from_fn(|_| ReadRequest::new(&db, target, decoy));
        assert_eq!(eviction_leaf(&db, target), 7);
        assert_eq!(eviction_leaf(&db, decoy), 11);
        let db = Box::leak(Box::new(db));
        let ingredient = eviction_leaf::fn_ingredient_(db, db.zalsa());
        let selected = memo(db, ingredient, target);
        Self {
            db,
            target,
            calls,
            key: ingredient.database_key_index(target.as_id()),
            selected: (
                std::ptr::from_ref(selected).addr(),
                std::ptr::from_ref(selected.value().unwrap()).addr(),
            ),
            stamp: Stamp::current(db),
        }
    }
    fn caller(&self, index: usize) -> DatabaseKeyIndex {
        read_caller::fn_ingredient_(self.db, self.db.zalsa())
            .database_key_index(self.calls[index].as_id())
    }
    fn install(&self, index: usize, point: Point, fault: Fault, ordinary: bool) -> Clear {
        SCRIPT.with_borrow_mut(|slot| {
            assert!(slot.is_none());
            *slot = Some(Script {
                db: self.db,
                target: self.target,
                key: self.key,
                caller: self.caller(index),
                point,
                fault,
                ordinary,
                endpoint: None,
                caller_task: None,
                awaiting: None,
                policy_calls: 0,
                decoy_calls: 0,
                fired: false,
                identity: Arc::new(()),
                notes: Vec::new(),
                admissions: Vec::new(),
                cancellations: 0,
            });
        });
        Clear
    }
    fn run(&self, index: usize) -> RunResult<u32> {
        let db: &'static dyn Database = self.db;
        let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
        let leaf = registry.reserve(db, eviction_leaf::fn_ingredient_(db, db.zalsa()))?;
        let caller = registry.reserve(db, read_caller::fn_ingredient_(db, db.zalsa()))?;
        let leaf_binding = registry.provider(&LEAF_PROVIDER)?;
        registry.bind_executable(&leaf, &leaf_binding)?;
        let provider = Box::leak(Box::new(CallerProvider {
            leaf,
            binding: leaf_binding,
        }));
        let binding = registry.provider(provider)?;
        registry.bind_executable(&caller, &binding)?;
        let call = self.calls[index];
        registry.seal()?.run(move |endpoint| async move {
            Ok(*endpoint
                .provider(binding)?
                .fetch_ref(&caller, call.as_id())?
                .await?)
        })
    }
    fn assert_cached_and_idle(&self, index: usize) {
        assert_eq!(Stamp::current(self.db), self.stamp);
        assert_eq!(BODY_CALLS.with(Cell::get), 2);
        let current = memo(
            self.db,
            eviction_leaf::fn_ingredient_(self.db, self.db.zalsa()),
            self.target,
        );
        assert_eq!(std::ptr::from_ref(current).addr(), self.selected.0);
        assert_eq!(
            std::ptr::from_ref(current.value().unwrap()).addr(),
            self.selected.1
        );
        assert!(!current.header.has_incomplete_attempt());
        assert!(!current.header.may_be_provisional());
        assert!(self.db.zalsa_local().active_query().is_none());
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert_eq!(
            self.db
                .zalsa()
                .attempt_operations
                .load(crate::sync::atomic::Ordering::SeqCst),
            0
        );
        assert!(!is_claimed(self.db, self.key));
        assert!(!is_claimed(self.db, self.caller(index)));
    }
}

fn target_events<'a>(
    fixture: &Fixture,
    events: &'a [completion::Event],
) -> Vec<&'a completion::Event> {
    events
        .iter()
        .filter(|event| event.key == fixture.key)
        .collect()
}

fn assert_read(
    fixture: &Fixture,
    index: usize,
    events: &[completion::Event],
    reads: &[prepared_source_probe::Read],
) {
    let events = target_events(fixture, events);
    assert_eq!(
        events.iter().map(|event| event.stage).collect::<Vec<_>>(),
        [Stage::Eviction, Stage::TrackedRead, Stage::PreparedSource]
    );
    for event in events {
        assert_eq!(event.memo_address, fixture.selected.0);
        assert_eq!(event.caller, Some(fixture.caller(index)));
        if event.stage != Stage::Eviction {
            assert!(event.inputs.contains(&fixture.key));
        }
    }
    let reads: Vec<_> = reads
        .iter()
        .filter(|read| read.key == fixture.key)
        .collect();
    assert_eq!(reads.len(), 1);
    assert_eq!(reads[0].memo_address, fixture.selected.0);
    assert_eq!(reads[0].parent, Some(fixture.caller(index)));
    assert_eq!(reads[0].status, Status::Final);
}

fn quiet(fixture: &Fixture, index: usize, ordinary: bool) {
    let _clear = fixture.install(index, Point::Eviction, Fault::Quiet, ordinary);
    let ((captured, trace), events) = completion::collect(|| {
        validation_trace::collect(|| {
            prepared_source_probe::capture(fixture.db, || {
                if ordinary {
                    Ok(read_caller(fixture.db, fixture.calls[index]))
                } else {
                    match try_with_attempt(fixture.db, 100_000, || fixture.run(index)).unwrap() {
                        AttemptOutcome::Complete(result) => result,
                        other => panic!("quiet fetch was incomplete: {other:?}"),
                    }
                }
            })
            .unwrap()
        })
    });
    assert_eq!(captured.value, Ok(8));
    assert_read(fixture, index, &events, &captured.reads);
    SCRIPT.with_borrow(|slot| {
        let script = slot.as_ref().unwrap();
        assert!(!script.fired);
        assert_eq!(script.policy_calls, 1);
        assert_eq!(script.decoy_calls, 1);
        let stages: Vec<_> = script.notes.iter().map(|(stage, _)| *stage).collect();
        assert_eq!(
            stages,
            if ordinary {
                vec!["eviction", "delivered", "caller.drop"]
            } else {
                vec!["initial", "eviction", "final", "delivered", "caller.drop"]
            }
        );
        if !ordinary {
            eprintln!(
                "SELECTED_DELIVERY admissions={:?} cancellation_callbacks={}",
                script.admissions, script.cancellations
            );
            assert_eq!(
                trace
                    .iter()
                    .filter(|event| matches!(
                        event,
                        TraceEvent::Outer {
                            phase: "fetch.selected",
                            ..
                        }
                    ))
                    .count(),
                3
            );
        }
    });
    fixture.assert_cached_and_idle(index);
}

fn reject(point: Point, fault: Fault) {
    let fixture = Fixture::new();
    let clear = fixture.install(0, point, fault, false);
    let identity = SCRIPT.with_borrow(|slot| slot.as_ref().unwrap().identity.clone());
    let mut returned = None;
    let ((captured, _trace), events) = completion::collect(|| {
        validation_trace::collect(|| {
            prepared_source_probe::capture(fixture.db, || {
                catch_unwind(AssertUnwindSafe(|| {
                    try_with_attempt(fixture.db, 100_000, || {
                        let result = fixture.run(0);
                        returned = Some(result);
                        result
                    })
                }))
            })
            .unwrap()
        })
    });
    // Check semantic effects before outcome or accepted-phase assertions on predecessor runtimes.
    let reads: Vec<_> = captured
        .reads
        .iter()
        .filter(|read| read.key == fixture.key)
        .collect();
    if point == Point::FinalCheck {
        assert_read(&fixture, 0, &events, &captured.reads);
    } else {
        assert!(reads.is_empty(), "rejected read recorded a prepared source");
        let expected = if point == Point::Eviction && fault != Fault::ChildPanic {
            vec![Stage::Eviction]
        } else {
            Vec::new()
        };
        assert_eq!(
            target_events(&fixture, &events)
                .iter()
                .map(|event| event.stage)
                .collect::<Vec<_>>(),
            expected
        );
    }
    SCRIPT.with_borrow(|slot| {
        let script = slot.as_ref().unwrap();
        assert!(script.fired);
        assert_eq!(script.decoy_calls, 1);
        assert!(!script.notes.iter().any(|(stage, _)| *stage == "delivered"));
        let locate = |stage| {
            script
                .notes
                .iter()
                .position(|(name, _)| *name == stage)
                .unwrap()
        };
        let trigger = locate(point.name());
        let child = locate("child.drop");
        let caller = locate("caller.drop");
        assert!(trigger < child && child < caller);
        let original = &script.notes[trigger].1;
        let child = &script.notes[child].1;
        assert_eq!(
            child.operations, original.operations,
            "selected operation escaped before child cleanup"
        );
        assert_eq!(child.caller, original.caller);
        assert_eq!(child.depths, original.depths);
        assert_eq!(child.inputs, original.inputs);
        assert_eq!((child.memo, child.value), fixture.selected);
        assert_eq!(
            (child.verified, child.changed),
            (original.verified, original.changed)
        );
        assert!(!child.target_claimed);
        assert!(child.caller_claimed);
        assert_eq!(child.caller, Some(fixture.caller(0)));
        assert_eq!(
            child.inputs.contains(&fixture.key),
            point == Point::FinalCheck
        );
        assert_eq!(
            script.notes[caller].1.inputs.contains(&fixture.key),
            point == Point::FinalCheck
        );
        assert_eq!(
            child.reason,
            match fault {
                Fault::ChildOnly => Some(Incomplete::Interrupted),
                Fault::ChildRefuse => Some(Incomplete::Allowance),
                _ => None,
            }
        );
        assert_eq!(child.panicking, fault == Fault::ChildPanic);
    });
    match fault {
        Fault::ChildPanic => {
            assert!(returned.is_none());
            let payload = captured.value.unwrap_err();
            assert!(Arc::ptr_eq(
                &payload.downcast_ref::<Payload>().unwrap().0,
                &identity
            ));
        }
        Fault::ChildOnly | Fault::ChildRefuse => {
            let (error, reason) = if fault == Fault::ChildOnly {
                (
                    RunError::Contract("completed task retained a child"),
                    Incomplete::Interrupted,
                )
            } else {
                (
                    RunError::Refused(Incomplete::Allowance),
                    Incomplete::Allowance,
                )
            };
            assert_eq!(returned, Some(Err(error)));
            assert_eq!(
                captured.value.unwrap(),
                Ok(AttemptOutcome::Incomplete(reason))
            );
        }
        Fault::Quiet => unreachable!(),
    }
    fixture.assert_cached_and_idle(0);
    drop(clear);
    quiet(&fixture, 1, false);
}

#[test]
fn selected_read_checks_retain_the_caller_and_selection() {
    for point in [Point::InitialCheck, Point::FinalCheck] {
        for fault in faults() {
            reject(point, fault);
        }
    }
}

#[test]
fn configured_eviction_rejection_records_no_dependency() {
    for fault in faults() {
        reject(Point::Eviction, fault);
    }
}

#[test]
fn selected_read_quiet_retry_preserves_order_and_cached_value() {
    let fixture = Fixture::new();
    quiet(&fixture, 0, false);
    quiet(&fixture, 1, true);
}

#[test]
#[cfg(not(feature = "shuttle"))]
fn configured_eviction_ordinary_panic_precedes_dependency_recording() {
    let fixture = Fixture::new();
    let clear = fixture.install(0, Point::Eviction, Fault::ChildPanic, true);
    let identity = SCRIPT.with_borrow(|slot| slot.as_ref().unwrap().identity.clone());
    let (captured, events) = completion::collect(|| {
        prepared_source_probe::capture(fixture.db, || {
            catch_unwind(AssertUnwindSafe(|| {
                read_caller(fixture.db, fixture.calls[0])
            }))
        })
        .unwrap()
    });
    assert!(target_events(&fixture, &events).is_empty());
    assert!(!captured.reads.iter().any(|read| read.key == fixture.key));
    let payload = captured.value.unwrap_err();
    assert!(Arc::ptr_eq(
        &payload.downcast_ref::<Payload>().unwrap().0,
        &identity
    ));
    SCRIPT.with_borrow(|slot| {
        let script = slot.as_ref().unwrap();
        assert!(script.fired);
        assert_eq!(
            script
                .notes
                .iter()
                .map(|(stage, _)| *stage)
                .collect::<Vec<_>>(),
            ["eviction", "caller.drop"]
        );
        assert!(!script.notes.last().unwrap().1.inputs.contains(&fixture.key));
    });
    fixture.assert_cached_and_idle(0);
    drop(clear);
    quiet(&fixture, 1, true);
}
