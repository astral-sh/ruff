use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::future::{Future, poll_fn, ready};
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::sync::Arc;
use std::task::Poll;

use super::super::registration::{
    ExecutableRouteProvider, ProviderContext, RegistryBuilder, Route,
};
use super::super::{Endpoint, ExecutionAdmission, ExecutionWork, RunError, RunResult};
use crate::attempt_probe::{self, AttemptOutcome, Incomplete, try_with_attempt};
use crate::cycle::IterationStamp;
use crate::function::{ClaimResult, Configuration, Reentrancy};
use crate::plumbing::AsId;
use crate::prepared_source_probe::Stamp;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseKeyIndex, EventKind, Id, Revision, Setter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Point {
    Backdate,
    Iterate,
    FinalizeBackdate,
    ReturnBackdate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fault {
    Quiet,
    Refuse,
    Panic,
    Local,
}

#[derive(Debug, Eq, PartialEq)]
struct Header {
    identity: usize,
    verified: Revision,
    changed: Revision,
    provisional: bool,
    iteration: IterationStamp,
    heads: Vec<(DatabaseKeyIndex, IterationStamp)>,
}

fn headers(db: &dyn Database, keys: &[DatabaseKeyIndex]) -> Vec<Header> {
    keys.iter()
        .map(|key| {
            let memo = db
                .zalsa()
                .lookup_ingredient(key.ingredient_index())
                .as_function()
                .unwrap()
                .memo(db.zalsa(), key.key_index())
                .expect("the callout has an existing selected memo");
            let header = memo.header();
            Header {
                identity: std::ptr::from_ref(header) as usize,
                verified: header.verified_at.load(),
                changed: header.revisions.changed_at,
                provisional: header.may_be_provisional(),
                iteration: header.revisions.iteration(),
                heads: header
                    .revisions
                    .cycle_heads()
                    .iter()
                    .map(|head| (head.database_key_index, head.iteration.load()))
                    .collect(),
            }
        })
        .collect()
}

#[derive(Debug)]
struct Observation {
    stage: &'static str,
    frame: Option<DatabaseKeyIndex>,
    query_depth: usize,
    operation_depth: usize,
    claim: bool,
    reason: Option<Incomplete>,
    panicking: bool,
    headers: Vec<Header>,
    requested: bool,
    enabled: bool,
}

struct State {
    point: Point,
    fault: Fault,
    keys: Vec<DatabaseKeyIndex>,
    old_value: usize,
    endpoint: Option<Endpoint<'static, 'static>>,
    identity: Arc<()>,
    fired: bool,
    candidate: usize,
    serial: usize,
    comparison: Option<(u32, u32)>,
    next_iteration: Option<u8>,
    resumed_headers: Option<Vec<Header>>,
    observations: Vec<Observation>,
    native_local: bool,
    cycle_calls: [[usize; 2]; 2],
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    static ORDINARY_SCALARS: Cell<usize> = const { Cell::new(0) };
}

struct Reset;

impl Drop for Reset {
    fn drop(&mut self) {
        STATE.with_borrow_mut(|state| *state = None);
    }
}

fn begin(point: Point, fault: Fault, keys: Vec<DatabaseKeyIndex>, old_value: usize) -> Reset {
    STATE.with_borrow_mut(|state| {
        assert!(state.is_none());
        *state = Some(State {
            point,
            fault,
            keys,
            old_value,
            endpoint: None,
            identity: Arc::new(()),
            fired: false,
            candidate: 0,
            serial: 0,
            comparison: None,
            next_iteration: None,
            resumed_headers: None,
            observations: Vec::new(),
            native_local: false,
            cycle_calls: [[0; 2]; 2],
        });
    });
    Reset
}

fn observe(stage: &'static str) {
    let keys = STATE.with_borrow(|state| {
        state
            .as_ref()
            .filter(|state| state.fired)
            .map(|state| state.keys.clone())
    });
    let Some(keys) = keys else { return };
    let observation = crate::with_attached_database(|db| Observation {
        stage,
        frame: db.zalsa_local().active_query().map(|(key, _)| key),
        query_depth: db
            .zalsa_local()
            .try_with_query_stack(|stack| stack.len())
            .unwrap(),
        operation_depth: attempt_probe::stack_depths().0,
        claim: matches!(
            db.zalsa()
                .lookup_ingredient(keys[0].ingredient_index())
                .as_function()
                .unwrap()
                .sync_table()
                .peek_claim(db.zalsa(), keys[0].key_index(), Reentrancy::Deny),
            ClaimResult::Cycle { .. }
        ),
        reason: attempt_probe::current().and_then(|support| support.reason()),
        panicking: crate::sync::thread::panicking(),
        headers: if matches!(
            stage,
            "callout" | "child" | "local.checked" | "native.check" | "native.return"
        ) {
            headers(db, &keys)
        } else {
            Vec::new()
        },
        requested: db.cancellation_token().is_cancelled(),
        enabled: db.zalsa_local().should_trigger_local_cancellation(),
    })
    .expect("query cleanup keeps the database attached");
    STATE.with_borrow_mut(|state| state.as_mut().unwrap().observations.push(observation));
}

struct Marker(&'static str);

impl Drop for Marker {
    fn drop(&mut self) {
        observe(self.0);
    }
}

#[derive(Debug)]
struct PanicMarker(Arc<()>);

fn fire() {
    let hook = STATE.with_borrow_mut(|state| {
        let state = state.as_mut()?;
        if state.fired {
            return None;
        }
        let endpoint = state.endpoint.take();
        if endpoint.is_none() && !state.native_local {
            return None;
        }
        state.fired = true;
        Some((endpoint, state.fault, state.identity.clone()))
    });
    let Some((endpoint, fault, identity)) = hook else {
        return;
    };
    observe("callout");
    if fault == Fault::Quiet {
        return;
    }
    if fault == Fault::Local {
        crate::with_attached_database(|db| {
            db.cancellation_token().cancel();
            assert!(!db.zalsa_local().should_trigger_local_cancellation());
            db.unwind_if_revision_cancelled();
        })
        .expect("backdating retains the attached database");
        observe("local.checked");
        return;
    }
    let endpoint = endpoint.expect("controlled failure retains its endpoint");
    let child = Marker("child");
    let _reply = endpoint
        .demand(move || {
            poll_fn(move |_| -> Poll<RunResult<()>> {
                let _child = &child;
                panic!("a child queued by a rejected completion must not run");
            })
        })
        .expect("the completion callout retains the root poll");
    match fault {
        Fault::Refuse => {
            attempt_probe::report_incomplete(endpoint.context.db, Incomplete::Interrupted);
        }
        Fault::Panic => panic_any(PanicMarker(identity)),
        Fault::Quiet | Fault::Local => {}
    }
}

fn arm(endpoint: Endpoint<'static, 'static>, candidate: usize) {
    STATE.with_borrow_mut(|state| {
        let state = state.as_mut().unwrap();
        if !state.fired {
            state.endpoint = Some(endpoint);
            state.candidate = candidate;
        }
    });
}

#[crate::db]
#[derive(Clone)]
struct Db {
    storage: crate::Storage<Self>,
}

#[crate::db]
impl Database for Db {}

fn database() -> Db {
    Db {
        storage: crate::Storage::new(Some(Box::new(|event| {
            if matches!(&event.kind, EventKind::WillCheckCancellation)
                && STATE.with_borrow(|state| {
                    state
                        .as_ref()
                        .is_some_and(|state| state.fired && state.fault == Fault::Local)
                })
            {
                observe("native.check");
            }
            if let EventKind::WillIterateCycle {
                database_key,
                iteration,
            } = event.kind
            {
                let armed = STATE.with_borrow_mut(|state| {
                    let Some(state) = state else { return false };
                    if state.point != Point::Iterate || state.keys[0] != database_key || state.fired
                    {
                        return false;
                    }
                    state.next_iteration = Some(iteration);
                    true
                });
                if armed {
                    fire();
                }
            }
        }))),
    }
}

#[derive(Debug)]
struct Value {
    value: u32,
    tag: usize,
}

impl Value {
    fn plain(value: u32) -> Self {
        Self { value, tag: 0 }
    }
    fn candidate(value: u32) -> Self {
        let tag = STATE.with_borrow_mut(|state| {
            let state = state.as_mut().unwrap();
            state.serial += 1;
            state.serial
        });
        Self { value, tag }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        let armed = STATE.with_borrow_mut(|state| {
            let Some(state) = state else { return false };
            if state.point == Point::Iterate
                || state.fired
                || state.old_value != std::ptr::from_ref(self) as usize
            {
                return false;
            }
            state.comparison = Some((self.value, other.value));
            true
        });
        if armed {
            fire();
        }
        self.value == other.value
    }
}
impl Eq for Value {}

impl Drop for Value {
    fn drop(&mut self) {
        if self.tag != 0
            && STATE.with_borrow(|state| {
                state.as_ref().is_some_and(|state| {
                    state.fired && state.fault != Fault::Quiet && state.candidate == self.tag
                })
            })
        {
            observe("candidate");
        }
    }
}

#[crate::input]
struct Input {
    #[returns(copy)]
    value: u32,
    #[returns(copy)]
    cyclic: bool,
}

#[crate::tracked(returns(ref), attempt = ReturnOnly)]
fn scalar(db: &dyn Database, input: Input) -> Value {
    ORDINARY_SCALARS.set(ORDINARY_SCALARS.get() + 1);
    Value::plain(input.value(db) / 10)
}

#[crate::tracked(returns(ref), attempt = ReturnOnly)]
fn parent(db: &dyn Database, input: Input) -> Value {
    Value::plain(if input.cyclic(db) {
        a(db, input).value
    } else {
        scalar(db, input).value
    })
}

fn count_cycle(controlled: bool, index: usize) {
    STATE.with_borrow_mut(|state| {
        if let Some(state) = state {
            state.cycle_calls[usize::from(controlled)][index] += 1;
        }
    });
}

#[crate::tracked(returns(ref), attempt = ReturnOnly, cycle_initial = initial, cycle_fn = recover_a)]
fn a(db: &dyn Database, input: Input) -> Value {
    count_cycle(false, 0);
    let _marker = input.value(db);
    if !input.cyclic(db) {
        return Value::plain(input.value(db) / 10);
    }
    Value::plain(b(db, input).value + 1)
}

#[crate::tracked(returns(ref), attempt = ReturnOnly, cycle_initial = initial)]
fn b(db: &dyn Database, input: Input) -> Value {
    count_cycle(false, 1);
    let _marker = input.value(db);
    Value::plain(a(db, input).value.max(b(db, input).value))
}

fn initial(_db: &dyn Database, _id: Id, _input: Input) -> Value {
    Value::plain(0)
}
fn recover_a(
    _db: &dyn Database,
    _cycle: &Cycle<'_>,
    _last: &Value,
    mut value: Value,
    _input: Input,
) -> Value {
    value.value = value.value.min(3);
    value
}

struct Routes<P: Configuration, S: Configuration, A: Configuration, B: Configuration> {
    parent: Route<'static, P>,
    scalar: Route<'static, S>,
    a: Route<'static, A>,
    b: Route<'static, B>,
}

impl<P, S, A, B, C> ExecutableRouteProvider<'static, 'static, C> for Routes<P, S, A, B>
where
    P: Configuration<DbView = dyn Database, Input<'static> = Input, Output<'static> = Value>,
    S: Configuration<DbView = dyn Database, Input<'static> = Input, Output<'static> = Value>,
    A: Configuration<DbView = dyn Database, Input<'static> = Input, Output<'static> = Value>,
    B: Configuration<DbView = dyn Database, Input<'static> = Input, Output<'static> = Value>,
    C: Configuration<DbView = dyn Database, Input<'static> = Input, Output<'static> = Value>,
{
    // Input conversion constructs a handle; Value equality compares its u32.
    // The fault hook separately admits queued tasks; observations are fixture instrumentation.
    fixture_native_value!(executable, 'static, 'static, C, 1);

    async fn body(
        &'static self,
        context: ProviderContext<'static, 'static, Self>,
        db: &'static dyn Database,
        input: Input,
    ) -> RunResult<Value> {
        if TypeId::of::<C>() == TypeId::of::<P>() {
            let _parent = Marker("parent");
            let value = if input.cyclic(db) {
                context.fetch_ref(&self.a, input.as_id())?.await?.value
            } else {
                context.fetch_ref(&self.scalar, input.as_id())?.await?.value
            };
            Ok(Value::plain(value))
        } else if TypeId::of::<C>() == TypeId::of::<S>() {
            let value = Value::candidate(input.value(db) / 10);
            arm(context.endpoint().inner.clone(), value.tag);
            Ok(value)
        } else if TypeId::of::<C>() == TypeId::of::<A>() {
            count_cycle(true, 0);
            if !input.cyclic(db) {
                let value = Value::candidate(input.value(db) / 10);
                arm(context.endpoint().inner.clone(), value.tag);
                return Ok(value);
            }
            let snapshot = STATE.with_borrow(|state| {
                let state = state.as_ref().unwrap();
                (state.point == Point::Iterate
                    && state.fired
                    && state.fault == Fault::Quiet
                    && state.resumed_headers.is_none())
                .then(|| state.keys.clone())
            });
            if let Some(keys) = snapshot {
                let snapshot = headers(db, &keys);
                STATE.with_borrow_mut(|state| {
                    state.as_mut().unwrap().resumed_headers = Some(snapshot)
                });
            }
            let _marker = input.value(db);
            Ok(Value::candidate(
                context.fetch_ref(&self.b, input.as_id())?.await?.value + 1,
            ))
        } else {
            assert_eq!(TypeId::of::<C>(), TypeId::of::<B>());
            count_cycle(true, 1);
            let _marker = input.value(db);
            let outer = context.fetch_ref(&self.a, input.as_id())?.await?.value;
            let inner = context.fetch_ref(&self.b, input.as_id())?.await?.value;
            Ok(Value::plain(outer.max(inner)))
        }
    }
    fn initial(
        &'static self,
        _context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        _id: Id,
        _input: Input,
    ) -> impl Future<Output = RunResult<Value>> + 'static {
        ready(Ok(Value::plain(0)))
    }
    fn recover<'call>(
        &'static self,
        context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call Value,
        mut value: Value,
        _input: Input,
    ) -> impl Future<Output = RunResult<Value>> + 'call
    where
        'static: 'call,
    {
        if TypeId::of::<C>() == TypeId::of::<A>() {
            value.value = value.value.min(3);
            arm(context.endpoint().inner.clone(), value.tag);
        }
        ready(Ok(value))
    }
}

