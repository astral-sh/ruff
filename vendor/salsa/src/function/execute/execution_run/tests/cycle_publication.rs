use std::any::TypeId;
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
use crate::function::{ClaimResult, Configuration, FunctionIngredient, IngredientImpl, Reentrancy};
use crate::plumbing::AsId;
use crate::prepared_source_probe::Stamp;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseKeyIndex, EventKind, Id, Revision};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Site {
    Finalized,
    Storage,
    Work,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    Observe,
    Refuse,
    ChildOnly,
    Panic,
    Local,
    PendingWrite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RequestedLocal {
    Return,
    Refuse,
    Child,
    Panic,
}

#[derive(Debug, Eq, PartialEq)]
struct MemoState {
    identity: usize,
    value: Option<u32>,
    final_value: bool,
    verified: Revision,
    iteration: IterationStamp,
    heads: Vec<(DatabaseKeyIndex, IterationStamp)>,
}

fn memo_state<C>(db: &dyn Database, ingredient: &IngredientImpl<C>, id: Id) -> MemoState
where
    C: for<'db> Configuration<DbView = dyn Database, Input<'db> = Input, Output<'db> = Value>,
{
    let memo = ingredient
        .get_memo_from_table_for(
            db.zalsa(),
            id,
            ingredient.memo_ingredient_index(db.zalsa(), id),
        )
        .expect("outer iteration has selected both heads");
    MemoState {
        identity: std::ptr::from_ref(memo) as usize,
        value: memo.value().map(|value| value.number),
        final_value: !memo.header.may_be_provisional(),
        verified: memo.header.verified_at.load(),
        iteration: memo.header.revisions.iteration(),
        heads: memo
            .header
            .revisions
            .cycle_heads()
            .iter()
            .map(|head| (head.database_key_index, head.iteration.load()))
            .collect(),
    }
}

fn memos(db: &dyn Database, input: Input) -> [MemoState; 2] {
    [
        memo_state(db, a::fn_ingredient_(db, db.zalsa()), input.as_id()),
        memo_state(db, b::fn_ingredient_(db, db.zalsa()), input.as_id()),
    ]
}

#[derive(Debug)]
struct Observation {
    stage: &'static str,
    caller: Option<DatabaseKeyIndex>,
    query_depth: usize,
    operation_depth: usize,
    held: [bool; 2],
    memos: Option<[MemoState; 2]>,
    reason: Option<Incomplete>,
    panicking: bool,
    requested: bool,
    enabled: bool,
}

struct State {
    site: Site,
    action: Action,
    input: Input,
    keys: [DatabaseKeyIndex; 2],
    endpoint: Option<Endpoint<'static, 'static>>,
    identity: Arc<()>,
    armed: bool,
    saw_storage: bool,
    fired: bool,
    next_tag: usize,
    candidate: usize,
    provider_counts: [usize; 3],
    observations: Vec<Observation>,
    requested_local: Option<RequestedLocal>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    static ORDINARY_COUNTS: Cell<[usize; 3]> = const { Cell::new([0; 3]) };
}

struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        STATE.with_borrow_mut(|state| *state = None);
    }
}

fn begin(site: Site, action: Action, input: Input, keys: [DatabaseKeyIndex; 2]) -> Reset {
    ORDINARY_COUNTS.set([0; 3]);
    STATE.with_borrow_mut(|state| {
        assert!(state.is_none());
        *state = Some(State {
            site,
            action,
            input,
            keys,
            endpoint: None,
            identity: Arc::new(()),
            armed: false,
            saw_storage: false,
            fired: false,
            next_tag: 0,
            candidate: 0,
            provider_counts: [0; 3],
            observations: Vec::new(),
            requested_local: None,
        });
    });
    Reset
}

