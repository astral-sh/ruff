use std::any::TypeId;
use std::cell::RefCell;
use std::future::{Future, poll_fn, ready};
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::sync::Arc;
use std::task::Poll;

use super::super::registration::{
    ExecutableRouteProvider, NativeValueOperation, NativeValueQuote, ProviderContext,
    RegistryBuilder, RetainedInput, Route,
};
use super::super::{Endpoint, ExecutionAdmission, ExecutionWork, RunError, RunResult};
use crate::attempt_probe::{self, AttemptOutcome, Incomplete, try_with_attempt};
use crate::function::{ClaimResult, Configuration, IngredientImpl, Reentrancy};
use crate::plumbing::AsId;
use crate::prepared_source_probe::Stamp;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseKeyIndex, EventKind, Id};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Point {
    Start,
    InputClone,
    Replacement,
    Equality,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fault {
    Quiet,
    Refuse,
    Panic,
}

#[derive(Debug)]
struct Observation {
    stage: &'static str,
    frame: Option<DatabaseKeyIndex>,
    claim: bool,
    depth: usize,
    reason: Option<Incomplete>,
    panicking: bool,
}

struct State {
    point: Point,
    fault: Fault,
    target: Option<DatabaseKeyIndex>,
    endpoint: Option<Endpoint<'static, 'static>>,
    identity: Arc<()>,
    fired: bool,
    body_returned: bool,
    initials: usize,
    recoveries: usize,
    child_bodies: usize,
    observations: Vec<Observation>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

struct ClearState;

impl Drop for ClearState {
    fn drop(&mut self) {
        STATE.with_borrow_mut(|state| *state = None);
    }
}

fn begin(point: Point, fault: Fault, target: DatabaseKeyIndex) -> ClearState {
    STATE.with_borrow_mut(|slot| {
        assert!(slot.is_none());
        *slot = Some(State {
            point,
            fault,
            target: Some(target),
            endpoint: None,
            identity: Arc::new(()),
            fired: false,
            body_returned: false,
            initials: 0,
            recoveries: 0,
            child_bodies: 0,
            observations: Vec::new(),
        });
    });
    ClearState
}

fn record(stage: &'static str) {
    let target = STATE.with_borrow(|state| state.as_ref().and_then(|state| state.target));
    let Some(target) = target else { return };
    let (frame, claim) = crate::with_attached_database(|db| {
        let claim = db
            .zalsa()
            .lookup_ingredient(target.ingredient_index())
            .as_function()
            .is_some_and(|function| {
                matches!(
                    function.sync_table().peek_claim(
                        db.zalsa(),
                        target.key_index(),
                        Reentrancy::Deny,
                    ),
                    ClaimResult::Cycle { .. }
                )
            });
        (db.zalsa_local().active_query().map(|(key, _)| key), claim)
    })
    .expect("callout cleanup retains the attached database");
    STATE.with_borrow_mut(|state| {
        state.as_mut().unwrap().observations.push(Observation {
            stage,
            frame,
            claim,
            depth: attempt_probe::stack_depths().0,
            reason: attempt_probe::current().and_then(|support| support.reason()),
            panicking: crate::sync::thread::panicking(),
        });
    });
}

struct Marker(&'static str);

impl Drop for Marker {
    fn drop(&mut self) {
        record(self.0);
    }
}

#[derive(Debug)]
struct PanicMarker(Arc<()>);

fn arm(point: Point, endpoint: Endpoint<'static, 'static>) {
    STATE.with_borrow_mut(|state| {
        let state = state.as_mut().unwrap();
        if state.point == point && state.fault != Fault::Quiet && !state.fired {
            assert!(state.endpoint.replace(endpoint).is_none());
        }
    });
}

fn fire(point: Point) {
    let hook = STATE.with_borrow_mut(|state| {
        let state = state.as_mut()?;
        if state.point != point {
            return None;
        }
        let endpoint = state.endpoint.take()?;
        assert!(!state.fired);
        state.fired = true;
        Some((endpoint, state.fault, state.identity.clone()))
    });
    let Some((endpoint, fault, identity)) = hook else {
        return;
    };
    record("callout");
    // Disarm before demand performs its own cancellation checks.
    let child = Marker("child");
    let _reply = endpoint
        .demand(move || {
            poll_fn(move |_| -> Poll<RunResult<()>> {
                let _child = &child;
                panic!("a child queued by a rejected canonical callout must not run");
            })
        })
        .expect("the canonical callout retains the active task poll");
    match fault {
        Fault::Refuse => {
            attempt_probe::report_incomplete(endpoint.context.db, Incomplete::Interrupted);
        }
        Fault::Panic => panic_any(PanicMarker(identity)),
        Fault::Quiet => panic!("a quiet fixture armed a failure hook"),
    }
}

#[crate::db]
#[derive(Clone)]
struct HookDb {
    storage: crate::Storage<Self>,
}

#[crate::db]
impl Database for HookDb {}

fn database() -> &'static HookDb {
    // Event hooks and value traits cannot borrow the provider endpoint. A dedicated static
    // database gives these test-only TLS endpoints their real lifetime without a cast.
    Box::leak(Box::new(HookDb {
        storage: crate::Storage::new(Some(Box::new(|event| {
            if let EventKind::WillExecute { database_key } = event.kind
                && STATE.with_borrow(|state| {
                    state.as_ref().and_then(|state| state.target) == Some(database_key)
                })
            {
                fire(Point::Start);
            }
        }))),
    }))
}

struct Unlimited;

impl ExecutionAdmission for Unlimited {
    fn admit(&self, _work: ExecutionWork) -> RunResult<()> {
        Ok(())
    }
}

static UNLIMITED: Unlimited = Unlimited;

fn execute(db: &'static HookDb, fault: Fault, expected: u32, run: impl FnOnce() -> RunResult<u32>) {
    let identity = STATE.with_borrow(|state| state.as_ref().unwrap().identity.clone());
    let stamp = Stamp::current(db);
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        try_with_attempt(db, 100_000, || {
            let result = run();
            assert_eq!(
                result,
                if fault == Fault::Quiet {
                    Ok(expected)
                } else {
                    Err(RunError::Refused(Incomplete::Interrupted))
                },
            );
            result
        })
    }));
    match fault {
        Fault::Quiet => assert_eq!(outcome.unwrap(), Ok(AttemptOutcome::Complete(Ok(expected))),),
        Fault::Refuse => assert_eq!(
            outcome.unwrap(),
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted)),
        ),
        Fault::Panic => {
            let payload = outcome.expect_err("the original callout panic escapes the driver");
            let marker = payload
                .downcast_ref::<PanicMarker>()
                .expect("original panic type");
            assert!(Arc::ptr_eq(&marker.0, &identity));
        }
    }
    assert_eq!(Stamp::current(db), stamp);
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    STATE.with_borrow(|state| {
        let state = state.as_ref().unwrap();
        assert_eq!(state.fired, fault != Fault::Quiet);
        assert!(state.endpoint.is_none());
    });
}