struct Unlimited;
impl ExecutionAdmission for Unlimited {
    fn admit(&self, _work: ExecutionWork) -> RunResult<()> {
        Ok(())
    }
}
static UNLIMITED: Unlimited = Unlimited;

fn run(db: &'static Db, input: Input) -> RunResult<u32> {
    run_selected(db, input, false)
}

fn run_selected(db: &'static Db, input: Input, cycle_root: bool) -> RunResult<u32> {
    let mut registry = RegistryBuilder::new(db, &UNLIMITED)?;
    let provider: &'static _ = Box::leak(Box::new(Routes {
        parent: registry.reserve(db as &dyn Database, parent::fn_ingredient_(db, db.zalsa()))?,
        scalar: registry.reserve(db as &dyn Database, scalar::fn_ingredient_(db, db.zalsa()))?,
        a: registry.reserve(db as &dyn Database, a::fn_ingredient_(db, db.zalsa()))?,
        b: registry.reserve(db as &dyn Database, b::fn_ingredient_(db, db.zalsa()))?,
    }));
    let binding = registry.provider(provider)?;
    registry.bind_executable(&provider.parent, &binding)?;
    registry.bind_executable(&provider.scalar, &binding)?;
    registry.bind_executable(&provider.a, &binding)?;
    registry.bind_executable(&provider.b, &binding)?;
    registry.seal()?.run(move |endpoint| async move {
        let _root = Marker("root");
        if cycle_root {
            return Ok(endpoint
                .provider(binding)?
                .fetch_ref(&provider.a, input.as_id())?
                .await?
                .value);
        }
        Ok(endpoint
            .provider(binding)?
            .fetch_ref(&provider.parent, input.as_id())?
            .await?
            .value)
    })
}