fn observe(stage: &'static str) {
    let data = STATE.with_borrow(|state| {
        state
            .as_ref()
            .filter(|state| state.fired)
            .map(|state| (state.input, state.keys))
    });
    let Some((input, keys)) = data else { return };
    let observation = crate::with_attached_database(|db| Observation {
        stage,
        caller: db.zalsa_local().active_query().map(|(key, _)| key),
        query_depth: db
            .zalsa_local()
            .try_with_query_stack(|stack| stack.len())
            .unwrap(),
        operation_depth: attempt_probe::stack_depths().0,
        held: keys.map(|key| {
            matches!(
                db.zalsa()
                    .lookup_ingredient(key.ingredient_index())
                    .as_function()
                    .unwrap()
                    .sync_table()
                    .peek_claim(db.zalsa(), key.key_index(), Reentrancy::Deny),
                ClaimResult::Cycle { .. }
            )
        }),
        memos: matches!(
            stage,
            "callout" | "child" | "local.request" | "local.checked" | "accepted" | "native.check"
        )
        .then(|| memos(db, input)),
        reason: attempt_probe::current().and_then(|support| support.reason()),
        panicking: crate::sync::thread::panicking(),
        requested: db.cancellation_token().is_cancelled(),
        enabled: db.zalsa_local().should_trigger_local_cancellation(),
    })
    .expect("publication cleanup retains the attached database");
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

fn fire() -> RunResult<()> {
    let hook = STATE.with_borrow_mut(|state| {
        let state = state.as_mut()?;
        if state.fired {
            return None;
        }
        let endpoint = state.endpoint.take()?;
        state.fired = true;
        Some((
            endpoint,
            state.site,
            state.action,
            state.identity.clone(),
            state.requested_local,
        ))
    });
    let Some((endpoint, site, action, identity, requested_local)) = hook else {
        return Ok(());
    };
    observe("callout");
    if let Some(action) = requested_local {
        endpoint.context.db.cancellation_token().cancel();
        observe("local.request");
        assert!(
            !endpoint
                .context
                .db
                .zalsa_local()
                .should_trigger_local_cancellation()
        );
        endpoint.context.db.unwind_if_revision_cancelled();
        observe("local.checked");
        if action == RequestedLocal::Return {
            return Ok(());
        }
        let child = Marker("child");
        let _reply = endpoint.demand(move || {
            poll_fn(move |_| -> Poll<RunResult<()>> {
                let _child = &child;
                panic!("a rejected publication must not run its queued child");
            })
        })?;
        return match action {
            RequestedLocal::Refuse => {
                attempt_probe::report_incomplete(endpoint.context.db, Incomplete::Interrupted);
                Err(RunError::Refused(Incomplete::Interrupted))
            }
            RequestedLocal::Panic => panic_any(PanicMarker(identity)),
            RequestedLocal::Child | RequestedLocal::Return => Ok(()),
        };
    }
    if action == Action::Observe {
        return Ok(());
    }
    if site != Site::Finalized || action != Action::Refuse {
        let child = Marker("child");
        let _reply = endpoint.demand(move || {
            poll_fn(move |_| -> Poll<RunResult<()>> {
                let _child = &child;
                panic!("a child queued by a failed publication callback must not run");
            })
        })?;
    }
    match action {
        Action::Refuse => {
            attempt_probe::report_incomplete(endpoint.context.db, Incomplete::Interrupted);
            Err(RunError::Refused(Incomplete::Interrupted))
        }
        Action::Panic => panic_any(PanicMarker(identity)),
        Action::Local | Action::PendingWrite => {
            if action == Action::Local {
                endpoint.context.db.cancellation_token().cancel();
            } else {
                endpoint
                    .context
                    .db
                    .zalsa()
                    .runtime()
                    .set_cancellation_flag();
            }
            endpoint.context.db.unwind_if_revision_cancelled();
            panic!("native cancellation unexpectedly returned");
        }
        Action::Observe | Action::ChildOnly => Ok(()),
    }
}

struct Admission;
impl ExecutionAdmission for Admission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let selected = STATE.with_borrow_mut(|state| {
            let Some(state) = state else { return false };
            if !state.armed || state.fired {
                return false;
            }
            match work {
                ExecutionWork::Resource { .. } => {
                    state.saw_storage = true;
                    state.site == Site::Storage
                }
                ExecutionWork::Work { .. } => state.site == Site::Work && state.saw_storage,
                _ => false,
            }
        });
        if selected {
            fire()?;
        }
        Ok(())
    }
}
static ADMISSION: Admission = Admission;