fn assert_order(frame: DatabaseKeyIndex, depth: usize, stages: &[&str]) {
    STATE.with_borrow(|state| {
        let state = state.as_ref().unwrap();
        assert_eq!(
            state
                .observations
                .iter()
                .map(|event| event.stage)
                .collect::<Vec<_>>(),
            stages,
        );
        for event in &state.observations {
            let root = event.stage == "root";
            let parent = event.stage == "parent";
            assert_eq!(event.frame, (!root).then_some(frame), "{event:?}");
            assert_eq!(event.claim, !root && !parent, "{event:?}");
            assert_eq!(
                event.depth,
                if root {
                    0
                } else if parent {
                    depth - 1
                } else {
                    depth
                },
                "{event:?}"
            );
            let before_failure = event.stage == "callout"
                || (state.point == Point::Replacement && event.stage == "computed");
            assert_eq!(
                event.reason,
                (state.fault == Fault::Refuse && !before_failure)
                    .then_some(Incomplete::Interrupted),
                "{event:?}",
            );
            assert_eq!(
                event.panicking,
                state.fault == Fault::Panic && !before_failure,
                "{event:?}"
            );
        }
    });
}

#[crate::input]
struct Input {
    #[returns(copy)]
    value: u32,
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn child(db: &dyn Database, input: Input) -> u32 {
    input.value(db)
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn parent(db: &dyn Database, input: Input) -> u32 {
    child(db, input) + 1
}

struct StartProvider<A: Configuration, B: Configuration> {
    child: Route<'static, B>,
    parent: Route<'static, A>,
}

impl<A, B, C> ExecutableRouteProvider<'static, 'static, C> for StartProvider<A, B>
where
    A: Configuration<DbView = dyn Database, Input<'static> = Input, Output<'static> = u32>,
    B: Configuration<DbView = dyn Database, Input<'static> = Input, Output<'static> = u32>,
    C: Configuration<DbView = dyn Database, Input<'static> = Input, Output<'static> = u32>,
{
    // Input conversion constructs a handle; output equality compares u32.
    fixture_native_value!(executable, 'static, 'static, C, 1);

    async fn body(
        &'static self,
        context: ProviderContext<'static, 'static, Self>,
        db: &'static dyn Database,
        input: Input,
    ) -> RunResult<u32> {
        if TypeId::of::<C>() == TypeId::of::<A>() {
            let _parent = Marker("parent");
            arm(Point::Start, context.endpoint().inner.clone());
            Ok(*context.fetch_ref(&self.child, input.as_id())?.await? + 1)
        } else {
            assert_eq!(TypeId::of::<C>(), TypeId::of::<B>());
            STATE.with_borrow_mut(|state| state.as_mut().unwrap().child_bodies += 1);
            Ok(input.value(db))
        }
    }

    fn initial(
        &'static self,
        _context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        _id: Id,
        _input: Input,
    ) -> impl Future<Output = RunResult<u32>> + 'static {
        ready(Err(RunError::Contract(
            "acyclic start fixture requested initial",
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
            "acyclic start fixture requested recovery",
        )))
    }
}

#[test]
fn start_event_retains_claim_and_original_caller() {
    let baseline = database();
    assert_eq!(parent(baseline, Input::new(baseline, 7)), 8);
    for fault in [Fault::Quiet, Fault::Refuse, Fault::Panic] {
        let db = database();
        let input = Input::new(db, 7);
        let child_ingredient = child::fn_ingredient_(db, db.zalsa());
        let parent_ingredient = parent::fn_ingredient_(db, db.zalsa());
        let child_key = child_ingredient.database_key_index(input.as_id());
        let parent_key = parent_ingredient.database_key_index(input.as_id());
        let _clear = begin(Point::Start, fault, child_key);
        execute(db, fault, 8, || {
            let mut registry = RegistryBuilder::new(db, &UNLIMITED)?;
            let child = registry.reserve(db as &dyn Database, child_ingredient)?;
            let parent = registry.reserve(db as &dyn Database, parent_ingredient)?;
            let provider: &'static _ = Box::leak(Box::new(StartProvider { child, parent }));
            let binding = registry.provider(provider)?;
            registry.bind_executable(&provider.child, &binding)?;
            registry.bind_executable(&provider.parent, &binding)?;
            registry.seal()?.run(move |endpoint| async move {
                let _root = Marker("root");
                Ok(*endpoint
                    .provider(binding)?
                    .fetch_ref(&provider.parent, input.as_id())?
                    .await?)
            })
        });
        STATE.with_borrow(|state| {
            assert_eq!(
                state.as_ref().unwrap().child_bodies,
                usize::from(fault == Fault::Quiet)
            );
        });
        if fault != Fault::Quiet {
            assert_order(parent_key, 2, &["callout", "child", "parent", "root"]);
            assert!(
                child_ingredient
                    .get_memo_from_table_for(
                        db.zalsa(),
                        input.as_id(),
                        child_ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
                    )
                    .is_none()
            );
            assert_eq!([parent(db, input), parent(db, input)], [8, 8]);
        }
    }
}

#[derive(Debug, Eq, Hash, PartialEq)]
struct Key(u32);

impl Clone for Key {
    fn clone(&self) -> Self {
        fire(Point::InputClone);
        Self(self.0)
    }
}

#[derive(Debug)]
struct Value {
    value: u32,
    drop_stage: Option<&'static str>,
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        fire(Point::Equality);
        self.value == other.value
    }
}