fn execute(db: &'static Db, input: Input, fault: Fault, expected: u32) {
    let identity = STATE.with_borrow(|state| state.as_ref().unwrap().identity.clone());
    let stamp = Stamp::current(db);
    let result = catch_unwind(AssertUnwindSafe(|| {
        try_with_attempt(db, 100_000, || {
            let result = run(db, input);
            assert_eq!(
                result,
                if fault == Fault::Quiet {
                    Ok(expected)
                } else {
                    Err(RunError::Refused(Incomplete::Interrupted))
                }
            );
            result
        })
    }));
    match fault {
        Fault::Quiet => assert_eq!(result.unwrap(), Ok(AttemptOutcome::Complete(Ok(expected)))),
        Fault::Refuse => assert_eq!(
            result.unwrap(),
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        ),
        Fault::Panic => {
            let payload = result.expect_err("the completion panic escapes the driver");
            let marker = payload
                .downcast_ref::<PanicMarker>()
                .expect("original panic type");
            assert!(Arc::ptr_eq(&marker.0, &identity));
        }
        Fault::Local => assert!(matches!(
            result.unwrap_err().downcast_ref::<crate::Cancelled>(),
            Some(crate::Cancelled::Local)
        )),
    }
    assert_eq!(Stamp::current(db), stamp);
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    STATE.with_borrow(|state| {
        let state = state.as_ref().unwrap();
        assert!(state.fired);
        assert!(state.endpoint.is_none());
    });
}