#[crate::db]
#[derive(Clone)]
struct Db {
    storage: crate::Storage<Self>,
}
#[crate::db]
impl Database for Db {}

fn database() -> &'static Db {
    Box::leak(Box::new(Db {
        storage: crate::Storage::new(Some(Box::new(|event| {
            let requested_local = STATE.with_borrow(|state| {
                state
                    .as_ref()
                    .is_some_and(|state| state.fired && state.requested_local.is_some())
            });
            if requested_local {
                match &event.kind {
                    EventKind::WillCheckCancellation => observe("native.check"),
                    EventKind::DidFinalizeCycle { database_key, .. }
                        if STATE.with_borrow(|state| {
                            state.as_ref().unwrap().keys[0] == *database_key
                        }) =>
                    {
                        observe("accepted")
                    }
                    _ => {}
                }
            }
            let finalized = STATE.with_borrow_mut(|state| {
                let Some(state) = state else { return false };
                if state.fired {
                    return false;
                }
                match event.kind {
                    EventKind::DidFinalizeCycle { database_key, .. } => {
                        state.site == Site::Finalized && database_key == state.keys[0]
                    }
                    EventKind::WillIterateCycle { database_key, .. }
                        if state.site != Site::Finalized && database_key == state.keys[0] =>
                    {
                        state.armed = true;
                        false
                    }
                    _ => false,
                }
            });
            if finalized {
                let _ = fire();
            }
        }))),
    }))
}

#[derive(Debug)]
struct Value {
    number: u32,
    tag: usize,
}
impl Value {
    fn plain(number: u32) -> Self {
        Self { number, tag: 0 }
    }
}
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        self.number == other.number
    }
}
impl Eq for Value {}
impl Drop for Value {
    fn drop(&mut self) {
        if self.tag != 0
            && STATE.with_borrow(|state| {
                state.as_ref().is_some_and(|state| {
                    state.fired && state.site != Site::Finalized && state.candidate == self.tag
                })
            })
        {
            observe("candidate");
        }
    }
}

#[crate::input]
struct Input {
    marker: u32,
}

fn count(index: usize) {
    let mut counts = ORDINARY_COUNTS.get();
    counts[index] += 1;
    ORDINARY_COUNTS.set(counts);
}

#[crate::tracked(returns(ref), attempt = ReturnOnly)]
fn parent(db: &dyn Database, input: Input) -> Value {
    Value::plain(a(db, input).number)
}

#[crate::tracked(returns(ref), attempt = ReturnOnly, cycle_initial = initial, cycle_fn = recover_a)]
fn a(db: &dyn Database, input: Input) -> Value {
    count(0);
    Value::plain(b(db, input).number + 1)
}

#[crate::tracked(returns(ref), attempt = ReturnOnly, cycle_initial = initial)]
fn b(db: &dyn Database, input: Input) -> Value {
    count(1);
    Value::plain(a(db, input).number.max(b(db, input).number))
}

fn initial(_db: &dyn Database, _id: Id, _input: Input) -> Value {
    Value::plain(0)
}
fn recover_a(
    _db: &dyn Database,
    _cycle: &Cycle<'_>,
    _previous: &Value,
    mut value: Value,
    _input: Input,
) -> Value {
    count(2);
    value.number = value.number.min(3);
    value
}

struct Provider<P: Configuration, A: Configuration, B: Configuration> {
    parent: Route<'static, P>,
    a: Route<'static, A>,
    b: Route<'static, B>,
}