impl Eq for Value {}

impl Drop for Value {
    fn drop(&mut self) {
        if let Some(stage) = self.drop_stage {
            record(stage);
            if stage == "computed" {
                fire(Point::Replacement);
            }
        }
    }
}

#[crate::tracked(returns(ref), attempt = ReturnOnly, cycle_initial = initial, cycle_fn = recover)]
fn fixpoint(db: &dyn Database, key: Key, unit: ()) -> Value {
    Value {
        value: (fixpoint(db, key, unit).value + 1).min(3),
        drop_stage: None,
    }
}

#[crate::tracked(returns(ref), attempt = ReturnOnly, cycle_result = initial)]
fn fallback(db: &dyn Database, key: Key, unit: ()) -> Value {
    Value {
        value: fallback(db, key, unit).value + 1,
        drop_stage: None,
    }
}

fn initial(_db: &dyn Database, _id: Id, _key: Key, _unit: ()) -> Value {
    Value {
        value: 0,
        drop_stage: None,
    }
}

fn recover(
    _db: &dyn Database,
    _cycle: &Cycle<'_>,
    _last: &Value,
    value: Value,
    _key: Key,
    _unit: (),
) -> Value {
    value
}

struct CycleProvider<C: Configuration> {
    route: Route<'static, C>,
    id: Id,
}