fn assert_retained(parent: DatabaseKeyIndex) {
    STATE.with_borrow(|state| {
        let state = state.as_ref().unwrap();
        assert_eq!(
            state
                .observations
                .iter()
                .map(|observation| observation.stage)
                .collect::<Vec<_>>(),
            ["callout", "child", "candidate", "parent", "root"]
        );
        for observation in &state.observations {
            let root = observation.stage == "root";
            let parent_drop = observation.stage == "parent";
            assert_eq!(
                observation.frame,
                (!root).then_some(parent),
                "{observation:?}"
            );
            assert_eq!(
                observation.query_depth,
                usize::from(!root),
                "{observation:?}"
            );
            assert_eq!(
                observation.operation_depth,
                if root {
                    0
                } else if parent_drop {
                    1
                } else {
                    2
                },
                "{observation:?}"
            );
            assert_eq!(observation.claim, !root && !parent_drop, "{observation:?}");
            assert_eq!(
                observation.reason,
                (state.fault == Fault::Refuse && observation.stage != "callout")
                    .then_some(Incomplete::Interrupted),
                "{observation:?}"
            );
            assert_eq!(
                observation.panicking,
                state.fault == Fault::Panic && observation.stage != "callout",
                "{observation:?}"
            );
        }
        assert_eq!(state.observations[0].headers, state.observations[1].headers);
    });
}