impl<P, A, B, C> ExecutableRouteProvider<'static, 'static, C> for Provider<P, A, B>
where
    P: Configuration<DbView = dyn Database, Input<'static> = Input, Output<'static> = Value>,
    A: Configuration<DbView = dyn Database, Input<'static> = Input, Output<'static> = Value>,
    B: Configuration<DbView = dyn Database, Input<'static> = Input, Output<'static> = Value>,
    C: Configuration<DbView = dyn Database, Input<'static> = Input, Output<'static> = Value>,
{
    // Input conversion constructs a handle; Value equality compares its u32.
    fixture_native_value!(executable, 'static, 'static, C, 1);

    async fn body(
        &'static self,
        context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        input: Input,
    ) -> RunResult<Value> {
        if TypeId::of::<C>() == TypeId::of::<P>() {
            let _parent = Marker("parent");
            Ok(Value::plain(
                context.fetch_ref(&self.a, input.as_id())?.await?.number,
            ))
        } else if TypeId::of::<C>() == TypeId::of::<A>() {
            STATE.with_borrow_mut(|state| state.as_mut().unwrap().provider_counts[0] += 1);
            let number = context.fetch_ref(&self.b, input.as_id())?.await?.number + 1;
            let tag = STATE.with_borrow_mut(|state| {
                let state = state.as_mut().unwrap();
                state.next_tag += 1;
                state.next_tag
            });
            Ok(Value { number, tag })
        } else {
            assert_eq!(TypeId::of::<C>(), TypeId::of::<B>());
            STATE.with_borrow_mut(|state| state.as_mut().unwrap().provider_counts[1] += 1);
            let outer = context.fetch_ref(&self.a, input.as_id())?.await?.number;
            let inner = context.fetch_ref(&self.b, input.as_id())?.await?.number;
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
        _previous: &'call Value,
        mut value: Value,
        _input: Input,
    ) -> impl Future<Output = RunResult<Value>> + 'call
    where
        'static: 'call,
    {
        if TypeId::of::<C>() == TypeId::of::<A>() {
            value.number = value.number.min(3);
            STATE.with_borrow_mut(|state| {
                let state = state.as_mut().unwrap();
                state.provider_counts[2] += 1;
                if !state.fired {
                    state.endpoint = Some(context.endpoint().inner.clone());
                    state.candidate = value.tag;
                }
            });
        }
        ready(Ok(value))
    }
}

fn run(db: &'static Db, input: Input) -> RunResult<u32> {
    let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
    let provider: &'static _ = Box::leak(Box::new(Provider {
        parent: registry.reserve(db as &dyn Database, parent::fn_ingredient_(db, db.zalsa()))?,
        a: registry.reserve(db as &dyn Database, a::fn_ingredient_(db, db.zalsa()))?,
        b: registry.reserve(db as &dyn Database, b::fn_ingredient_(db, db.zalsa()))?,
    }));
    let binding = registry.provider(provider)?;
    registry.bind_executable(&provider.parent, &binding)?;
    registry.bind_executable(&provider.a, &binding)?;
    registry.bind_executable(&provider.b, &binding)?;
    registry.seal()?.run(move |endpoint| async move {
        let _root = Marker("root");
        Ok(endpoint
            .provider(binding)?
            .fetch_ref(&provider.parent, input.as_id())?
            .await?
            .number)
    })
}