impl<C> ExecutableRouteProvider<'static, 'static, C> for CycleProvider<C>
where
    C: Configuration<DbView = dyn Database, Input<'static> = (Key, ()), Output<'static> = Value>,
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
        // Key clones one u32; Value equality compares one u32. Fault hooks enqueue separately
        // admitted children, and observation records belong to the fixture instrumentation.
        let work = match operation {
            NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => 2,
            NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => {
                return Err(RunError::Contract("callout key requires retained tuple"));
            }
            NativeValueOperation::Comparison { .. } => 1,
        };
        Ok(NativeValueQuote {
            work,
            requested_bytes: 0,
            cleanup_work: 0,
        })
    }

    async fn body(
        &'static self,
        context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        _input: (Key, ()),
    ) -> RunResult<Value> {
        let value = context.fetch_ref(&self.route, self.id)?.await?.value;
        STATE.with_borrow_mut(|state| state.as_mut().unwrap().body_returned = true);
        arm(Point::InputClone, context.endpoint().inner.clone());
        Ok(Value {
            value: (value + 1).min(3),
            drop_stage: Some("computed"),
        })
    }

    fn initial(
        &'static self,
        context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        _id: Id,
        _input: (Key, ()),
    ) -> impl Future<Output = RunResult<Value>> + 'static {
        let replacement = STATE.with_borrow_mut(|state| {
            let state = state.as_mut().unwrap();
            state.initials += 1;
            state.body_returned
        });
        if replacement {
            arm(Point::Replacement, context.endpoint().inner.clone());
        }
        ready(Ok(Value {
            value: 0,
            drop_stage: replacement.then_some("replacement"),
        }))
    }

    fn recover<'call>(
        &'static self,
        context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call Value,
        value: Value,
        _input: (Key, ()),
    ) -> impl Future<Output = RunResult<Value>> + 'call
    where
        'static: 'call,
    {
        STATE.with_borrow_mut(|state| state.as_mut().unwrap().recoveries += 1);
        arm(Point::Equality, context.endpoint().inner.clone());
        ready(Ok(value))
    }
}