#[test]
fn backdating_equality_retains_popped_owner_and_candidate() {
    let mut baseline = database();
    let input = Input::new(&baseline, 70, false);
    assert_eq!(parent(&baseline, input).value, 7);
    input.set_value(&mut baseline).to(71);
    assert_eq!(parent(&baseline, input).value, 7);
    for fault in [Fault::Quiet, Fault::Refuse, Fault::Panic] {
        ORDINARY_SCALARS.set(0);
        let mut db = database();
        let input = Input::new(&db, 70, false);
        let old_value = std::ptr::from_ref(scalar(&db, input)) as usize;
        input.set_value(&mut db).to(71);
        let db: &'static Db = Box::leak(Box::new(db));
        let key = scalar::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id());
        let parent_key = parent::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id());
        let old_headers = headers(db, &[key]);
        let _reset = begin(Point::Backdate, fault, vec![key], old_value);
        execute(db, input, fault, 7);
        STATE.with_borrow(|state| assert_eq!(state.as_ref().unwrap().comparison, Some((7, 7))));
        if fault == Fault::Quiet {
            let accepted = headers(db, &[key]);
            assert_eq!(accepted[0].changed, old_headers[0].changed);
            assert_eq!(accepted[0].verified, db.zalsa().current_revision());
            assert_ne!(accepted[0].identity, old_headers[0].identity);
        } else {
            assert_retained(parent_key);
            assert_eq!(headers(db, &[key]), old_headers);
            assert!(
                db.zalsa()
                    .lookup_ingredient(parent_key.ingredient_index())
                    .as_function()
                    .unwrap()
                    .memo(db.zalsa(), parent_key.key_index())
                    .is_none()
            );
        }
        let stamp = Stamp::current(db);
        assert_eq!([parent(db, input).value, parent(db, input).value], [7, 7]);
        assert_eq!(
            ORDINARY_SCALARS.get(),
            if fault == Fault::Quiet { 1 } else { 2 }
        );
        assert_eq!(Stamp::current(db), stamp);
    }
}