fn control(site: Site, action: Action) {
    let db = database();
    let input = Input::new(db, 0);
    let a_ingredient = a::fn_ingredient_(db, db.zalsa());
    let keys = [
        a_ingredient.database_key_index(input.as_id()),
        b::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
    ];
    let parent_key = parent::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id());
    let _reset = begin(site, action, input, keys);
    let stamp = Stamp::current(db);
    let identity = STATE.with_borrow(|state| state.as_ref().unwrap().identity.clone());
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        try_with_attempt(db, 100_000, || {
            let result = run(db, input);
            assert_eq!(
                result,
                match action {
                    Action::Observe => Ok(3),
                    Action::ChildOnly => Err(RunError::Contract("completed task retained a child")),
                    _ => Err(RunError::Refused(Incomplete::Interrupted)),
                }
            );
            result
        })
    }));
    if matches!(action, Action::Local | Action::PendingWrite) {
        db.zalsa_local().uncancel();
        db.zalsa().runtime().reset_cancellation_flag();
    }
    assert_eq!(Stamp::current(db), stamp);
    match action {
        Action::Observe => assert_eq!(outcome.unwrap(), Ok(AttemptOutcome::Complete(Ok(3)))),
        Action::Refuse | Action::ChildOnly => assert_eq!(
            outcome.unwrap(),
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        ),
        Action::Panic => {
            let payload = outcome.expect_err("postcommit event preserves native panic");
            let marker = payload
                .downcast_ref::<PanicMarker>()
                .expect("original panic type");
            assert!(Arc::ptr_eq(&marker.0, &identity));
        }
        Action::Local | Action::PendingWrite => {
            let payload = outcome.expect_err("cancellation uses native panic transport");
            assert!(matches!(
                (action, payload.downcast_ref::<crate::Cancelled>()),
                (Action::Local, Some(crate::Cancelled::Local))
                    | (Action::PendingWrite, Some(crate::Cancelled::PendingWrite))
            ));
        }
    }
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    STATE.with_borrow(|state| {
        let state = state.as_ref().unwrap();
        assert!(state.fired);
        assert!(state.endpoint.is_none());
        let expected = if site != Site::Finalized {
            &["callout", "child", "candidate", "parent", "root"][..]
        } else if matches!(action, Action::Observe | Action::Refuse) {
            &["callout", "parent", "root"][..]
        } else {
            &["callout", "child", "parent", "root"][..]
        };
        assert_eq!(
            state
                .observations
                .iter()
                .map(|event| event.stage)
                .collect::<Vec<_>>(),
            expected
        );
        for event in &state.observations {
            let root = event.stage == "root";
            let parent = event.stage == "parent";
            assert_eq!(event.caller, (!root).then_some(parent_key), "{event:?}");
            assert_eq!(event.query_depth, usize::from(!root), "{event:?}");
            assert_eq!(
                event.operation_depth,
                if root {
                    0
                } else if parent {
                    1
                } else {
                    2
                },
                "{event:?}"
            );
            assert_eq!(
                event.held[0],
                site != Site::Finalized && !root && !parent,
                "{event:?}"
            );
            if site == Site::Finalized {
                assert_eq!(event.held, [false; 2], "{event:?}");
            }
            let after_failure = event.stage != "callout";
            assert_eq!(
                event.reason,
                (matches!(
                    action,
                    Action::Refuse | Action::ChildOnly | Action::Local | Action::PendingWrite
                ) && after_failure)
                    .then_some(Incomplete::Interrupted),
                "{event:?}"
            );
            assert_eq!(
                event.panicking,
                action == Action::Panic && after_failure,
                "{event:?}"
            );
        }
        let observed = state.observations[0].memos.as_ref().unwrap();
        assert!(
            observed
                .iter()
                .all(|memo| memo.final_value == (site == Site::Finalized))
        );
        if site == Site::Finalized {
            assert!(observed.iter().all(|memo| memo.value == Some(3)));
        }
        if let Some(child) = state
            .observations
            .iter()
            .find(|event| event.stage == "child")
        {
            assert_eq!(child.memos.as_ref().unwrap(), observed);
        }
    });
    if site == Site::Finalized {
        let accepted = memos(db, input);
        STATE.with_borrow(|state| {
            assert_eq!(
                state.as_ref().unwrap().observations[0]
                    .memos
                    .as_ref()
                    .unwrap(),
                &accepted
            )
        });
        let provider_counts = STATE.with_borrow(|state| state.as_ref().unwrap().provider_counts);
        assert_eq!(ORDINARY_COUNTS.get(), [0; 3]);
        let retry_stamp = Stamp::current(db);
        assert_eq!(
            [
                (a(db, input).number, b(db, input).number),
                (a(db, input).number, b(db, input).number)
            ],
            [(3, 3), (3, 3)]
        );
        assert_eq!(ORDINARY_COUNTS.get(), [0; 3]);
        assert_eq!(memos(db, input), accepted);
        assert_eq!(
            STATE.with_borrow(|state| state.as_ref().unwrap().provider_counts),
            provider_counts
        );
        assert_eq!(Stamp::current(db), retry_stamp);
    } else {
        let selected = memos(db, input);
        assert!(selected.iter().all(|memo| !memo.final_value));
        if matches!(action, Action::Refuse | Action::Local | Action::PendingWrite) {
            let memo = a_ingredient
                .get_memo_from_table_for(
                    db.zalsa(),
                    input.as_id(),
                    a_ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
                )
                .unwrap();
            assert!(!memo.header.can_seed_attempt(db.zalsa()));
            let retry_stamp = Stamp::current(db);
            assert_eq!(
                [
                    (a(db, input).number, b(db, input).number),
                    (a(db, input).number, b(db, input).number)
                ],
                [(3, 3), (3, 3)]
            );
            assert!(ORDINARY_COUNTS.get()[0] > 0);
            assert_eq!(Stamp::current(db), retry_stamp);
        } else {
            assert_eq!(selected[0].value, None);
        }
    }
}