fn cycle_control<C>(
    db: &'static HookDb,
    ingredient: &'static IngredientImpl<C>,
    id: Id,
    point: Point,
    fault: Fault,
    expected: u32,
    ordinary: impl Fn() -> u32,
) where
    C: Configuration<DbView = dyn Database, Input<'static> = (Key, ()), Output<'static> = Value>,
{
    let key = ingredient.database_key_index(id);
    let _clear = begin(point, fault, key);
    execute(db, fault, expected, || {
        let mut registry = RegistryBuilder::new(db, &UNLIMITED)?;
        let route = registry.reserve(db as &dyn Database, ingredient)?;
        let provider: &'static _ = Box::leak(Box::new(CycleProvider { route, id }));
        let binding = registry.provider(provider)?;
        registry.bind_executable(&provider.route, &binding)?;
        registry.seal()?.run(move |endpoint| async move {
            let _root = Marker("root");
            Ok(endpoint
                .provider(binding)?
                .fetch_ref(&provider.route, id)?
                .await?
                .value)
        })
    });
    if fault == Fault::Quiet {
        assert_eq!(ordinary(), expected);
        return;
    }
    let stages = if point == Point::Replacement {
        &["computed", "callout", "child", "replacement", "root"][..]
    } else {
        &["callout", "child", "computed", "root"][..]
    };
    assert_order(key, 1, stages);
    STATE.with_borrow(|state| {
        let state = state.as_ref().unwrap();
        assert_eq!(
            state.initials,
            if point == Point::Replacement { 2 } else { 1 }
        );
        assert_eq!(state.recoveries, usize::from(point == Point::Equality));
    });
    let memo = ingredient
        .get_memo_from_table_for(
            db.zalsa(),
            id,
            ingredient.memo_ingredient_index(db.zalsa(), id),
        )
        .expect("the self-read installed a provisional seed");
    assert!(memo.header.may_be_provisional());
    assert_eq!(memo.value().is_none(), fault == Fault::Panic);
    if fault == Fault::Refuse {
        assert!(!memo.header.can_seed_attempt(db.zalsa()));
        let stamp = Stamp::current(db);
        assert_eq!([ordinary(), ordinary()], [expected, expected]);
        assert_eq!(Stamp::current(db), stamp);
    }
}

fn fixpoint_control(point: Point) {
    let baseline = database();
    assert_eq!(fixpoint(baseline, Key(0), ()).value, 3);
    for fault in [Fault::Quiet, Fault::Refuse, Fault::Panic] {
        let db = database();
        let ingredient = fixpoint::fn_ingredient_(db, db.zalsa());
        // Two arguments force generated tuple interning. Only id_to_input cloning is armed;
        // argument interning is completed beforehand.
        let id = fixpoint::intern_ingredient_(db.zalsa()).intern_id(
            db.zalsa(),
            db.zalsa_local(),
            (Key(0), ()),
            |_, key| key,
        );
        cycle_control(db, ingredient, id, point, fault, 3, || {
            fixpoint(db, Key(0), ()).value
        });
    }
}

#[test]
fn recovery_input_clone_failure_retains_computed_operand() {
    fixpoint_control(Point::InputClone);
}

#[test]
fn accepted_initial_replacement_drop_keeps_new_value_owned() {
    let baseline = database();
    assert_eq!(fallback(baseline, Key(0), ()).value, 0);
    for fault in [Fault::Quiet, Fault::Refuse, Fault::Panic] {
        let db = database();
        let ingredient = fallback::fn_ingredient_(db, db.zalsa());
        let id = fallback::intern_ingredient_(db.zalsa()).intern_id(
            db.zalsa(),
            db.zalsa_local(),
            (Key(0), ()),
            |_, key| key,
        );
        cycle_control(db, ingredient, id, Point::Replacement, fault, 0, || {
            fallback(db, Key(0), ()).value
        });
    }
}

#[test]
fn cycle_equality_failure_cannot_finalize_candidate() {
    fixpoint_control(Point::Equality);
}