fn cycle_control(point: Point, fault: Fault) {
    let mut db = database();
    let input = Input::new(&db, 0, true);
    let old_value = if point == Point::FinalizeBackdate {
        let address = std::ptr::from_ref(a(&db, input)) as usize;
        assert_eq!(b(&db, input).value, 3);
        input.set_value(&mut db).to(1);
        address
    } else {
        0
    };
    let db: &'static Db = Box::leak(Box::new(db));
    let a_ingredient = a::fn_ingredient_(db, db.zalsa());
    let a_key = a_ingredient.database_key_index(input.as_id());
    let b_key = b::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id());
    let parent_key = parent::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id());
    let _reset = begin(point, fault, vec![a_key, b_key], old_value);
    execute(db, input, fault, 3);
    STATE.with_borrow(|state| {
        let state = state.as_ref().unwrap();
        let at_callout = &state.observations[0].headers;
        assert!(at_callout.iter().all(|header| header.provisional));
        if point == Point::Iterate {
            let next = state.next_iteration.unwrap();
            assert!(at_callout.iter().all(|header| {
                header.iteration.iteration() < next
                    && header
                        .heads
                        .iter()
                        .all(|(_, stamp)| stamp.iteration() < next)
            }));
            if fault == Fault::Quiet {
                let resumed = state
                    .resumed_headers
                    .as_ref()
                    .expect("accepted iteration entered its next body");
                assert!(
                    resumed
                        .iter()
                        .all(|header| header.iteration.iteration() == next)
                );
            }
        } else {
            assert_eq!(state.comparison, Some((3, 3)));
        }
    });
    if fault != Fault::Quiet {
        assert_retained(parent_key);
        let memo = a_ingredient
            .get_memo_from_table_for(
                db.zalsa(),
                input.as_id(),
                a_ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
            )
            .unwrap();
        assert!(memo.header.may_be_provisional());
        assert_eq!(memo.value().is_none(), fault == Fault::Panic);
        assert!(headers(db, &[b_key])[0].provisional);
        if fault == Fault::Refuse {
            assert!(!memo.header.can_seed_attempt(db.zalsa()));
        }
    }
    if fault != Fault::Panic {
        let stamp = Stamp::current(db);
        assert_eq!(
            [
                (a(db, input).value, b(db, input).value),
                (a(db, input).value, b(db, input).value)
            ],
            [(3, 3), (3, 3)]
        );
        assert_eq!(Stamp::current(db), stamp);
    }
}

#[test]
fn will_iterate_rejection_retains_popped_owner_and_headers() {
    let baseline = database();
    let input = Input::new(&baseline, 0, true);
    assert_eq!(
        (a(&baseline, input).value, b(&baseline, input).value),
        (3, 3)
    );
    for fault in [Fault::Quiet, Fault::Refuse, Fault::Panic] {
        cycle_control(Point::Iterate, fault);
    }
}

#[test]
fn converged_backdating_rejection_cannot_finalize_nested_headers() {
    for fault in [Fault::Quiet, Fault::Refuse] {
        cycle_control(Point::FinalizeBackdate, fault);
    }
}

fn local_during_backdating(point: Point, controlled: bool) {
    let cyclic = point == Point::FinalizeBackdate;
    let wanted = if cyclic { 3 } else { 7 };
    let mut db = database();
    let input = Input::new(&db, if cyclic { 0 } else { 70 }, cyclic);
    assert_eq!(a(&db, input).value, wanted);
    if cyclic {
        assert_eq!(b(&db, input).value, wanted);
    }
    let old_value = std::ptr::from_ref(a(&db, input)) as usize;
    input.set_value(&mut db).to(if cyclic { 1 } else { 71 });
    let db: &'static Db = Box::leak(Box::new(db));
    let a_key = a::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id());
    let mut keys = vec![a_key];
    if cyclic {
        keys.push(b::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()));
    }
    let _reset = begin(point, Fault::Local, keys.clone(), old_value);
    STATE.with_borrow_mut(|state| state.as_mut().unwrap().native_local = !controlled);
    let stamp = Stamp::current(db);
    if controlled {
        let result = catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(db, 100_000, || run_selected(db, input, !cyclic))
        }));
        assert!(matches!(
            result.unwrap_err().downcast_ref::<crate::Cancelled>(),
            Some(crate::Cancelled::Local)
        ));
    } else {
        assert_eq!(
            crate::attach(db, || {
                let value = a(db, input).value;
                observe("native.return");
                value
            }),
            wanted
        );
    }
    assert_eq!(Stamp::current(db), stamp);
    assert!(attempt_probe::current().is_none());
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    assert!(crate::with_attached_database(|_| ()).is_none());
    let accepted = headers(db, &keys);
    assert!(
        accepted
            .iter()
            .all(|header| !header.provisional && header.verified == db.zalsa().current_revision())
    );
    STATE.with_borrow(|state| {
        let state = state.as_ref().unwrap();
        assert!(state.fired && state.endpoint.is_none());
        assert_eq!(state.comparison, Some((wanted, wanted)));
        let checked = state
            .observations
            .iter()
            .position(|event| event.stage == "local.checked")
            .unwrap();
        let local = &state.observations[checked];
        assert!(local.requested && !local.enabled && local.claim && !local.panicking);
        let delivered = if controlled {
            state
                .observations
                .iter()
                .position(|event| event.stage == "native.check" && event.requested && event.enabled)
                .unwrap()
        } else {
            state
                .observations
                .iter()
                .position(|event| event.stage == "native.return")
                .unwrap()
        };
        assert!(checked < delivered);
        let retired = &state.observations[delivered];
        assert!(retired.requested && retired.enabled && !retired.claim);
        assert_eq!(retired.headers, accepted);
        for event in &state.observations[checked..delivered] {
            if event.stage == "native.check" {
                assert!(event.requested && !event.enabled);
            }
        }
        assert!(
            !state
                .observations
                .iter()
                .any(|event| event.stage == "child")
        );
    });
    db.zalsa_local().uncancel();
    let calls = STATE.with_borrow(|state| state.as_ref().unwrap().cycle_calls);
    for _ in 0..2 {
        assert_eq!(
            try_with_attempt(db, 100_000, || a(db, input).value),
            Ok(AttemptOutcome::Complete(wanted))
        );
        if cyclic {
            assert_eq!(b(db, input).value, wanted);
        }
        assert_eq!(headers(db, &keys), accepted);
        assert_eq!(
            STATE.with_borrow(|state| state.as_ref().unwrap().cycle_calls),
            calls
        );
        assert_eq!(Stamp::current(db), stamp);
    }
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn local_backdating_keeps_finalize_and_cycle_return_masked() {
    for point in [Point::FinalizeBackdate, Point::ReturnBackdate] {
        for controlled in [false, true] {
            local_during_backdating(point, controlled);
        }
    }
}