#[test]
fn did_finalize_observes_accepted_unclaimed_group() {
    let baseline = database();
    let input = Input::new(baseline, 0);
    assert_eq!(
        (a(baseline, input).number, b(baseline, input).number),
        (3, 3)
    );
    for action in [
        Action::Observe,
        Action::Refuse,
        Action::ChildOnly,
        Action::Panic,
    ] {
        control(Site::Finalized, action);
    }
}

#[test]
fn publication_admission_retains_candidate_and_popped_owner() {
    for site in [Site::Storage, Site::Work] {
        control(site, Action::Refuse);
    }
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn cancellation_payloads_abort_precommit_and_preserve_postcommit_values() {
    control(Site::Work, Action::PendingWrite);
    for action in [Action::Local, Action::PendingWrite] {
        control(Site::Finalized, action);
    }
}

fn requested_local_control(site: Site, action: RequestedLocal) {
    let db = database();
    let input = Input::new(db, 0);
    let keys = [
        a::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
        b::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
    ];
    let parent_key = parent::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id());
    let _reset = begin(site, Action::Observe, input, keys);
    STATE.with_borrow_mut(|state| state.as_mut().unwrap().requested_local = Some(action));
    let identity = STATE.with_borrow(|state| state.as_ref().unwrap().identity.clone());
    let stamp = Stamp::current(db);
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        try_with_attempt(db, 100_000, || {
            let result = run(db, input);
            assert_eq!(
                result,
                match action {
                    RequestedLocal::Refuse => Err(RunError::Refused(Incomplete::Interrupted)),
                    RequestedLocal::Child =>
                        Err(RunError::Contract("completed task retained a child")),
                    RequestedLocal::Return | RequestedLocal::Panic =>
                        panic!("the native payload must leave the driver"),
                }
            );
            result
        })
    }));
    match action {
        RequestedLocal::Return => assert!(matches!(
            outcome.unwrap_err().downcast_ref::<crate::Cancelled>(),
            Some(crate::Cancelled::Local)
        )),
        RequestedLocal::Panic => {
            let payload = outcome.unwrap_err();
            assert!(Arc::ptr_eq(
                &payload.downcast_ref::<PanicMarker>().unwrap().0,
                &identity
            ));
        }
        RequestedLocal::Refuse | RequestedLocal::Child => assert_eq!(
            outcome.unwrap(),
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        ),
    }
    assert_eq!(Stamp::current(db), stamp);
    assert!(attempt_probe::current().is_none());
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    assert!(crate::with_attached_database(|_| ()).is_none());
    STATE.with_borrow(|state| {
        let state = state.as_ref().unwrap();
        assert!(state.fired && state.endpoint.is_none());
        let request = state
            .observations
            .iter()
            .position(|event| event.stage == "local.request")
            .unwrap();
        let checked = state
            .observations
            .iter()
            .position(|event| event.stage == "local.checked")
            .unwrap();
        assert!(request < checked);
        for event in &state.observations[request..=checked] {
            assert!(event.requested && !event.enabled && !event.panicking);
            assert!(event.held[0]);
        }
        let at_callout = state.observations[0].memos.as_ref().unwrap();
        assert!(at_callout.iter().all(|memo| !memo.final_value));
        if action == RequestedLocal::Return {
            let accepted = state
                .observations
                .iter()
                .position(|event| event.stage == "accepted")
                .unwrap();
            let enabled = state
                .observations
                .iter()
                .position(|event| event.stage == "native.check" && event.requested && event.enabled)
                .unwrap();
            assert!(checked < accepted && accepted < enabled);
            let published = &state.observations[accepted];
            assert_eq!(published.held, [false; 2]);
            assert!(
                published
                    .memos
                    .as_ref()
                    .unwrap()
                    .iter()
                    .all(|memo| memo.final_value && memo.value == Some(3))
            );
            for event in &state.observations[request..accepted] {
                if event.stage == "native.check" {
                    assert!(event.requested && !event.enabled);
                }
            }
            assert!(
                !state
                    .observations
                    .iter()
                    .any(|event| matches!(event.stage, "child" | "candidate"))
            );
            assert_eq!(state.observations[enabled].held, [false; 2]);
        } else {
            let drops = state
                .observations
                .iter()
                .filter(|event| {
                    matches!(
                        event.stage,
                        "callout" | "child" | "candidate" | "parent" | "root"
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(
                drops.iter().map(|event| event.stage).collect::<Vec<_>>(),
                ["callout", "child", "candidate", "parent", "root"]
            );
            assert_eq!(drops[0].memos, drops[1].memos);
            for event in drops {
                let root = event.stage == "root";
                let parent = event.stage == "parent";
                assert_eq!(event.caller, (!root).then_some(parent_key));
                assert_eq!(event.query_depth, usize::from(!root));
                assert_eq!(
                    event.operation_depth,
                    if root {
                        0
                    } else if parent {
                        1
                    } else {
                        2
                    }
                );
                assert_eq!(event.held[0], !root && !parent);
                assert_eq!(
                    event.panicking,
                    action == RequestedLocal::Panic && event.stage != "callout"
                );
                assert_eq!(
                    event.reason,
                    (action != RequestedLocal::Panic && event.stage != "callout")
                        .then_some(Incomplete::Interrupted)
                );
            }
            assert!(
                !state
                    .observations
                    .iter()
                    .any(|event| event.stage == "accepted")
            );
        }
    });
    db.zalsa_local().uncancel();
    assert!(!db.zalsa().runtime().load_cancellation_flag());
    let before = memos(db, input);
    let counts = STATE.with_borrow(|state| state.as_ref().unwrap().provider_counts);
    if action == RequestedLocal::Panic {
        assert!(before.iter().all(|memo| !memo.final_value));
        assert_eq!(before[0].value, None);
        for _ in 0..2 {
            let result = crate::Cancelled::catch(AssertUnwindSafe(|| {
                try_with_attempt(db, 100_000, || a(db, input).number)
            }));
            assert!(matches!(result, Err(crate::Cancelled::PropagatedPanic)));
            assert_eq!(memos(db, input), before);
            assert_eq!(ORDINARY_COUNTS.get(), [0; 3]);
            assert_eq!(attempt_probe::stack_depths(), (0, 0));
        }
    } else {
        if action != RequestedLocal::Return {
            assert!(before.iter().all(|memo| !memo.final_value));
            let memo = a::fn_ingredient_(db, db.zalsa())
                .memo(db.zalsa(), input.as_id())
                .unwrap();
            assert!(!memo.header().can_seed_attempt(db.zalsa()));
        }
        assert_eq!(
            try_with_attempt(db, 100_000, || (a(db, input).number, b(db, input).number)),
            Ok(AttemptOutcome::Complete((3, 3)))
        );
        let accepted = memos(db, input);
        let ordinary = ORDINARY_COUNTS.get();
        assert_eq!((a(db, input).number, b(db, input).number), (3, 3));
        assert_eq!(memos(db, input), accepted);
        assert_eq!(ORDINARY_COUNTS.get(), ordinary);
        if action == RequestedLocal::Return {
            assert_eq!(accepted, before);
            assert_eq!(ordinary, [0; 3]);
        }
    }
    assert_eq!(
        STATE.with_borrow(|state| state.as_ref().unwrap().provider_counts),
        counts
    );
    assert_eq!(Stamp::current(db), stamp);
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn requested_local_finishes_publication_before_delivery() {
    for site in [Site::Storage, Site::Work] {
        requested_local_control(site, RequestedLocal::Return);
    }
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn requested_local_preserves_precommit_rejection_and_panic() {
    for site in [Site::Storage, Site::Work] {
        for action in [
            RequestedLocal::Refuse,
            RequestedLocal::Child,
            RequestedLocal::Panic,
        ] {
            requested_local_control(site, action);
        }
    }
    control(Site::Storage, Action::PendingWrite);
}