#[test]
fn cycle_return_backdating_panic_keeps_the_prior_memo() {
    let mut db = database();
    let input = Input::new(&db, 70, false);
    let old_value = std::ptr::from_ref(a(&db, input)) as usize;
    input.set_value(&mut db).to(71);
    let db: &'static Db = Box::leak(Box::new(db));
    let key = a::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id());
    let before = headers(db, &[key]);
    let stamp = Stamp::current(db);
    let _reset = begin(Point::ReturnBackdate, Fault::Panic, vec![key], old_value);
    let identity = STATE.with_borrow(|state| state.as_ref().unwrap().identity.clone());
    let payload = catch_unwind(AssertUnwindSafe(|| {
        try_with_attempt(db, 100_000, || run_selected(db, input, true))
    }))
    .unwrap_err();
    assert!(Arc::ptr_eq(
        &payload.downcast_ref::<PanicMarker>().unwrap().0,
        &identity
    ));
    assert_eq!(headers(db, &[key]), before);
    let ingredient = a::fn_ingredient_(db, db.zalsa());
    let memo = ingredient
        .get_memo_from_table_for(
            db.zalsa(),
            input.as_id(),
            ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
        )
        .unwrap();
    assert_eq!(memo.value().unwrap().value, 7);
    STATE.with_borrow(|state| {
        let state = state.as_ref().unwrap();
        assert!(state.fired && state.endpoint.is_none());
        assert_eq!(
            state
                .observations
                .iter()
                .map(|event| event.stage)
                .collect::<Vec<_>>(),
            ["callout", "child", "candidate", "root"]
        );
        assert_eq!(state.observations[0].headers, state.observations[1].headers);
        for event in &state.observations {
            assert_eq!(event.frame, None);
            assert_eq!(event.query_depth, 0);
            assert_eq!(event.operation_depth, usize::from(event.stage != "root"));
            assert_eq!(event.claim, event.stage != "root");
            assert_eq!(event.reason, None);
            assert_eq!(event.panicking, event.stage != "callout");
        }
    });
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    assert!(attempt_probe::current().is_none());
    assert_eq!(Stamp::current(db), stamp);
    assert_eq!(
        try_with_attempt(db, 100_000, || a(db, input).value),
        Ok(AttemptOutcome::Complete(7))
    );
    let accepted = headers(db, &[key]);
    let calls = STATE.with_borrow(|state| state.as_ref().unwrap().cycle_calls);
    assert_eq!(a(db, input).value, 7);
    assert_eq!(headers(db, &[key]), accepted);
    assert_eq!(
        STATE.with_borrow(|state| state.as_ref().unwrap().cycle_calls),
        calls
    );
    assert_eq!(Stamp::current(db), stamp);
}

#[test]
fn completion_state_layouts() {
    fn record<C: Configuration>(_: &crate::function::IngredientImpl<C>) {
        fn layout<T>(name: &str) {
            eprintln!(
                "COMPLETION_STATE_LAYOUT {name} size={} align={}",
                size_of::<T>(),
                align_of::<T>()
            );
        }
        layout::<super::super::super::CompletionDisposition<'static, C>>("CompletionDisposition");
        layout::<super::super::super::CompletionPreparation<'static, C>>("CompletionPreparation");
        layout::<super::super::super::PreparedQueryCommit<'static, C>>("PreparedQueryCommit");
        layout::<super::super::super::ExecutionStep<'static, C>>("ExecutionStep");
    }
    let db = database();
    record(a::fn_ingredient_(&db, db.zalsa()));
}

#[crate::tracked]
struct Syntax<'db> {
    #[returns(copy)]
    value: u32,
}

#[crate::tracked(returns(copy), specify)]
fn specified<'db>(_db: &'db dyn Database, _syntax: Syntax<'db>) -> u32 {
    5
}

#[crate::tracked(returns(copy), attempt = CompleteOnly)]
fn producer(db: &dyn Database) -> Syntax<'_> {
    let syntax = Syntax::new(db, 7);
    specified::specify(db, syntax, 11);
    syntax
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn output_consumer(_db: &dyn Database, _input: Input) -> u32 {
    0
}

struct OutputProvider {
    syntax: Syntax<'static>,
    specify: bool,
}

impl<C> ExecutableRouteProvider<'static, 'static, C> for OutputProvider
where
    C: Configuration<DbView = dyn Database, Input<'static> = Input, Output<'static> = u32>,
{
    // Input conversion constructs a handle; output equality compares u32.
    fixture_native_value!(executable, 'static, 'static, C, 1);

    async fn body(
        &'static self,
        _context: ProviderContext<'static, 'static, Self>,
        db: &'static dyn Database,
        _input: Input,
    ) -> RunResult<u32> {
        if self.specify {
            specified::specify(db, self.syntax, 99);
        } else {
            let _syntax = Syntax::new(db, 9);
        }
        Ok(0)
    }
    fn initial(
        &'static self,
        _context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        _id: Id,
        _input: Input,
    ) -> impl Future<Output = RunResult<u32>> + 'static {
        ready(Err(RunError::Contract(
            "acyclic contract fixture requested initial",
        )))
    }
    fn recover<'call>(
        &'static self,
        _context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call u32,
        _value: u32,
        _input: Input,
    ) -> impl Future<Output = RunResult<u32>> + 'call
    where
        'static: 'call,
    {
        ready(Err(RunError::Contract(
            "acyclic contract fixture requested recovery",
        )))
    }
}

fn assert_output_contract(payload: &(dyn Any + Send)) {
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .expect("string contract panic");
    assert!(
        message.contains("return-only query attempted to create a tracked output"),
        "{message}"
    );
}

#[test]
fn registered_return_only_outputs_are_rejected_before_completion() {
    for specify in [false, true] {
        let db: &'static Db = Box::leak(Box::new(database()));
        let syntax = producer(db);
        assert_eq!(specified(db, syntax), 11);
        let input = Input::new(db, 0, false);
        let ingredient = output_consumer::fn_ingredient_(db, db.zalsa());
        let provider: &'static _ = Box::leak(Box::new(OutputProvider { syntax, specify }));
        let payload = catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(db, 100_000, || -> RunResult<u32> {
                let mut registry = RegistryBuilder::new(db, &UNLIMITED)?;
                let route = registry.reserve(db as &dyn Database, ingredient)?;
                let binding = registry.provider(provider)?;
                registry.bind_executable(&route, &binding)?;
                registry.seal()?.run(move |endpoint| async move {
                    Ok(*endpoint
                        .provider(binding)?
                        .fetch_ref(&route, input.as_id())?
                        .await?)
                })
            })
        }))
        .expect_err("the ReturnOnly output policy rejects the provider action");
        assert_output_contract(payload.as_ref());
        assert_eq!(specified(db, syntax), 11);
        assert!(
            ingredient
                .get_memo_from_table_for(
                    db.zalsa(),
                    input.as_id(),
                    ingredient.memo_ingredient_index(db.zalsa(), input.as_id())
                )
                .is_none()
        );
        assert!(matches!(
            ingredient
                .sync_table
                .peek_claim(db.zalsa(), input.as_id(), Reentrancy::Deny),
            ClaimResult::Claimed(())
        ));
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(db.zalsa_local().active_query().is_none());
    }
}
