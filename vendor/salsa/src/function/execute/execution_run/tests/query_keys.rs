use std::cell::{Cell, RefCell};
use std::hash::{Hash, Hasher};
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::rc::Rc;
use std::sync::Arc;

use super::super::registration::{
    ExecutableRouteProvider, FixedQueryKeys, NativeValueOperation, NativeValueQuote,
    ProviderBinding, ProviderContext, RegistryBuilder, RetainedInput, Route, TaskEndpoint,
};
use super::super::{ExecutionAdmission, ExecutionWork, RunError, RunResult};
use super::observation;
#[cfg(feature = "accumulator")]
use crate::Accumulator;
use crate::active_query::read_storage::ReadState;
use crate::attempt_probe::{self, AttemptOutcome, Incomplete, try_with_attempt};
use crate::function::memo::check_passive_key_memo;
use crate::function::{
    ClaimResult, Configuration, CopyMemoProfile, FixedQueryFields, IngredientImpl,
    InternedQueryConfiguration, Memo, Reentrancy,
};
use crate::id::FromId;
use crate::ingredient::Ingredient;
use crate::plumbing::{AsId, QuoteError, QuoteFuel};
use crate::prepared_source_probe::Stamp;
use crate::table::memo::detached_observation;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseKeyIndex, Durability, EventKind, Id, Setter};

mod owned;
mod quotation;
#[cfg(feature = "salsa_unstable")]
mod retirement;

#[derive(Clone, Copy, Debug, Eq, PartialEq, crate::SalsaValue)]
struct Collision(u32);

impl Hash for Collision {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u32(0);
    }
}

// Equality and hashing examine fixed scalar data and cannot invoke semantic work.
impl FixedQueryFields for Collision {}

#[crate::input]
struct Request {
    #[returns(copy)]
    left: u32,
    #[returns(copy)]
    right: u32,
    #[returns(copy)]
    next: Option<Request>,
}

fn value(left: Collision, right: u32) -> bool {
    left.0 == right
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn compare(_db: &dyn Database, left: Collision, right: u32) -> bool {
    state().ordinary.set(state().ordinary.get() + 1);
    value(left, right)
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn caller(db: &dyn Database, request: Request) -> bool {
    let current = compare(db, Collision(request.left(db)), request.right(db));
    current ^ request.next(db).is_some_and(|next| caller(db, next))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeyEvent {
    Intern,
    Validate,
    Reuse,
    Discard,
}

#[derive(Debug)]
enum FixedTrace {
    Work(ExecutionWork),
    Cancellation,
    Key(KeyEvent, DatabaseKeyIndex),
}

fn fixed_trace(record: FixedTrace) {
    if let Some(trace) = state().fixed_trace.borrow_mut().as_mut() {
        trace.push(record);
    }
}

#[derive(Default)]
struct State {
    fixed_trace: RefCell<Option<Vec<FixedTrace>>>,
    ordinary: Cell<usize>,
    leaves: Cell<usize>,
    callers: Cell<usize>,
    first_leaf_allowance: Cell<Option<usize>>,
    first_key_allowance: Cell<Option<usize>>,
    attempted: Cell<usize>,
    work: Cell<usize>,
    in_key: Cell<bool>,
    delivered: RefCell<Vec<Id>>,
    events: RefCell<Vec<(KeyEvent, DatabaseKeyIndex)>>,
    journal: RefCell<Vec<&'static str>>,
    direct_error: Cell<Option<RunError>>,
    hook: RefCell<Option<Rc<Hook>>>,
    values: finite_values::ValueState,
}

thread_local! {
    static STATE: RefCell<Option<Rc<State>>> = const { RefCell::new(None) };
}

fn state() -> Rc<State> {
    STATE.with_borrow(|slot| slot.as_ref().unwrap().clone())
}

fn fixture() -> (Rc<State>, impl Drop) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            STATE.with_borrow_mut(|slot| *slot = None);
        }
    }
    let state = Rc::new(State::default());
    STATE.with_borrow_mut(|slot| assert!(slot.replace(state.clone()).is_none()));
    (state, Reset)
}

#[crate::db]
struct TestDb {
    storage: crate::Storage<Self>,
}

#[crate::db]
impl Database for TestDb {}

impl TestDb {
    fn new() -> Self {
        Self {
            storage: crate::Storage::new(Some(Box::new(|event| {
                if matches!(event.kind, EventKind::WillCheckCancellation) {
                    fixed_trace(FixedTrace::Cancellation);
                }
                let observed = match event.kind {
                    EventKind::DidInternValue { key, .. } => Some((KeyEvent::Intern, key)),
                    EventKind::DidValidateInternedValue { key, .. } => {
                        Some((KeyEvent::Validate, key))
                    }
                    EventKind::DidReuseInternedValue { key, .. } => Some((KeyEvent::Reuse, key)),
                    EventKind::DidDiscard { key } => Some((KeyEvent::Discard, key)),
                    _ => None,
                };
                if let Some(observed) = observed {
                    fixed_trace(FixedTrace::Key(observed.0, observed.1));
                    state().events.borrow_mut().push(observed);
                    let hook = state().hook.borrow().clone();
                    if let Some(hook) = hook {
                        hook.event(observed);
                    }
                    finite_values::event(observed);
                    profiled_keys::event(observed);
                }
            }))),
        }
    }
}

struct Admission;

impl ExecutionAdmission for Admission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        fixed_trace(FixedTrace::Work(work));
        finite_values::admit(work)?;
        if state().in_key.get()
            && let Some(hook) = state().hook.borrow().clone()
            && matches!(
                hook.fault,
                Fault::ReadWork | Fault::ReadBytes | Fault::RevisionReadWork
            )
            && hook.notes.borrow().is_empty()
        {
            hook.note("request");
        }
        if state().in_key.get()
            && let Some(hook) = state().hook.borrow().clone()
            && !hook.fired.get()
            && !ids(hook.db).is_empty()
            && match hook.fault {
                Fault::ReadWork | Fault::RevisionReadWork => {
                    matches!(work, ExecutionWork::Work { .. })
                }
                Fault::ReadBytes => {
                    matches!(work, ExecutionWork::Resource { requested_bytes } if requested_bytes != 0)
                }
                _ => false,
            }
        {
            hook.fail();
            return Err(RunError::Refused(Incomplete::Interrupted));
        }
        if state().in_key.get() && matches!(work, ExecutionWork::Work { units: 1 }) {
            state().work.set(state().work.get() + 1);
            let hook = state().hook.borrow().clone();
            if let Some(hook) = hook
                && hook.fault == Fault::Admission
                && !hook.fired.get()
            {
                hook.fail();
                return Err(RunError::Refused(Incomplete::Interrupted));
            }
        }
        Ok(())
    }
}

static ADMISSION: Admission = Admission;

struct KeyScope(bool);

impl KeyScope {
    fn new() -> Self {
        state().attempted.set(state().attempted.get() + 1);
        Self(state().in_key.replace(true))
    }
}

impl Drop for KeyScope {
    fn drop(&mut self) {
        state().in_key.set(self.0);
    }
}

struct Marker(&'static str);

impl Drop for Marker {
    fn drop(&mut self) {
        finite_values::note(self.0);
        profiled_keys::note(self.0);
        let hook = state().hook.borrow().clone();
        if let Some(hook) = hook {
            hook.note(self.0);
            if self.0 == "child" && hook.fault == Fault::DiscardPanic {
                let id = compare::intern_ingredient_(hook.db.zalsa()).intern_id(
                    hook.db.zalsa(),
                    hook.db.zalsa_local(),
                    hook.replacement,
                    |_, fields| fields,
                );
                assert_eq!(Some(id), hook.new_id.get());
            }
        }
        state().journal.borrow_mut().push(self.0);
    }
}

struct Leaf;

impl<'run, 'db: 'run, C> ExecutableRouteProvider<'run, 'db, C> for Leaf
where
    C: for<'a> Configuration<
            DbView = dyn Database,
            Input<'a> = (Collision, u32),
            Output<'a> = bool,
        >,
{
    // Collision and u32 clone their scalar fields; output equality compares one bool.
    fixture_native_value!(executable, 'run, 'db, C, 2);

    async fn body(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        input: (Collision, u32),
    ) -> RunResult<bool> {
        state().leaves.set(state().leaves.get() + 1);
        Ok(value(input.0, input.1))
    }

    async fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _id: Id,
        _input: (Collision, u32),
    ) -> RunResult<bool> {
        Err(RunError::RequiresFetch)
    }

    async fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call bool,
        _value: bool,
        _input: (Collision, u32),
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }
}

struct Caller<'run, 'db: 'run, C: InternedQueryConfiguration, D: Configuration> {
    target: Route<'db, C>,
    keys: FixedQueryKeys<'db, C>,
    leaf: ProviderBinding<'run, Leaf>,
    caller: Route<'db, D>,
    arm: fn(TaskEndpoint<'run, 'db>),
}

fn no_hook(_endpoint: TaskEndpoint<'_, '_>) {}

fn arm_hook(endpoint: TaskEndpoint<'static, 'static>) {
    if let Some(hook) = state().hook.borrow().as_ref() {
        if hook.fault == Fault::ReadBytes {
            // Three distinct input fields fill the ordinary frame's initial edge allocation.
            // Delivering the interned key then needs a larger backing store.
            let _ = Request::from_id(hook.caller.key_index()).next(hook.db);
        }
        *hook.endpoint.borrow_mut() = Some(endpoint);
    }
}

impl<'run, 'db: 'run, C, D> ExecutableRouteProvider<'run, 'db, D> for Caller<'run, 'db, C, D>
where
    C: InternedQueryConfiguration
        + for<'a> crate::interned::Configuration<Fields<'a> = (Collision, u32)>
        + for<'a> Configuration<DbView = dyn Database, Output<'a> = bool>,
    D: for<'a> Configuration<DbView = dyn Database, Input<'a> = Request, Output<'a> = bool>,
{
    // Request conversion constructs a handle; output equality compares bool.
    fixture_native_value!(executable, 'run, 'db, D, 1);

    async fn body(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Database,
        input: Request,
    ) -> RunResult<bool> {
        let _caller = Marker("caller");
        state().callers.set(state().callers.get() + 1);
        let endpoint = context.endpoint();
        (self.arm)(endpoint.clone());
        let tuple = (Collision(input.left(db)), input.right(db));
        let id = {
            let _key = KeyScope::new();
            endpoint.intern_query_key(&self.keys, tuple).await
        };
        state().delivered.borrow_mut().push(id);
        if state().first_key_allowance.get().is_none() {
            state()
                .first_key_allowance
                .set(attempt_probe::remaining_allowance_for_diagnostics(db));
        }
        let leaf = endpoint.provider(self.leaf.clone())?;
        let current = *endpoint
            .child_call(|| async { leaf.fetch_ref(&self.target, id)?.await })
            .await;
        if state().first_leaf_allowance.get().is_none() {
            state()
                .first_leaf_allowance
                .set(attempt_probe::remaining_allowance_for_diagnostics(db));
        }
        let next = if let Some(next) = input.next(db) {
            *endpoint
                .child_call(|| async { context.fetch_ref(&self.caller, next.as_id())?.await })
                .await
        } else {
            false
        };
        Ok(current ^ next)
    }

    async fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _id: Id,
        _input: Request,
    ) -> RunResult<bool> {
        Err(RunError::RequiresFetch)
    }

    async fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call bool,
        _value: bool,
        _input: Request,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }
}

fn run(db: &TestDb, request: Request) -> RunResult<bool> {
    let leaf = Leaf;
    let provider;
    let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
    let target = registry.reserve(db as &dyn Database, compare::fn_ingredient_(db, db.zalsa()))?;
    let keys = registry.fixed_query_keys(&target)?;
    let caller = registry.reserve(db as &dyn Database, caller::fn_ingredient_(db, db.zalsa()))?;
    let leaf = registry.provider(&leaf)?;
    registry.bind_executable(&target, &leaf)?;
    provider = Caller {
        target,
        keys,
        leaf,
        caller,
        arm: no_hook,
    };
    let route = provider.caller.clone();
    let binding = registry.provider(&provider)?;
    registry.bind_executable(&provider.caller, &binding)?;
    registry.seal()?.run(move |endpoint| async move {
        let _root = Marker("root");
        Ok(*endpoint
            .provider(binding)?
            .fetch_ref(&route, request.as_id())?
            .await?)
    })
}

fn run_hook(db: &'static TestDb, request: Request) -> RunResult<bool> {
    let leaf: &'static Leaf = Box::leak(Box::new(Leaf));
    let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
    let target = registry.reserve(db as &dyn Database, compare::fn_ingredient_(db, db.zalsa()))?;
    let keys = registry.fixed_query_keys(&target)?;
    let caller = registry.reserve(db as &dyn Database, caller::fn_ingredient_(db, db.zalsa()))?;
    let leaf = registry.provider(leaf)?;
    registry.bind_executable(&target, &leaf)?;
    let provider: &'static _ = Box::leak(Box::new(Caller {
        target,
        keys,
        leaf,
        caller,
        arm: arm_hook,
    }));
    let binding = registry.provider(provider)?;
    registry.bind_executable(&provider.caller, &binding)?;
    registry.seal()?.run(move |endpoint| async move {
        let _root = Marker("root");
        Ok(*endpoint
            .provider(binding)?
            .fetch_ref(&provider.caller, request.as_id())?
            .await?)
    })
}

fn stored<'db, C: Configuration>(
    db: &'db dyn Database,
    ingredient: &'db IngredientImpl<C>,
    id: Id,
) -> Option<&'db Memo<C>> {
    ingredient.get_memo_from_table_for(
        db.zalsa(),
        id,
        ingredient.memo_ingredient_index(db.zalsa(), id),
    )
}

fn ids(db: &dyn Database) -> Vec<Id> {
    compare::intern_ingredient_(db.zalsa())
        .entries(db.zalsa())
        .map(|entry| entry.key().key_index())
        .collect()
}

fn revisions<C: InternedQueryConfiguration>(_ingredient: &IngredientImpl<C>) -> usize {
    <C as crate::interned::Configuration>::REVISIONS.get()
}

fn idle(db: &dyn Database) {
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    assert!(!state().in_key.get());
}

fn complete(db: &TestDb, request: Request) -> bool {
    let outcome = try_with_attempt(db, 10_000, || run(db, request)).unwrap();
    let AttemptOutcome::Complete(Ok(result)) = outcome else {
        panic!("{outcome:?}")
    };
    idle(db);
    result
}

#[test]
fn registered_query_keys_create_and_reuse_canonical_memos() {
    let (state, _reset) = fixture();
    let db = TestDb::new();
    let first = Request::new(&db, 0, 0, None);
    assert!(ids(&db).is_empty());
    assert!(complete(&db, first));
    let id = state.delivered.borrow()[0];
    let ingredient = compare::fn_ingredient_(&db, db.zalsa());
    let memo = stored(&db, ingredient, id).unwrap();
    assert_eq!(memo.value(), Some(&true));
    let edges = stored(&db, caller::fn_ingredient_(&db, db.zalsa()), first.as_id())
        .unwrap()
        .header
        .origin()
        .inputs()
        .collect::<Vec<_>>();
    let argument_key = compare::intern_ingredient_(db.zalsa()).database_key_index(id);
    let function_key = ingredient.database_key_index(id);
    assert_ne!(
        argument_key.ingredient_index(),
        function_key.ingredient_index()
    );
    assert_eq!(argument_key.key_index(), function_key.key_index());
    assert!(edges.contains(&argument_key));
    // The leaf reads no mutable state. Persistence retains its otherwise unnecessary edge.
    assert_eq!(memo.header.revisions.durability, Durability::NEVER_CHANGE);
    assert_eq!(edges.contains(&function_key), cfg!(feature = "persistence"));
    assert!(complete(&db, Request::new(&db, 0, 0, None)));
    assert_eq!(&*state.delivered.borrow(), &[id, id]);
    assert_eq!(state.work.get(), 9 + 8);
    assert_eq!(state.leaves.get(), 1);
    assert_eq!(ids(&db), [id]);
    assert!(std::ptr::eq(memo, stored(&db, ingredient, id).unwrap()));
    assert!(compare(&db, Collision(0), 0));
    assert_eq!(state.ordinary.get(), 0);
    assert!(complete(&db, first));
    assert_eq!(state.attempted.get(), 2);
}

fn stale_fixture() -> (TestDb, Request, Id) {
    let mut db = TestDb::new();
    let request = Request::new(&db, 0, 0, None);
    let count = revisions(compare::fn_ingredient_(&db, db.zalsa()));
    let mut old = None;
    for index in 0..count {
        if index != 0 {
            request.set_left(&mut db).to(index as u32);
        }
        assert_eq!(caller(&db, request), index == 0);
        if index == 0 {
            old = Some(ids(&db)[0]);
        }
    }
    request.set_left(&mut db).to(count as u32);
    let new = Request::new(&db, count as u32, 0, None);
    (db, new, old.unwrap())
}

#[test]
fn registered_query_keys_reuse_stale_generations() {
    let (state, _reset) = fixture();
    let (db, request, old) = stale_fixture();
    state.events.borrow_mut().clear();
    assert!(!complete(&db, request));
    let new = state.delivered.borrow()[0];
    assert_eq!(new.index(), old.index());
    assert_eq!(new.generation(), old.generation() + 1);
    let old_key = compare::fn_ingredient_(&db, db.zalsa()).database_key_index(old);
    let new_key = compare::intern_ingredient_(db.zalsa()).database_key_index(new);
    let events = state.events.borrow();
    let discard = events
        .iter()
        .position(|event| *event == (KeyEvent::Discard, old_key))
        .unwrap();
    let reuse = events
        .iter()
        .position(|event| *event == (KeyEvent::Reuse, new_key))
        .unwrap();
    assert!(discard < reuse);
    drop(events);
    assert!(complete(&db, Request::new(&db, 0, 0, None)));
    assert_ne!(state.delivered.borrow()[1], old);
}

#[test]
fn fixed_zero_profile_trace() {
    for phase in ["cold", "hit", "reuse"] {
        let (state, _reset) = fixture();
        let (db, request) = if phase == "reuse" {
            let (db, request, _) = stale_fixture();
            (db, request)
        } else {
            let db = TestDb::new();
            if phase == "hit" {
                assert!(complete(&db, Request::new(&db, 0, 0, None)));
            }
            let request = Request::new(&db, 0, 0, None);
            (db, request)
        };
        *state.fixed_trace.borrow_mut() = Some(Vec::new());
        let result = complete(&db, request);
        let trace = state.fixed_trace.borrow_mut().take().unwrap();
        assert_eq!(result, phase != "reuse");
        for (ordinal, event) in trace.iter().enumerate() {
            match event {
                FixedTrace::Work(work) => eprintln!("FIXED_TRACE {phase} {ordinal} work {work:?}"),
                FixedTrace::Cancellation => eprintln!("FIXED_TRACE {phase} {ordinal} cancellation"),
                FixedTrace::Key(event, key) => {
                    eprintln!("FIXED_TRACE {phase} {ordinal} key {event:?} {key:?}")
                }
            }
        }
    }
}

#[test]
fn registered_query_key_chains_share_one_allowance() {
    // Measure through the first leaf delivery so the step includes the selected read's
    // storage work, including the dependency edge retained with persistence enabled.
    let (cold_step, key_step) = {
        let (state, _reset) = fixture();
        let db = TestDb::new();
        assert!(complete(&db, Request::new(&db, 0, 0, None)));
        (
            10_000 - state.first_leaf_allowance.get().unwrap(),
            10_000 - state.first_key_allowance.get().unwrap(),
        )
    };
    assert!(cold_step > 9);
    let reused_step = cold_step - 10;
    for fresh in [false, true] {
        let step = if fresh { cold_step } else { reused_step };
        for budget in [
            0,
            1,
            8,
            32,
            key_step - 1,
            key_step,
            key_step + 4,
            cold_step - 1,
            cold_step,
            cold_step + 3,
            cold_step + 4,
            cold_step + 5 * step + 3,
        ] {
            let (state, _reset) = fixture();
            let db = TestDb::new();
            let mut next = None;
            let mut requests = Vec::new();
            for index in (0..10).rev() {
                let request = Request::new(&db, if fresh { index as u32 } else { 0 }, 0, next);
                requests.push(request);
                next = Some(request);
            }
            let root = next.unwrap();
            assert!(ids(&db).is_empty());
            let stamp = Stamp::current(&db);
            let (outcome, observed) = observation::collect(|| {
                try_with_attempt(&db, budget, || {
                    let result = run(&db, root);
                    state.direct_error.set(result.err());
                    result
                })
            });
            assert_eq!(
                outcome,
                Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
            );
            assert_eq!(
                state.direct_error.get(),
                Some(RunError::Refused(Incomplete::Allowance))
            );
            // Each caller converts its input for three units. A cold key needs quotation
            // quanta one and two; a hit needs only one. Their preparation costs thirteen
            // and eight units, respectively, before the dependency read. A cold leaf also
            // converts its tuple for four units and publishes for one. Each step then
            // admits the selected read before the next caller starts.
            let (completed_steps, remainder) = if fresh || budget < cold_step {
                (budget / cold_step, budget % cold_step)
            } else {
                (
                    1 + (budget - cold_step) / reused_step,
                    (budget - cold_step) % reused_step,
                )
            };
            let cold = fresh || completed_steps == 0;
            let current_key_step = key_step - if cold { 0 } else { 5 };
            let attempted = completed_steps + usize::from(remainder >= 3);
            let delivered = completed_steps + usize::from(remainder >= current_key_step);
            assert_eq!(state.attempted.get(), attempted);
            assert_eq!(state.delivered.borrow().len(), delivered);
            let key_work = if cold {
                &[1, 1, 1, 1, 1, 1, 1, 2, 1, 1, 2][..]
            } else {
                &[1, 1, 1, 1, 1, 1, 1, 1][..]
            };
            let mut spent = 3;
            let partial_scalar_work = key_work
                .iter()
                .filter(|&&units| {
                    spent += units;
                    spent <= remainder && units == 1
                })
                .count();
            let completed_scalar_work = if fresh {
                9 * completed_steps
            } else {
                8 * completed_steps + usize::from(completed_steps != 0)
            };
            assert_eq!(state.work.get(), completed_scalar_work + partial_scalar_work);
            assert_eq!(
                state.leaves.get(),
                if fresh {
                    completed_steps + usize::from(remainder >= current_key_step + 4)
                } else {
                    usize::from(budget >= key_step + 4)
                }
            );
            assert_eq!(observed.max_active_polls, 1);
            for request in requests {
                assert!(
                    stored(
                        &db,
                        caller::fn_ingredient_(&db, db.zalsa()),
                        request.as_id()
                    )
                    .is_none()
                );
            }
            idle(&db);
            assert_eq!(Stamp::current(&db), stamp);
            let result = complete(&db, root);
            assert_eq!(result, caller(&db, root));
            assert_eq!(Stamp::current(&db), stamp);
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Fault {
    Admission,
    ReadWork,
    ReadBytes,
    RevisionReadWork,
    Intern,
    DiscardReenter,
    DiscardPanic,
}

#[derive(Debug)]
struct Payload(Arc<()>);

struct Snapshot {
    stage: &'static str,
    frame: Option<DatabaseKeyIndex>,
    claimed: bool,
    detached: usize,
    reason: Option<Incomplete>,
    reads: Option<ReadState>,
}

struct Hook {
    db: &'static TestDb,
    caller: DatabaseKeyIndex,
    old: Option<DatabaseKeyIndex>,
    replacement: (Collision, u32),
    fault: Fault,
    endpoint: RefCell<Option<TaskEndpoint<'static, 'static>>>,
    fired: Cell<bool>,
    new_id: Cell<Option<Id>>,
    installed_memo: Cell<usize>,
    detached: Rc<detached_observation::State>,
    payload: Arc<()>,
    notes: RefCell<Vec<Snapshot>>,
}

impl Hook {
    fn note(&self, stage: &'static str) {
        let frame = self.db.zalsa_local().active_query().map(|(key, _)| key);
        let claimed = matches!(
            self.db
                .zalsa()
                .lookup_ingredient(self.caller.ingredient_index())
                .as_function()
                .unwrap()
                .sync_table()
                .peek_claim(self.db.zalsa(), self.caller.key_index(), Reentrancy::Deny),
            ClaimResult::Cycle { .. }
        );
        self.notes.borrow_mut().push(Snapshot {
            stage,
            frame,
            claimed,
            detached: self.detached.live.get(),
            reason: attempt_probe::current().and_then(|current| current.reason()),
            reads: self
                .db
                .zalsa_local()
                .try_with_query_stack(|stack| stack.last().map(|query| query.read_state()))
                .flatten(),
        });
    }

    fn event(&self, event: (KeyEvent, DatabaseKeyIndex)) {
        if self.fault == Fault::DiscardReenter
            && event.0 == KeyEvent::Reuse
            && event.1.ingredient_index()
                == compare::intern_ingredient_(self.db.zalsa()).ingredient_index()
            && Some(event.1.key_index()) == self.new_id.get()
        {
            assert_eq!(self.detached.live.get(), 1);
            self.note("reuse");
        }
        if self.fired.get() {
            return;
        }
        let selected = match self.fault {
            Fault::Intern => {
                event.0 == KeyEvent::Intern
                    && event.1.ingredient_index()
                        == compare::intern_ingredient_(self.db.zalsa()).ingredient_index()
            }
            Fault::DiscardReenter | Fault::DiscardPanic => {
                event.0 == KeyEvent::Discard && Some(event.1) == self.old
            }
            Fault::Admission | Fault::ReadWork | Fault::ReadBytes | Fault::RevisionReadWork => {
                false
            }
        };
        if !selected {
            return;
        }
        if self.fault == Fault::Intern {
            self.new_id.set(Some(event.1.key_index()));
        } else {
            // The old table is detached before this callback can reacquire the live slot.
            self.fired.set(true);
            let id = compare::intern_ingredient_(self.db.zalsa()).intern_id(
                self.db.zalsa(),
                self.db.zalsa_local(),
                self.replacement,
                |_, fields| fields,
            );
            self.new_id.set(Some(id));
            assert!(
                stored(
                    self.db,
                    compare::fn_ingredient_(self.db, self.db.zalsa()),
                    id
                )
                .is_none()
            );
            self.fired.set(false);
        }
        if self.fault == Fault::DiscardReenter {
            self.fired.set(true);
            self.note("event");
            assert_eq!(
                compare(self.db, self.replacement.0, self.replacement.1),
                value(self.replacement.0, self.replacement.1)
            );
            let memo = stored(
                self.db,
                compare::fn_ingredient_(self.db, self.db.zalsa()),
                self.new_id.get().unwrap(),
            )
            .unwrap();
            self.installed_memo.set(std::ptr::from_ref(memo).addr());
        } else {
            self.fail();
        }
    }

    fn fail(&self) {
        assert!(!self.fired.replace(true));
        self.note("event");
        let endpoint = self.endpoint.borrow_mut().take().unwrap();
        let marker = Marker("child");
        let _reply = endpoint
            .demand::<(), _>(move || async move {
                let _marker = marker;
                panic!("rejected key request polled its queued child");
            })
            .unwrap();
        if self.fault == Fault::DiscardPanic {
            panic_any(Payload(self.payload.clone()));
        }
        attempt_probe::report_incomplete(self.db, Incomplete::Interrupted);
    }
}

fn hook_fixture(fault: Fault) -> (&'static TestDb, Request, Rc<Hook>, impl Drop) {
    let (db, request, old) = if matches!(fault, Fault::DiscardPanic | Fault::DiscardReenter) {
        let (db, request, old) = stale_fixture();
        (db, request, Some(old))
    } else {
        let mut db = TestDb::new();
        let request = Request::builder(0, 0, None)
            .durability(if fault == Fault::RevisionReadWork {
                Durability::HIGH
            } else {
                Durability::LOW
            })
            .new(&db);
        if fault == Fault::RevisionReadWork {
            db.synthetic_write(Durability::LOW);
        }
        (db, request, None)
    };
    // Complete revision writes before giving the event fixture its real static endpoint lifetime.
    let db = Box::leak(Box::new(db));
    let detached = Rc::new(detached_observation::State::default());
    let observer = detached_observation::install(detached.clone());
    let hook = Rc::new(Hook {
        db,
        caller: caller::fn_ingredient_(db, db.zalsa()).database_key_index(request.as_id()),
        old: old.map(|id| compare::fn_ingredient_(db, db.zalsa()).database_key_index(id)),
        replacement: (Collision(request.left(db)), request.right(db)),
        fault,
        endpoint: RefCell::new(None),
        fired: Cell::new(false),
        new_id: Cell::new(None),
        installed_memo: Cell::new(0),
        detached,
        payload: Arc::new(()),
        notes: RefCell::new(Vec::new()),
    });
    state().events.borrow_mut().clear();
    state().journal.borrow_mut().clear();
    *state().hook.borrow_mut() = Some(hook.clone());
    (db, request, hook, observer)
}

fn check_failed_hook(hook: &Hook, request: Request) {
    assert!(hook.fired.get());
    assert!(state().delivered.borrow().is_empty());
    assert_eq!(state().leaves.get(), 0);
    assert!(
        stored(
            hook.db,
            caller::fn_ingredient_(hook.db, hook.db.zalsa()),
            request.as_id()
        )
        .is_none()
    );
    assert_eq!(&*state().journal.borrow(), &["child", "caller", "root"]);
    let notes = hook.notes.borrow();
    for stage in ["event", "child"] {
        let note = notes.iter().find(|note| note.stage == stage).unwrap();
        assert_eq!(note.frame, Some(hook.caller));
        assert!(note.claimed);
        assert_eq!(
            note.detached,
            usize::from(hook.fault == Fault::DiscardPanic)
        );
        if stage == "child" && hook.fault != Fault::DiscardPanic {
            assert_eq!(note.reason, Some(Incomplete::Interrupted));
        }
    }
    assert_eq!(
        notes
            .iter()
            .find(|note| note.stage == "caller")
            .unwrap()
            .detached,
        0
    );
    assert_eq!(hook.detached.live.get(), 0);
    assert_eq!(
        hook.detached.acquired.get(),
        usize::from(hook.fault == Fault::DiscardPanic)
    );
    assert_eq!(hook.detached.retired.get(), hook.detached.acquired.get());
    idle(hook.db);
}

#[test]
fn key_request_refusal_and_committed_event_retry() {
    for fault in [Fault::Admission, Fault::Intern] {
        let (state, _reset) = fixture();
        let (db, request, hook, _observer) = hook_fixture(fault);
        let stamp = Stamp::current(db);
        let outcome = try_with_attempt(db, 10_000, || {
            let result = run_hook(db, request);
            state.direct_error.set(result.err());
            result
        });
        assert_eq!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        );
        assert_eq!(
            state.direct_error.get(),
            Some(RunError::Refused(Incomplete::Interrupted))
        );
        check_failed_hook(&hook, request);
        assert_eq!(ids(db).len(), usize::from(fault == Fault::Intern));
        if let Some(id) = hook.new_id.get() {
            assert!(stored(db, compare::fn_ingredient_(db, db.zalsa()), id).is_none());
        }
        *state.hook.borrow_mut() = None;
        state.events.borrow_mut().clear();
        assert!(complete(db, request));
        if let Some(id) = hook.new_id.get() {
            assert_eq!(state.delivered.borrow()[0], id);
            assert!(
                !state
                    .events
                    .borrow()
                    .iter()
                    .any(|event| event.0 == KeyEvent::Intern)
            );
        }
        assert!(complete(db, request));
        assert_eq!(Stamp::current(db), stamp);
    }
}

#[test]
fn interned_key_read_refusal_preserves_recipient_and_retries() {
    for fault in [Fault::ReadWork, Fault::ReadBytes, Fault::RevisionReadWork] {
        let (state, _reset) = fixture();
        let (db, request, hook, _observer) = hook_fixture(fault);
        let stamp = Stamp::current(db);
        assert_eq!(
            try_with_attempt(db, 10_000, || run_hook(db, request)),
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        );
        check_failed_hook(&hook, request);
        assert!(state.events.borrow().is_empty());
        let notes = hook.notes.borrow();
        let request_state = notes.iter().find(|note| note.stage == "request").unwrap();
        let boundary = notes.iter().find(|note| note.stage == "event").unwrap();
        let child = notes.iter().find(|note| note.stage == "child").unwrap();
        assert!(boundary.reads.is_some());
        assert_eq!(request_state.reads, boundary.reads);
        assert_eq!(boundary.reads, child.reads);
        drop(notes);

        let [id] = ids(db).try_into().unwrap();
        *state.hook.borrow_mut() = None;
        assert!(complete(db, request));
        assert_eq!(state.delivered.borrow().as_slice(), &[id]);
        assert!(state.events.borrow().is_empty());
        let memo = stored(db, caller::fn_ingredient_(db, db.zalsa()), request.as_id()).unwrap();
        let interned = compare::intern_ingredient_(db.zalsa()).database_key_index(id);
        assert_eq!(
            memo.header.origin().inputs().any(|input| input == interned),
            fault != Fault::RevisionReadWork
        );
        assert_eq!(
            memo.header.revisions.changed_at,
            db.zalsa().current_revision()
        );
        assert_eq!(Stamp::current(db), stamp);
        assert!(complete(db, request));
    }
}

#[test]
fn discard_callback_reentry_keeps_the_new_generation_memo() {
    let (state, _reset) = fixture();
    let (db, request, hook, _observer) = hook_fixture(Fault::DiscardReenter);
    assert_eq!(
        try_with_attempt(db, 10_000, || run_hook(db, request)),
        Ok(AttemptOutcome::Complete(Ok(false)))
    );
    assert!(hook.fired.get());
    let id = hook.new_id.get().unwrap();
    assert_eq!(state.delivered.borrow()[0], id);
    assert_eq!(state.leaves.get(), 0);
    assert_eq!(
        std::ptr::from_ref(stored(db, compare::fn_ingredient_(db, db.zalsa()), id).unwrap()).addr(),
        hook.installed_memo.get()
    );
    assert_eq!(
        hook.notes
            .borrow()
            .iter()
            .find(|note| note.stage == "event")
            .unwrap()
            .detached,
        1
    );
    assert_eq!(
        hook.notes
            .borrow()
            .iter()
            .find(|note| note.stage == "reuse")
            .unwrap()
            .detached,
        1
    );
    assert_eq!(
        (
            hook.detached.live.get(),
            hook.detached.acquired.get(),
            hook.detached.retired.get()
        ),
        (0, 1, 1)
    );
    let events = state.events.borrow();
    let discard = events
        .iter()
        .position(|event| event.0 == KeyEvent::Discard && Some(event.1) == hook.old)
        .unwrap();
    let reuse = events
        .iter()
        .position(|event| event.0 == KeyEvent::Reuse && event.1.key_index() == id)
        .unwrap();
    assert!(discard < reuse);
    idle(db);
}

#[test]
fn discard_panic_retains_detached_storage_until_child_cleanup() {
    let (state, _reset) = fixture();
    let (db, request, hook, _observer) = hook_fixture(Fault::DiscardPanic);
    let stamp = Stamp::current(db);
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        try_with_attempt(db, 10_000, || run_hook(db, request))
    }));
    let payload = outcome.expect_err("discard must retain the original native panic");
    assert!(Arc::ptr_eq(
        &payload.downcast_ref::<Payload>().unwrap().0,
        &hook.payload
    ));
    check_failed_hook(&hook, request);
    let id = hook.new_id.get().unwrap();
    assert!(ids(db).contains(&id));
    *state.hook.borrow_mut() = None;
    assert!(!complete(db, request));
    assert_eq!(state.delivered.borrow()[0], id);
    assert!(!complete(db, request));
    assert_eq!(Stamp::current(db), stamp);
}

#[crate::tracked]
struct Produced<'db> {
    #[returns(copy)]
    value: u32,
}

#[cfg(feature = "accumulator")]
#[crate::accumulator]
struct Warning {
    _value: u32,
}

#[crate::tracked(returns(copy), attempt = CompleteOnly)]
fn historical(db: &dyn Database, left: Collision, kind: u32) -> bool {
    if kind == 0 {
        let _output = Produced::new(db, left.0);
    }
    #[cfg(feature = "accumulator")]
    if kind == 1 {
        Warning { _value: left.0 }.accumulate(db);
    }
    true
}

#[crate::tracked(returns(copy), attempt = CompleteOnly)]
fn historical_caller(db: &dyn Database, request: Request) -> bool {
    historical(db, Collision(request.left(db)), request.right(db))
}

fn historical_profile<C>(
    db: &TestDb,
    ingredient: &IngredientImpl<C>,
    replacement: (Collision, u32),
    old: Id,
    message: &'static str,
) where
    C: InternedQueryConfiguration
        + for<'a> crate::interned::Configuration<Fields<'a> = (Collision, u32)>
        + for<'a> Configuration<DbView = dyn Database, Output<'a> = bool>,
{
    let argument = C::argument_ingredient(db.zalsa());
    let index = ingredient
        .query_key_memo_index(db.zalsa(), argument)
        .unwrap_or_else(|error| panic!("{}", error.message()));
    let before = argument
        .entries(db.zalsa())
        .map(|entry| entry.key())
        .collect::<Vec<_>>();
    let memo = stored(db, ingredient, old).unwrap();
    let pointer = std::ptr::from_ref(memo).addr();
    let produced = memo.header.revisions.tracked_struct_ids().to_vec();
    if replacement.1 == 0 {
        assert!(!memo.header.outputs_are_empty());
        // New tracked structs live in the identity map, independently of output edges.
        assert_eq!(produced.len(), 1);
        assert!(memo.header.origin().outputs().next().is_none());
    }
    #[cfg(feature = "accumulator")]
    if replacement.1 == 1 {
        assert!(memo.header.outputs_are_empty());
        assert!(memo.header.revisions.accumulated().is_some());
    }
    let detached = Rc::new(detached_observation::State::default());
    let _observer = detached_observation::install(detached.clone());
    for _ in 0..2 {
        let result = argument.prepare_intern(
            db.zalsa(),
            db.zalsa_local(),
            replacement,
            |_, fields| fields,
            |table| check_passive_key_memo::<C>(table, index),
        );
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("historical output metadata was accepted"),
        };
        assert_eq!(error.message(), message);
        assert_eq!(
            argument
                .entries(db.zalsa())
                .map(|entry| entry.key())
                .collect::<Vec<_>>(),
            before
        );
        assert_eq!(
            std::ptr::from_ref(stored(db, ingredient, old).unwrap()).addr(),
            pointer
        );
        assert_eq!(stored(db, ingredient, old).unwrap().value(), Some(&true));
        assert_eq!(
            stored(db, ingredient, old)
                .unwrap()
                .header
                .revisions
                .tracked_struct_ids(),
            produced
        );
        assert_eq!(detached.acquired.get(), 0);
    }
}

#[test]
fn historical_query_key_outputs_are_rejected_before_reuse() {
    let kinds: &[(u32, &str)] = &[
        (0, "fixed query key memo has tracked outputs"),
        #[cfg(feature = "accumulator")]
        (1, "fixed query key memo has direct accumulators"),
    ];
    for &(kind, message) in kinds {
        let (_state, _reset) = fixture();
        let mut db = TestDb::new();
        let request = Request::new(&db, 0, kind, None);
        let count = revisions(historical::fn_ingredient_(&db, db.zalsa()));
        let mut old = None;
        for index in 0..count {
            if index != 0 {
                request.set_left(&mut db).to(index as u32);
            }
            assert!(historical_caller(&db, request));
            if index == 0 {
                old = historical::intern_ingredient_(db.zalsa())
                    .entries(db.zalsa())
                    .next()
                    .map(|entry| entry.key().key_index());
            }
        }
        request.set_left(&mut db).to(count as u32);
        let old = old.unwrap();
        historical_profile(
            &db,
            historical::fn_ingredient_(&db, db.zalsa()),
            (Collision(count as u32), kind),
            old,
            message,
        );
        assert!(historical_caller(&db, request));
        let reused = historical::intern_ingredient_(db.zalsa())
            .entries(db.zalsa())
            .map(|entry| entry.key().key_index())
            .find(|id| id.index() == old.index())
            .unwrap();
        assert_eq!(reused.generation(), old.generation() + 1);
        idle(&db);
    }
}

mod profiled_keys {
    use super::super::super::key_run::key_layout_for_tests;
    use super::super::super::registration::{
        CallableRoute, CallableRouteProvider, PassiveMemoProfile, QueryKeyProfile, QueryKeys,
    };
    use super::*;
    use crate::function::memo::inspect_passive_singleton;

    #[derive(Clone, Copy, Debug, Eq, PartialEq, crate::SalsaValue)]
    struct InlineText(&'static str);

    impl Hash for InlineText {
        fn hash<H: Hasher>(&self, state: &mut H) {
            // Unequal texts share a shard so actual stale-generation reuse is deterministic.
            state.write_u32(0);
        }
    }

    #[crate::input]
    struct ProfiledRequest {
        #[returns(copy)]
        text: InlineText,
        #[returns(copy)]
        left: u32,
        #[returns(copy)]
        right: u32,
    }

    fn make_value(text: InlineText, left: u32, right: u32) -> Vec<u32> {
        let mut value = Vec::with_capacity(text.0.len() + 2);
        value.extend(text.0.bytes().map(u32::from));
        value.extend([left, right]);
        value
    }

    #[crate::tracked(returns(ref), attempt = ReturnOnly)]
    fn profiled_value(_db: &dyn Database, text: InlineText, left: u32, right: u32) -> Vec<u32> {
        state().ordinary.set(state().ordinary.get() + 1);
        make_value(text, left, right)
    }

    #[crate::tracked(returns(copy), attempt = ReturnOnly)]
    fn profiled_caller(db: &dyn Database, request: ProfiledRequest) -> usize {
        let tuple = (request.text(db), request.left(db), request.right(db));
        let first = profiled_value(db, tuple.0, tuple.1, tuple.2);
        let second = profiled_value(db, tuple.0, tuple.1, tuple.2);
        assert!(std::ptr::eq(first, second));
        second.len()
    }

    struct Profile;

    impl<C> PassiveMemoProfile<C> for Profile
    where
        C: for<'db> Configuration<Output<'db> = Vec<u32>>,
    {
        fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
            Some(output.len())
        }

        fn retired_output_work_bounded<'db>(
            output: &C::Output<'db>,
            fuel: &mut QuoteFuel,
        ) -> Result<usize, QuoteError> {
            fuel.consume(1)?;
            <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
        }
    }

    // InlineText hashes/compares finite static bytes; it deliberately is not FixedQueryFields.
    // Vec<u32> retirement frees one allocation and has no element callbacks or semantic work.
    impl<C> QueryKeyProfile<C> for Profile
    where
        C: InternedQueryConfiguration
            + for<'db> crate::interned::Configuration<Fields<'db> = (InlineText, u32, u32)>
            + for<'db> Configuration<Output<'db> = Vec<u32>>,
    {
        fn input_work<'db>(
            input: &<C as crate::interned::Configuration>::Fields<'db>,
        ) -> Option<usize> {
            Some(input.0.0.len())
        }

        fn input_work_bounded<'db>(
            input: &<C as crate::interned::Configuration>::Fields<'db>,
            fuel: &mut QuoteFuel,
        ) -> Result<usize, QuoteError> {
            fuel.consume(1)?;
            <Self as QueryKeyProfile<C>>::input_work(input).ok_or(QuoteError::Overflow)
        }
    }

    #[derive(Default)]
    struct Observed {
        leaves: Cell<usize>,
        callers: Cell<usize>,
        ids: Cell<Option<[Id; 2]>>,
        same_value: Cell<bool>,
        token_bytes: Cell<usize>,
        work: RefCell<Vec<ExecutionWork>>,
        key_work: RefCell<Vec<usize>>,
        accepted: Cell<usize>,
        last_id: Cell<Option<Id>>,
        future_layout: Cell<Option<(usize, usize)>>,
    }

    impl ExecutionAdmission for Observed {
        fn admit(&self, work: ExecutionWork) -> RunResult<()> {
            self.work.borrow_mut().push(work);
            if state().in_key.get()
                && let ExecutionWork::Work { units } = work
            {
                self.key_work.borrow_mut().push(units);
                let hook = active_hook();
                if let Some(hook) = hook
                    && hook.fault == ProfileFault::RetirementWork
                    && units == 8
                    // Quotation also admits a quantum of 8. Retirement follows the input's 13 units.
                    && self.key_work.borrow().ends_with(&[13, 8])
                    && !hook.fired.get()
                    && hook
                        .db
                        .zalsa_local()
                        .active_query()
                        .is_some_and(|(key, _)| key == hook.caller)
                {
                    hook.fail();
                    return Err(RunError::Refused(Incomplete::Interrupted));
                }
            }
            ADMISSION.admit(work)
        }
    }

    async fn intern<'call, 'run: 'call, 'db: 'run, C, P>(
        endpoint: &'call TaskEndpoint<'run, 'db>,
        keys: &'call QueryKeys<'db, C, P>,
        tuple: (InlineText, u32, u32),
        observed: &'call Observed,
    ) -> Id
    where
        C: InternedQueryConfiguration
            + for<'a> crate::interned::Configuration<Fields<'a> = (InlineText, u32, u32)>,
        P: QueryKeyProfile<C> + 'call,
    {
        let _scope = KeyScope::new();
        let future = endpoint.intern_query_key(keys, tuple);
        observed
            .future_layout
            .set(Some((size_of_val(&future), align_of_val(&future))));
        let id = future.await;
        observed.accepted.set(observed.accepted.get() + 1);
        observed.last_id.set(Some(id));
        id
    }

    async fn read_request(
        endpoint: &TaskEndpoint<'_, '_>,
        db: &dyn Database,
        request: ProfiledRequest,
        observed: &Observed,
    ) -> (InlineText, u32, u32) {
        endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                observed.callers.set(observed.callers.get() + 1);
                Ok((request.text(db), request.left(db), request.right(db)))
            })
            .await
    }

    async fn leaf_body(
        endpoint: &TaskEndpoint<'_, '_>,
        tuple: (InlineText, u32, u32),
        observed: &Observed,
    ) -> Vec<u32> {
        endpoint
            .local_call(|| {
                let units = tuple
                    .0
                    .0
                    .len()
                    .checked_add(2)
                    .ok_or(RunError::Contract("profiled fixture size overflow"))?;
                endpoint.admit_work(units)?;
                let bytes = units
                    .checked_mul(size_of::<u32>())
                    .ok_or(RunError::Contract("profiled fixture size overflow"))?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: bytes,
                })?;
                observed.leaves.set(observed.leaves.get() + 1);
                Ok(make_value(tuple.0, tuple.1, tuple.2))
            })
            .await
    }

    fn record_result(
        observed: &Observed,
        ids: [Id; 2],
        first: &Vec<u32>,
        second: &Vec<u32>,
    ) -> usize {
        observed.ids.set(Some(ids));
        observed.same_value.set(std::ptr::eq(first, second));
        second.len()
    }

    fn native_value_quote<'call, 'db, C>(
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        C: for<'a> Configuration<
                DbView = dyn Database,
                Input<'a> = (InlineText, u32, u32),
                Output<'a> = Vec<u32>,
            >,
    {
        let work = match operation {
            // Clone copies the static string reference and two integers, with no owned payload.
            NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => 3,
            NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => {
                return Err(RunError::Contract("profiled key requires retained tuple"));
            }
            // Vec equality can inspect every u32 in both actual operands, including old memos.
            NativeValueOperation::Comparison { left, right } => left
                .len()
                .checked_add(right.len())
                .and_then(|work| work.checked_add(1))
                .ok_or(RunError::Contract("profiled value comparison overflow"))?,
        };
        Ok(NativeValueQuote {
            work,
            requested_bytes: 0,
            cleanup_work: 0,
        })
    }

    struct Leaf<'obs>(&'obs Observed);

    impl<'run, 'db: 'run, C> ExecutableRouteProvider<'run, 'db, C> for Leaf<'run>
    where
        C: for<'a> Configuration<
                DbView = dyn Database,
                Input<'a> = (InlineText, u32, u32),
                Output<'a> = Vec<u32>,
            >,
    {
        async fn native_value<'call>(
            &'run self,
            _context: ProviderContext<'run, 'db, Self>,
            _db: &'db dyn Database,
            operation: NativeValueOperation<'call, 'db, C>,
        ) -> RunResult<NativeValueQuote>
        where
            'run: 'call,
        {
            native_value_quote(operation)
        }

        async fn body(
            &'run self,
            context: ProviderContext<'run, 'db, Self>,
            _db: &'db dyn Database,
            input: (InlineText, u32, u32),
        ) -> RunResult<Vec<u32>> {
            Ok(leaf_body(&context.endpoint(), input, self.0).await)
        }

        async fn initial(
            &'run self,
            context: ProviderContext<'run, 'db, Self>,
            db: &'db dyn Database,
            id: Id,
            input: (InlineText, u32, u32),
        ) -> RunResult<Vec<u32>> {
            Ok(context
                .endpoint()
                .local_call(|| Ok(C::cycle_initial(db, id, input)))
                .await)
        }

        async fn recover<'call>(
            &'run self,
            context: ProviderContext<'run, 'db, Self>,
            db: &'db dyn Database,
            cycle: &'call Cycle<'call>,
            last: &'call Vec<u32>,
            value: Vec<u32>,
            input: (InlineText, u32, u32),
        ) -> RunResult<Vec<u32>>
        where
            'run: 'call,
        {
            Ok(context
                .endpoint()
                .local_call(|| Ok(C::recover_from_cycle(db, cycle, last, value, input)))
                .await)
        }
    }

    impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for Leaf<'run>
    where
        C: for<'a> Configuration<
                DbView = dyn Database,
                Input<'a> = (InlineText, u32, u32),
                Output<'a> = Vec<u32>,
            >,
    {
        async fn native_value<'call>(
            &'call self,
            _endpoint: TaskEndpoint<'run, 'db>,
            _db: &'db dyn Database,
            operation: NativeValueOperation<'call, 'db, C>,
        ) -> RunResult<NativeValueQuote>
        where
            'run: 'call,
        {
            native_value_quote(operation)
        }

        async fn body<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            _db: &'db dyn Database,
            input: (InlineText, u32, u32),
        ) -> RunResult<Vec<u32>>
        where
            'run: 'call,
        {
            Ok(leaf_body(&endpoint, input, self.0).await)
        }

        async fn initial<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            id: Id,
            input: (InlineText, u32, u32),
        ) -> RunResult<Vec<u32>>
        where
            'run: 'call,
        {
            Ok(endpoint
                .local_call(|| Ok(C::cycle_initial(db, id, input)))
                .await)
        }

        async fn recover<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            cycle: &'call Cycle<'call>,
            last: &'call Vec<u32>,
            value: Vec<u32>,
            input: (InlineText, u32, u32),
        ) -> RunResult<Vec<u32>>
        where
            'run: 'call,
        {
            Ok(endpoint
                .local_call(|| Ok(C::recover_from_cycle(db, cycle, last, value, input)))
                .await)
        }
    }

    struct BorrowedCaller<'run, 'db: 'run, C: InternedQueryConfiguration, P = Profile> {
        target: Route<'db, C>,
        keys: QueryKeys<'db, C, P>,
        leaf: ProviderBinding<'run, Leaf<'run>>,
        observed: &'run Observed,
        arm: fn(TaskEndpoint<'run, 'db>),
    }

    impl<'run, 'db: 'run, C, D, P> ExecutableRouteProvider<'run, 'db, D>
        for BorrowedCaller<'run, 'db, C, P>
    where
        C: InternedQueryConfiguration
            + for<'a> crate::interned::Configuration<Fields<'a> = (InlineText, u32, u32)>
            + for<'a> Configuration<DbView = dyn Database, Output<'a> = Vec<u32>>,
        D: for<'a> Configuration<
                DbView = dyn Database,
                Input<'a> = ProfiledRequest,
                Output<'a> = usize,
            >,
        P: QueryKeyProfile<C> + 'run,
    {
        // ProfiledRequest conversion constructs a handle; output equality compares usize.
        fixture_native_value!(executable, 'run, 'db, D, 1);

        async fn body(
            &'run self,
            context: ProviderContext<'run, 'db, Self>,
            db: &'db dyn Database,
            input: ProfiledRequest,
        ) -> RunResult<usize> {
            let _caller = Marker("caller");
            let endpoint = context.endpoint();
            (self.arm)(endpoint.clone());
            let tuple = read_request(&endpoint, db, input, self.observed).await;
            let leaf = endpoint.provider(self.leaf.clone())?;
            let first_id = intern(&endpoint, &self.keys, tuple, self.observed).await;
            let first = endpoint
                .child_call(|| async { leaf.fetch_ref(&self.target, first_id)?.await })
                .await;
            let second_id = intern(&endpoint, &self.keys, tuple, self.observed).await;
            let second = endpoint
                .child_call(|| async { leaf.fetch_ref(&self.target, second_id)?.await })
                .await;
            Ok(record_result(
                self.observed,
                [first_id, second_id],
                first,
                second,
            ))
        }

        async fn initial(
            &'run self,
            context: ProviderContext<'run, 'db, Self>,
            db: &'db dyn Database,
            id: Id,
            input: ProfiledRequest,
        ) -> RunResult<usize> {
            Ok(context
                .endpoint()
                .local_call(|| Ok(D::cycle_initial(db, id, input)))
                .await)
        }

        async fn recover<'call>(
            &'run self,
            context: ProviderContext<'run, 'db, Self>,
            db: &'db dyn Database,
            cycle: &'call Cycle<'call>,
            last: &'call usize,
            value: usize,
            input: ProfiledRequest,
        ) -> RunResult<usize>
        where
            'run: 'call,
        {
            Ok(context
                .endpoint()
                .local_call(|| Ok(D::recover_from_cycle(db, cycle, last, value, input)))
                .await)
        }
    }

    struct CallableCaller<'run, 'db: 'run, C: InternedQueryConfiguration> {
        target: CallableRoute<'run, 'db, C>,
        keys: QueryKeys<'db, C, Profile>,
        observed: &'run Observed,
    }

    impl<'run, 'db: 'run, C, D> CallableRouteProvider<'run, 'db, D> for CallableCaller<'run, 'db, C>
    where
        C: InternedQueryConfiguration
            + for<'a> crate::interned::Configuration<Fields<'a> = (InlineText, u32, u32)>
            + for<'a> Configuration<DbView = dyn Database, Output<'a> = Vec<u32>>,
        D: for<'a> Configuration<
                DbView = dyn Database,
                Input<'a> = ProfiledRequest,
                Output<'a> = usize,
            >,
    {
        // ProfiledRequest conversion constructs a handle; output equality compares usize.
        fixture_native_value!(callable, 'run, 'db, D, 1);

        async fn body<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            input: ProfiledRequest,
        ) -> RunResult<usize>
        where
            'run: 'call,
        {
            let tuple = read_request(&endpoint, db, input, self.observed).await;
            let first_id = intern(&endpoint, &self.keys, tuple, self.observed).await;
            let first = endpoint
                .child_call(|| async { endpoint.fetch_ref(&self.target, first_id)?.await })
                .await;
            let second_id = intern(&endpoint, &self.keys, tuple, self.observed).await;
            let second = endpoint
                .child_call(|| async { endpoint.fetch_ref(&self.target, second_id)?.await })
                .await;
            Ok(record_result(
                self.observed,
                [first_id, second_id],
                first,
                second,
            ))
        }

        async fn initial<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            id: Id,
            input: ProfiledRequest,
        ) -> RunResult<usize>
        where
            'run: 'call,
        {
            Ok(endpoint
                .local_call(|| Ok(D::cycle_initial(db, id, input)))
                .await)
        }

        async fn recover<'call>(
            &'call self,
            endpoint: TaskEndpoint<'run, 'db>,
            db: &'db dyn Database,
            cycle: &'call Cycle<'call>,
            last: &'call usize,
            value: usize,
            input: ProfiledRequest,
        ) -> RunResult<usize>
        where
            'run: 'call,
        {
            Ok(endpoint
                .local_call(|| Ok(D::recover_from_cycle(db, cycle, last, value, input)))
                .await)
        }
    }

    fn run_borrowed(
        db: &TestDb,
        request: ProfiledRequest,
        observed: &Observed,
    ) -> RunResult<usize> {
        run_for::<_, _, Profile>(
            db,
            request,
            observed,
            profiled_value::fn_ingredient_(db, db.zalsa()),
            profiled_caller::fn_ingredient_(db, db.zalsa()),
        )
    }

    fn run_for<'db, C, D, P>(
        db: &'db TestDb,
        request: ProfiledRequest,
        observed: &Observed,
        target: &'db IngredientImpl<C>,
        caller_ingredient: &'db IngredientImpl<D>,
    ) -> RunResult<usize>
    where
        C: InternedQueryConfiguration
            + for<'a> crate::interned::Configuration<Fields<'a> = (InlineText, u32, u32)>
            + for<'a> Configuration<DbView = dyn Database, Output<'a> = Vec<u32>>,
        D: for<'a> Configuration<
                DbView = dyn Database,
                Input<'a> = ProfiledRequest,
                Output<'a> = usize,
            >,
        P: QueryKeyProfile<C>,
    {
        let leaf = Leaf(observed);
        let caller;
        let mut registry = RegistryBuilder::new(db, observed)?;
        let target = registry.reserve(db as &dyn Database, target)?;
        let keys = registry.query_keys::<_, P>(&target)?;
        observed.token_bytes.set(size_of_val(&keys));
        let caller_route = registry.reserve(db as &dyn Database, caller_ingredient)?;
        let binding = registry.provider(&leaf)?;
        registry.bind_executable(&target, &binding)?;
        caller = BorrowedCaller {
            target,
            keys,
            leaf: binding,
            observed,
            arm: no_hook,
        };
        let binding = registry.provider(&caller)?;
        registry.bind_executable(&caller_route, &binding)?;
        registry.seal()?.run(move |endpoint| async move {
            let _root = Marker("root");
            Ok::<usize, RunError>(
                *endpoint
                    .provider(binding)?
                    .fetch_ref(&caller_route, request.as_id())?
                    .await?,
            )
        })
    }

    fn run_callable(
        db: &TestDb,
        request: ProfiledRequest,
        observed: &Observed,
    ) -> RunResult<usize> {
        let mut registry = RegistryBuilder::new(db, observed)?;
        let target = registry.reserve_callable(
            db as &dyn Database,
            profiled_value::fn_ingredient_(db, db.zalsa()),
        )?;
        let keys = registry.callable_query_keys::<_, Profile>(&target)?;
        observed.token_bytes.set(size_of_val(&keys));
        let caller = registry.reserve_callable(
            db as &dyn Database,
            profiled_caller::fn_ingredient_(db, db.zalsa()),
        )?;
        registry.bind_callable(&target, Leaf(observed))?;
        registry.bind_callable(
            &caller,
            CallableCaller {
                target,
                keys,
                observed,
            },
        )?;
        registry.seal()?.run(move |endpoint| async move {
            Ok::<usize, RunError>(*endpoint.fetch_ref(&caller, request.as_id())?.await?)
        })
    }

    const OLD_TEXT: InlineText = InlineText("old");
    const NEW_TEXT: InlineText = InlineText("incoming-long");

    fn target_ids(db: &dyn Database) -> Vec<Id> {
        profiled_value::intern_ingredient_(db.zalsa())
            .entries(db.zalsa())
            .map(|entry| entry.key().key_index())
            .collect()
    }

    fn stale() -> (TestDb, ProfiledRequest, Id) {
        let mut db = TestDb::new();
        let request = ProfiledRequest::new(&db, OLD_TEXT, 17, 29);
        let labels = [
            OLD_TEXT,
            InlineText("hold-a"),
            InlineText("hold-b"),
            InlineText("hold-c"),
        ];
        let count = revisions(profiled_value::fn_ingredient_(&db, db.zalsa()));
        assert!(count <= labels.len());
        let mut old = None;
        for (index, &text) in labels[..count].iter().enumerate() {
            if index != 0 {
                request.set_text(&mut db).to(text);
            }
            assert_eq!(profiled_caller(&db, request), text.0.len() + 2);
            if index == 0 {
                old = Some(target_ids(&db)[0]);
            }
        }
        request.set_text(&mut db).to(NEW_TEXT);
        let fresh = ProfiledRequest::new(&db, NEW_TEXT, 17, 29);
        (db, fresh, old.unwrap())
    }

    fn complete_profile(db: &TestDb, request: ProfiledRequest, observed: &Observed) -> usize {
        let outcome =
            try_with_attempt(db, 100_000, || run_borrowed(db, request, observed)).unwrap();
        let AttemptOutcome::Complete(Ok(value)) = outcome else {
            panic!("{outcome:?}")
        };
        idle(db);
        value
    }

    fn assert_reused(db: &TestDb, old: Id) -> Id {
        let new = target_ids(db)
            .into_iter()
            .find(|id| id.index() == old.index())
            .unwrap();
        assert_eq!(new.generation(), old.generation() + 1);
        assert_eq!(
            tuple_of(db, profiled_value::fn_ingredient_(db, db.zalsa()), new),
            (NEW_TEXT, 17, 29)
        );
        new
    }

    fn assert_completed(db: &TestDb, request: ProfiledRequest, id: Id) {
        let target = profiled_value::fn_ingredient_(db, db.zalsa());
        let memo = stored(db, target, id).unwrap();
        assert!(!memo.header.may_be_provisional());
        assert!(!memo.header.has_incomplete_attempt());
        assert_eq!(memo.value().unwrap(), &make_value(NEW_TEXT, 17, 29));
        let caller = stored(
            db,
            profiled_caller::fn_ingredient_(db, db.zalsa()),
            request.as_id(),
        )
        .unwrap();
        assert!(!caller.header.may_be_provisional());
        assert!(!caller.header.has_incomplete_attempt());
        assert_eq!(caller.header.revisions.durability, Durability::LOW);
        let argument = profiled_value::intern_ingredient_(db.zalsa()).database_key_index(id);
        assert!(caller.header.origin().inputs().any(|key| key == argument));
        assert_eq!(
            caller
                .header
                .origin()
                .inputs()
                .any(|key| key == target.database_key_index(id)),
            cfg!(feature = "persistence")
        );
    }

    #[test]
    fn reuse_quotes_the_actual_retired_fields_and_non_copy_output() {
        let (state, _reset) = fixture();
        let (db, request, old) = stale();
        let target = profiled_value::fn_ingredient_(&db, db.zalsa());
        assert_eq!(stored(&db, target, old).unwrap().value().unwrap().len(), 5);
        let observed = Observed::default();
        let detached = Rc::new(detached_observation::State::default());
        let _observer = detached_observation::install(detached.clone());
        state.events.borrow_mut().clear();
        let ordinary_before = state.ordinary.get();
        assert_eq!(complete_profile(&db, request, &observed), 15);
        assert_eq!(state.ordinary.get(), ordinary_before);
        assert_eq!(
            &*observed.key_work.borrow(),
            &[
                1, 1, 1,
                1, 1, 13, 1,
                1, 2, 13, 1,
                1, 4, 13, 1,
                1, 8, 13,
                1, 13, 8, 1, 8, 98,
                1, 1, 1,
                1, 1, 13,
                1, 13, 1, 1,
                if cfg!(feature = "persistence") {
                    136
                } else {
                    117
                }
            ]
        );
        assert_eq!(observed.accepted.get(), 2);
        assert_eq!(observed.leaves.get(), 1);
        let new = assert_reused(&db, old);
        assert_eq!(observed.ids.get(), Some([new, new]));
        assert_completed(&db, request, new);
        let events = state.events.borrow();
        let discard = events
            .iter()
            .position(|event| *event == (KeyEvent::Discard, target.database_key_index(old)))
            .unwrap();
        let reuse = events
            .iter()
            .position(|event| {
                *event
                    == (
                        KeyEvent::Reuse,
                        profiled_value::intern_ingredient_(db.zalsa()).database_key_index(new),
                    )
            })
            .unwrap();
        assert!(discard < reuse);
        drop(events);
        assert_eq!(
            (
                detached.live.get(),
                detached.acquired.get(),
                detached.retired.get()
            ),
            (0, 1, 1)
        );
        let ordinary = TestDb::new();
        assert_eq!(
            stored(&db, target, new).unwrap().value(),
            Some(profiled_value(&ordinary, NEW_TEXT, 17, 29))
        );
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ProfileFault {
        RetirementWork,
        Discard,
        Panic,
        Reenter,
        ReenterRefuse,
    }

    struct ProfileHook {
        db: &'static TestDb,
        caller: DatabaseKeyIndex,
        old: DatabaseKeyIndex,
        fault: ProfileFault,
        endpoint: RefCell<Option<TaskEndpoint<'static, 'static>>>,
        fired: Cell<bool>,
        detached: Rc<detached_observation::State>,
        notes: RefCell<Vec<Snapshot>>,
        child_panicking: Cell<bool>,
        new_id: Cell<Option<Id>>,
        new_memo: Cell<usize>,
        payload: Arc<()>,
    }

    thread_local! {
        static HOOK: RefCell<Option<Rc<ProfileHook>>> = const { RefCell::new(None) };
    }

    fn active_hook() -> Option<Rc<ProfileHook>> {
        HOOK.with_borrow(Clone::clone)
    }

    struct ResetHook;
    impl Drop for ResetHook {
        fn drop(&mut self) {
            let hook = HOOK.with_borrow_mut(Option::take);
            if let Some(hook) = hook {
                hook.endpoint.borrow_mut().take();
            }
        }
    }

    struct ProfileGuards<G> {
        _reset: ResetHook,
        _observer: G,
    }
    impl<G> Drop for ProfileGuards<G> {
        fn drop(&mut self) {}
    }

    impl ProfileHook {
        fn note(&self, stage: &'static str) {
            if stage == "child" {
                self.child_panicking.set(std::thread::panicking());
            }
            self.notes.borrow_mut().push(Snapshot {
                stage,
                frame: self.db.zalsa_local().active_query().map(|(key, _)| key),
                claimed: matches!(
                    self.db
                        .zalsa()
                        .lookup_ingredient(self.caller.ingredient_index())
                        .as_function()
                        .unwrap()
                        .sync_table()
                        .peek_claim(self.db.zalsa(), self.caller.key_index(), Reentrancy::Deny),
                    ClaimResult::Cycle { .. }
                ),
                detached: self.detached.live.get(),
                reason: attempt_probe::current().and_then(|current| current.reason()),
                reads: self
                    .db
                    .zalsa_local()
                    .try_with_query_stack(|stack| stack.last().map(|query| query.read_state()))
                    .flatten(),
            });
        }

        fn fail(&self) {
            self.fired.set(true);
            self.note("boundary");
            let endpoint = self.endpoint.borrow_mut().take().unwrap();
            let marker = Marker("child");
            let _demand = endpoint
                .demand::<(), _>(move || async move {
                    let _marker = marker;
                    panic!("failed profiled key request polled its queued child");
                })
                .unwrap();
            if self.fault == ProfileFault::Panic {
                panic_any(Payload(self.payload.clone()));
            }
            attempt_probe::report_incomplete(self.db, Incomplete::Interrupted);
        }

        fn event(&self, event: (KeyEvent, DatabaseKeyIndex)) {
            if event.0 == KeyEvent::Reuse
                && Some(event.1.key_index()) == self.new_id.get()
                && event.1.ingredient_index()
                    == profiled_value::intern_ingredient_(self.db.zalsa()).ingredient_index()
            {
                self.note("reuse");
            }
            if self.fired.get() || event != (KeyEvent::Discard, self.old) {
                return;
            }
            self.fired.set(true);
            self.note("discard");
            if matches!(
                self.fault,
                ProfileFault::Reenter | ProfileFault::ReenterRefuse
            ) {
                let value = profiled_value(self.db, NEW_TEXT, 17, 29);
                assert_eq!(value, &make_value(NEW_TEXT, 17, 29));
                let new = assert_reused(self.db, self.old.key_index());
                self.new_id.set(Some(new));
                self.new_memo.set(
                    std::ptr::from_ref(
                        stored(
                            self.db,
                            profiled_value::fn_ingredient_(self.db, self.db.zalsa()),
                            new,
                        )
                        .unwrap(),
                    )
                    .addr(),
                );
                if self.fault == ProfileFault::Reenter {
                    return;
                }
            }
            self.fail();
        }
    }

    pub(super) fn note(stage: &'static str) {
        if let Some(hook) = active_hook() {
            hook.note(stage);
        }
    }

    pub(super) fn event(event: (KeyEvent, DatabaseKeyIndex)) {
        if let Some(hook) = active_hook() {
            hook.event(event);
        }
    }

    fn arm_profile(endpoint: TaskEndpoint<'static, 'static>) {
        if let Some(hook) = active_hook() {
            *hook.endpoint.borrow_mut() = Some(endpoint);
        }
    }

    fn fault_fixture(
        fault: ProfileFault,
    ) -> (
        &'static TestDb,
        ProfiledRequest,
        Rc<ProfileHook>,
        &'static Observed,
        impl Drop,
    ) {
        let (db, request, old) = stale();
        let db: &'static TestDb = Box::leak(Box::new(db));
        let observed: &'static Observed = Box::leak(Box::new(Observed::default()));
        let detached = Rc::new(detached_observation::State::default());
        let observer = detached_observation::install(detached.clone());
        let hook = Rc::new(ProfileHook {
            db,
            caller: profiled_caller::fn_ingredient_(db, db.zalsa())
                .database_key_index(request.as_id()),
            old: profiled_value::fn_ingredient_(db, db.zalsa()).database_key_index(old),
            fault,
            endpoint: RefCell::new(None),
            fired: Cell::new(false),
            detached,
            notes: RefCell::new(Vec::new()),
            child_panicking: Cell::new(false),
            new_id: Cell::new(None),
            new_memo: Cell::new(0),
            payload: Arc::new(()),
        });
        let reset = ResetHook;
        HOOK.with_borrow_mut(|slot| assert!(slot.replace(hook.clone()).is_none()));
        state().events.borrow_mut().clear();
        state().journal.borrow_mut().clear();
        (
            db,
            request,
            hook,
            observed,
            ProfileGuards {
                _reset: reset,
                _observer: observer,
            },
        )
    }

    fn run_fault(
        db: &'static TestDb,
        request: ProfiledRequest,
        observed: &'static Observed,
    ) -> RunResult<usize> {
        let leaf: &'static Leaf<'static> = Box::leak(Box::new(Leaf(observed)));
        let mut registry = RegistryBuilder::new(db, observed)?;
        let target = registry.reserve(
            db as &dyn Database,
            profiled_value::fn_ingredient_(db, db.zalsa()),
        )?;
        let keys = registry.query_keys::<_, Profile>(&target)?;
        let route = registry.reserve(
            db as &dyn Database,
            profiled_caller::fn_ingredient_(db, db.zalsa()),
        )?;
        let leaf = registry.provider(leaf)?;
        registry.bind_executable(&target, &leaf)?;
        let caller: &'static _ = Box::leak(Box::new(BorrowedCaller {
            target,
            keys,
            leaf,
            observed,
            arm: arm_profile,
        }));
        let binding = registry.provider(caller)?;
        registry.bind_executable(&route, &binding)?;
        registry.seal()?.run(move |endpoint| async move {
            let _root = Marker("root");
            Ok::<usize, RunError>(
                *endpoint
                    .provider(binding)?
                    .fetch_ref(&route, request.as_id())?
                    .await?,
            )
        })
    }

    fn assert_failed(hook: &ProfileHook, request: ProfiledRequest, observed: &Observed) {
        assert!(hook.fired.get());
        assert_eq!(observed.accepted.get(), 0);
        assert!(observed.last_id.get().is_none());
        assert!(
            stored(
                hook.db,
                profiled_caller::fn_ingredient_(hook.db, hook.db.zalsa()),
                request.as_id()
            )
            .is_none()
        );
        assert_eq!(&*state().journal.borrow(), &["child", "caller", "root"]);
        let notes = hook.notes.borrow();
        let expected_detached = usize::from(hook.fault != ProfileFault::RetirementWork);
        for stage in ["boundary", "child"] {
            let note = notes.iter().find(|note| note.stage == stage).unwrap();
            assert_eq!(note.frame, Some(hook.caller));
            assert!(note.claimed);
            assert_eq!(note.detached, expected_detached);
            if stage == "child" && hook.fault != ProfileFault::Panic {
                assert_eq!(note.reason, Some(Incomplete::Interrupted));
            }
        }
        assert_eq!(
            notes
                .iter()
                .find(|note| note.stage == "caller")
                .unwrap()
                .detached,
            0
        );
        if hook.fault == ProfileFault::Panic {
            assert!(hook.child_panicking.get());
        }
        assert_eq!(
            (
                hook.detached.live.get(),
                hook.detached.acquired.get(),
                hook.detached.retired.get()
            ),
            (0, expected_detached, expected_detached)
        );
        idle(hook.db);
    }

    fn disable_fault() {
        if let Some(hook) = HOOK.with_borrow_mut(Option::take) {
            hook.endpoint.borrow_mut().take();
        }
    }

    fn retries(db: &TestDb, request: ProfiledRequest, new: Id, stamp: Stamp) {
        disable_fault();
        for _ in 0..2 {
            let observed = Observed::default();
            assert_eq!(complete_profile(db, request, &observed), 15);
            assert_completed(db, request, new);
            assert_eq!(Stamp::current(db), stamp);
        }
    }

    #[test]
    fn non_copy_retirement_survives_admission_and_event_failures() {
        for fault in [
            ProfileFault::RetirementWork,
            ProfileFault::Discard,
            ProfileFault::Panic,
        ] {
            let (state, _reset) = fixture();
            let (db, request, hook, observed, _guards) = fault_fixture(fault);
            let stamp = Stamp::current(db);
            let target = profiled_value::fn_ingredient_(db, db.zalsa());
            let before = target_ids(db);
            let old_pointer = std::ptr::from_ref(stored(db, target, hook.old.key_index()).unwrap()).addr();
            let old_value = stored(db, target, hook.old.key_index())
                .unwrap()
                .value()
                .unwrap()
                .clone();
            let result = catch_unwind(AssertUnwindSafe(|| {
                try_with_attempt(db, 100_000, || {
                    let result = run_fault(db, request, observed);
                    state.direct_error.set(result.as_ref().err().copied());
                    result
                })
            }));
            if fault == ProfileFault::Panic {
                let payload = result.unwrap_err().downcast::<Payload>().unwrap();
                assert!(Arc::ptr_eq(&payload.0, &hook.payload));
            } else {
                assert_eq!(
                    result.unwrap(),
                    Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
                );
                assert_eq!(
                    state.direct_error.get(),
                    Some(RunError::Refused(Incomplete::Interrupted))
                );
            }
            assert_failed(&hook, request, observed);
            let mut expected = vec![
                1, 1, 1,
                1, 1, 13, 1,
                1, 2, 13, 1,
                1, 4, 13, 1,
                1, 8, 13,
                1, 13, 8,
            ];
            if fault != ProfileFault::RetirementWork {
                expected.extend([1, 8, 98]);
            }
            assert_eq!(&*observed.key_work.borrow(), &expected);
            if fault == ProfileFault::RetirementWork {
                assert_eq!(target_ids(db), before);
                #[cfg(feature = "salsa_unstable")]
                {
                    let entry = profiled_value::intern_ingredient_(db.zalsa())
                        .entries(db.zalsa())
                        .find(|entry| entry.key().key_index() == hook.old.key_index())
                        .unwrap();
                    assert_eq!(entry.value().fields(), &(OLD_TEXT, 17, 29));
                }
                let current = stored(db, target, hook.old.key_index()).unwrap();
                assert_eq!(std::ptr::from_ref(current).addr(), old_pointer);
                assert_eq!(current.value(), Some(&old_value));
            } else {
                let new = assert_reused(db, hook.old.key_index());
                assert!(stored(db, target, new).is_none());
            }
            let events = state.events.borrow();
            assert_eq!(
                events.contains(&(KeyEvent::Discard, hook.old)),
                fault != ProfileFault::RetirementWork
            );
            assert!(!events.iter().any(|(event, _)| *event == KeyEvent::Reuse));
            drop(events);
            if fault == ProfileFault::RetirementWork {
                disable_fault();
                assert_eq!(complete_profile(db, request, &Observed::default()), 15);
            }
            let new = assert_reused(db, hook.old.key_index());
            retries(db, request, new, stamp);
        }
    }

    #[test]
    fn non_copy_callback_replacement_survives_old_generation_retirement() {
        for fault in [ProfileFault::Reenter, ProfileFault::ReenterRefuse] {
            let (state, _reset) = fixture();
            let (db, request, hook, observed, _guards) = fault_fixture(fault);
            let stamp = Stamp::current(db);
            let ordinary_before = state.ordinary.get();
            let result = try_with_attempt(db, 100_000, || {
                let result = run_fault(db, request, observed);
                state.direct_error.set(result.as_ref().err().copied());
                result
            });
            if fault == ProfileFault::ReenterRefuse {
                assert_eq!(
                    result,
                    Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
                );
                assert_eq!(
                    state.direct_error.get(),
                    Some(RunError::Refused(Incomplete::Interrupted))
                );
                assert_failed(&hook, request, observed);
            } else {
                assert_eq!(result, Ok(AttemptOutcome::Complete(Ok(15))));
                assert_eq!(observed.accepted.get(), 2);
                let notes = hook.notes.borrow();
                for stage in ["discard", "reuse"] {
                    assert_eq!(
                        notes
                            .iter()
                            .find(|note| note.stage == stage)
                            .unwrap()
                            .detached,
                        1
                    );
                }
            }
            assert_eq!(state.ordinary.get(), ordinary_before + 1);
            assert_eq!(observed.leaves.get(), 0);
            let new = hook.new_id.get().unwrap();
            let memo = stored(db, profiled_value::fn_ingredient_(db, db.zalsa()), new).unwrap();
            assert_eq!(std::ptr::from_ref(memo).addr(), hook.new_memo.get());
            assert_eq!(memo.value(), Some(&make_value(NEW_TEXT, 17, 29)));
            assert_eq!(
                (
                    hook.detached.live.get(),
                    hook.detached.acquired.get(),
                    hook.detached.retired.get()
                ),
                (0, 1, 1)
            );
            retries(db, request, new, stamp);
            assert_eq!(
                std::ptr::from_ref(
                    stored(db, profiled_value::fn_ingredient_(db, db.zalsa()), new).unwrap()
                )
                .addr(),
                hook.new_memo.get()
            );
        }
    }

    struct RefusingProfile<const MODE: u8>;
    type InputRefusal = RefusingProfile<0>;
    type OutputRefusal = RefusingProfile<1>;
    type CombinedOverflow = RefusingProfile<2>;

    impl<C, const MODE: u8> PassiveMemoProfile<C> for RefusingProfile<MODE>
    where
        C: for<'a> Configuration<Output<'a> = Vec<u32>>,
    {
        fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
            if MODE == 1 { None } else { Some(output.len()) }
        }

        fn retired_output_work_bounded<'db>(
            output: &C::Output<'db>,
            fuel: &mut QuoteFuel,
        ) -> Result<usize, QuoteError> {
            fuel.consume(1)?;
            <Self as PassiveMemoProfile<C>>::retired_output_work(output)
                .ok_or(QuoteError::Unsupported)
        }
    }

    impl<C, const MODE: u8> QueryKeyProfile<C> for RefusingProfile<MODE>
    where
        C: InternedQueryConfiguration
            + for<'a> crate::interned::Configuration<Fields<'a> = (InlineText, u32, u32)>
            + for<'a> Configuration<Output<'a> = Vec<u32>>,
    {
        fn input_work<'db>(
            input: &<C as crate::interned::Configuration>::Fields<'db>,
        ) -> Option<usize> {
            if MODE == 0 {
                None
            } else if MODE == 2 && input.0.0.len() == OLD_TEXT.0.len() {
                Some(usize::MAX)
            } else {
                Some(input.0.0.len())
            }
        }

        fn input_work_bounded<'db>(
            input: &<C as crate::interned::Configuration>::Fields<'db>,
            fuel: &mut QuoteFuel,
        ) -> Result<usize, QuoteError> {
            fuel.consume(1)?;
            <Self as QueryKeyProfile<C>>::input_work(input).ok_or(QuoteError::Unsupported)
        }
    }

    fn reject_quote<C, P>(
        db: &TestDb,
        request: ProfiledRequest,
        old: Id,
        ingredient: &IngredientImpl<C>,
        message: &'static str,
    ) where
        C: InternedQueryConfiguration
            + for<'a> crate::interned::Configuration<Fields<'a> = (InlineText, u32, u32)>
            + for<'a> Configuration<DbView = dyn Database, Output<'a> = Vec<u32>>,
        P: QueryKeyProfile<C> + 'static,
    {
        let state = state();
        let old_pointer = std::ptr::from_ref(stored(db, ingredient, old).unwrap()).addr();
        let old_value = stored(db, ingredient, old)
            .unwrap()
            .value()
            .unwrap()
            .clone();
        let before = target_ids(db);
        let stamp = Stamp::current(db);
        let detached = Rc::new(detached_observation::State::default());
        let _observer = detached_observation::install(detached.clone());
        state.events.borrow_mut().clear();
        for _ in 0..2 {
            let observed = Observed::default();
            let outcome = try_with_attempt(db, 100_000, || {
                let result = run_for::<_, _, P>(
                    db,
                    request,
                    &observed,
                    ingredient,
                    profiled_caller::fn_ingredient_(db, db.zalsa()),
                );
                state.direct_error.set(result.as_ref().err().copied());
                result
            });
            assert_eq!(
                outcome,
                Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
            );
            assert_eq!(state.direct_error.get(), Some(RunError::Contract(message)));
            assert_eq!(observed.accepted.get(), 0);
            assert!(observed.last_id.get().is_none());
            assert!(
                stored(
                    db,
                    profiled_caller::fn_ingredient_(db, db.zalsa()),
                    request.as_id()
                )
                .is_none()
            );
            assert_eq!(target_ids(db), before);
            let current = stored(db, ingredient, old).unwrap();
            assert_eq!(std::ptr::from_ref(current).addr(), old_pointer);
            assert_eq!(current.value(), Some(&old_value));
            assert_eq!((detached.acquired.get(), detached.retired.get()), (0, 0));
            assert_eq!(detached.live.get(), 0);
            assert!(
                !state
                    .events
                    .borrow()
                    .iter()
                    .any(|(event, _)| matches!(event, KeyEvent::Discard | KeyEvent::Reuse))
            );
            assert_eq!(Stamp::current(db), stamp);
            idle(db);
        }
        assert_eq!(complete_profile(db, request, &Observed::default()), 15);
        let new = assert_reused(db, old);
        retries(db, request, new, stamp);
    }

    #[test]
    fn profile_quote_refusals_preserve_the_selected_generation_contract() {
        macro_rules! row {
            ($profile:ty, $message:literal) => {{
                let (_state, _reset) = fixture();
                let (db, request, old) = stale();
                reject_quote::<_, $profile>(
                    &db,
                    request,
                    old,
                    profiled_value::fn_ingredient_(&db, db.zalsa()),
                    $message,
                );
            }};
        }
        row!(InputRefusal, "retirement quotation is unsupported");
        row!(OutputRefusal, "retirement quotation is unsupported");
        for _ in 0..2 {
            row!(
                CombinedOverflow,
                "retirement quotation work overflow"
            );
        }
    }

    #[crate::tracked(returns(ref), attempt = CompleteOnly)]
    fn historical_value(db: &dyn Database, text: InlineText, left: u32, kind: u32) -> Vec<u32> {
        if kind == 0 {
            let _value = Produced::new(db, left);
        }
        #[cfg(feature = "accumulator")]
        if kind == 1 {
            Warning { _value: left }.accumulate(db);
        }
        make_value(text, left, kind)
    }

    #[crate::tracked(returns(copy), attempt = CompleteOnly)]
    fn historical_caller(db: &dyn Database, request: ProfiledRequest) -> usize {
        historical_value(db, request.text(db), request.left(db), request.right(db)).len()
    }

    fn reject_historical<C>(db: &TestDb, ingredient: &IngredientImpl<C>, old: Id, kind: u32)
    where
        C: InternedQueryConfiguration
            + for<'a> crate::interned::Configuration<Fields<'a> = (InlineText, u32, u32)>
            + for<'a> Configuration<DbView = dyn Database, Output<'a> = Vec<u32>>,
    {
        let argument = C::argument_ingredient(db.zalsa());
        let index = ingredient
            .query_key_memo_index(db.zalsa(), argument)
            .unwrap_or_else(|error| panic!("{}", error.message()));
        let memo = stored(db, ingredient, old).unwrap();
        let pointer = std::ptr::from_ref(memo).addr();
        let value = memo.value().unwrap().clone();
        let entries = argument
            .entries(db.zalsa())
            .map(|entry| entry.key())
            .collect::<Vec<_>>();
        let identities = memo.header.revisions.tracked_struct_ids().to_vec();
        if kind == 0 {
            assert_eq!(identities.len(), 1);
            assert!(!memo.header.outputs_are_empty());
            assert!(memo.header.origin().outputs().next().is_none());
        }
        #[cfg(feature = "accumulator")]
        if kind == 1 {
            assert!(memo.header.outputs_are_empty());
            assert!(memo.header.revisions.accumulated().is_some());
        }
        let detached = Rc::new(detached_observation::State::default());
        let _observer = detached_observation::install(detached.clone());
        for _ in 0..2 {
            let result = argument.prepare_intern(
                db.zalsa(),
                db.zalsa_local(),
                (NEW_TEXT, 17, kind),
                |_, fields| fields,
                |table| inspect_passive_singleton::<C, OutputRefusal>(table, index).map(|_| ()),
            );
            let error = match result {
                Err(error) => error,
                Ok(_) => panic!("historical metadata was accepted"),
            };
            assert_eq!(
                error.message(),
                if kind == 0 {
                    "fixed query key memo has tracked outputs"
                } else {
                    "fixed query key memo has direct accumulators"
                }
            );
            assert_eq!(
                argument
                    .entries(db.zalsa())
                    .map(|entry| entry.key())
                    .collect::<Vec<_>>(),
                entries
            );
            let current = stored(db, ingredient, old).unwrap();
            assert_eq!(std::ptr::from_ref(current).addr(), pointer);
            assert_eq!(current.value(), Some(&value));
            assert_eq!(current.header.revisions.tracked_struct_ids(), identities);
            #[cfg(feature = "accumulator")]
            if kind == 1 {
                assert!(current.header.revisions.accumulated().is_some());
            }
            assert_eq!(detached.acquired.get(), 0);
        }
    }

    #[test]
    fn historical_non_copy_metadata_precedes_the_output_quote() {
        let kinds = [
            0,
            #[cfg(feature = "accumulator")]
            1,
        ];
        for kind in kinds {
            let (_state, _reset) = fixture();
            let mut db = TestDb::new();
            let request = ProfiledRequest::new(&db, OLD_TEXT, 17, kind);
            let labels = [
                OLD_TEXT,
                InlineText("hold-a"),
                InlineText("hold-b"),
                InlineText("hold-c"),
            ];
            let count = revisions(historical_value::fn_ingredient_(&db, db.zalsa()));
            assert!(count <= labels.len());
            let mut old = None;
            for (index, &text) in labels[..count].iter().enumerate() {
                if index != 0 {
                    request.set_text(&mut db).to(text);
                }
                assert_eq!(historical_caller(&db, request), text.0.len() + 2);
                if index == 0 {
                    old = Some(
                        historical_value::intern_ingredient_(db.zalsa())
                            .entries(db.zalsa())
                            .next()
                            .unwrap()
                            .key()
                            .key_index(),
                    );
                }
            }
            request.set_text(&mut db).to(NEW_TEXT);
            reject_historical(
                &db,
                historical_value::fn_ingredient_(&db, db.zalsa()),
                old.unwrap(),
                kind,
            );
            idle(&db);
        }
    }

    #[crate::tracked(returns(ref), attempt = ReturnOnly, lru = 1)]
    fn evictable_value(_db: &dyn Database, text: InlineText, left: u32, right: u32) -> Vec<u32> {
        state().ordinary.set(state().ordinary.get() + 1);
        make_value(text, left, right)
    }

    #[crate::tracked(returns(copy), attempt = ReturnOnly)]
    fn evictable_caller(db: &dyn Database, request: ProfiledRequest) -> usize {
        let tuple = (request.text(db), request.left(db), request.right(db));
        let first = evictable_value(db, tuple.0, tuple.1, tuple.2);
        let second = evictable_value(db, tuple.0, tuple.1, tuple.2);
        assert!(std::ptr::eq(first, second));
        second.len()
    }

    #[crate::tracked(returns(copy), attempt = ReturnOnly)]
    fn evictable_key_only(db: &dyn Database, request: ProfiledRequest) -> Id {
        let tuple = (request.text(db), request.left(db), request.right(db));
        evictable_value::intern_ingredient_(db.zalsa()).intern_id(
            db.zalsa(),
            db.zalsa_local(),
            tuple,
            |_, fields| fields,
        )
    }

    fn stale_evictable(empty: bool) -> (TestDb, ProfiledRequest, Id) {
        let mut db = TestDb::new();
        let request = ProfiledRequest::new(&db, OLD_TEXT, 17, 29);
        let labels = [
            OLD_TEXT,
            InlineText("hold-a"),
            InlineText("hold-b"),
            InlineText("hold-c"),
        ];
        let count = revisions(evictable_value::fn_ingredient_(&db, db.zalsa()));
        assert!(count <= labels.len());
        let mut old = None;
        for (index, &text) in labels[..count].iter().enumerate() {
            if index != 0 {
                request.set_text(&mut db).to(text);
            }
            let id = if empty {
                evictable_key_only(&db, request)
            } else {
                assert_eq!(evictable_caller(&db, request), text.0.len() + 2);
                evictable_value::intern_ingredient_(db.zalsa())
                    .entries(db.zalsa())
                    .map(|entry| entry.key().key_index())
                    .find(|&id| {
                        tuple_of(&db, evictable_value::fn_ingredient_(&db, db.zalsa()), id).0
                            == text
                    })
                    .unwrap()
            };
            if index == 0 {
                old = Some(id);
                if !empty {
                    // A second real fetch selects the older value for ordinary LRU eviction.
                    let recent = ProfiledRequest::new(&db, InlineText("recent"), 17, 29);
                    assert_eq!(evictable_caller(&db, recent), 8);
                }
            }
        }
        request.set_text(&mut db).to(NEW_TEXT);
        let old = old.unwrap();
        assert!(
            evictable_value::intern_ingredient_(db.zalsa())
                .entries(db.zalsa())
                .any(|entry| entry.key().key_index() == old)
        );
        let memo = stored(&db, evictable_value::fn_ingredient_(&db, db.zalsa()), old);
        if empty {
            assert!(memo.is_none());
        } else {
            assert!(
                memo.unwrap().value().is_none(),
                "ordinary LRU did not evict the old value"
            );
        }
        let fresh = ProfiledRequest::new(&db, NEW_TEXT, 17, 29);
        (db, fresh, old)
    }

    #[test]
    fn selected_receipt_distinguishes_empty_and_value_less_memos() {
        for empty in [true, false] {
            let (state, _reset) = fixture();
            let (db, request, old) = stale_evictable(empty);
            let ingredient = evictable_value::fn_ingredient_(&db, db.zalsa());
            let argument = evictable_value::intern_ingredient_(db.zalsa());
            let caller = evictable_caller::fn_ingredient_(&db, db.zalsa());
            let observed = Observed::default();
            let detached = Rc::new(detached_observation::State::default());
            let _observer = detached_observation::install(detached.clone());
            state.events.borrow_mut().clear();
            let ordinary_before = state.ordinary.get();
            let outcome = try_with_attempt(&db, 100_000, || {
                run_for::<_, _, Profile>(&db, request, &observed, ingredient, caller)
            });
            assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(15))));
            idle(&db);
            assert_eq!(state.ordinary.get(), ordinary_before);
            assert_eq!(
                &*observed.key_work.borrow(),
                &[
                    1, 1, 1,
                    1, 1, 13, 1,
                    1, 2, 13, 1,
                    1, 4, 13, 1,
                    1, 8, 13,
                    1, 13, 3, 1, 8, 98,
                    1, 1, 1,
                    1, 1, 13,
                    1, 13, 1, 1,
                    if cfg!(feature = "persistence") {
                        136
                    } else {
                        117
                    }
                ]
            );
            assert_eq!(observed.accepted.get(), 2);
            assert_eq!(observed.leaves.get(), 1);
            assert!(observed.same_value.get());
            let [first, second] = observed.ids.get().unwrap();
            assert_eq!(first, second);
            assert_eq!(first.index(), old.index());
            assert_eq!(first.generation(), old.generation() + 1);
            assert_eq!(tuple_of(&db, ingredient, first), (NEW_TEXT, 17, 29));
            let memo = stored(&db, ingredient, first).unwrap();
            assert!(!memo.header.may_be_provisional());
            assert!(!memo.header.has_incomplete_attempt());
            assert_eq!(memo.value(), Some(&make_value(NEW_TEXT, 17, 29)));
            let caller_memo = stored(&db, caller, request.as_id()).unwrap();
            assert!(!caller_memo.header.may_be_provisional());
            assert!(!caller_memo.header.has_incomplete_attempt());
            assert_eq!(caller_memo.header.revisions.durability, Durability::LOW);
            assert!(
                caller_memo
                    .header
                    .origin()
                    .inputs()
                    .any(|key| key == argument.database_key_index(first))
            );
            let events = state.events.borrow();
            let discarded = events
                .iter()
                .filter(|event| **event == (KeyEvent::Discard, ingredient.database_key_index(old)))
                .count();
            assert_eq!(discarded, usize::from(!empty));
            assert!(events.contains(&(KeyEvent::Reuse, argument.database_key_index(first))));
            assert_eq!(
                (
                    detached.live.get(),
                    detached.acquired.get(),
                    detached.retired.get()
                ),
                (0, 1, 1)
            );
        }
    }

    fn report_layout<C, P>(_ingredient: &IngredientImpl<C>, name: &str)
    where
        C: InternedQueryConfiguration,
    {
        for (carrier, size, alignment) in key_layout_for_tests::<C, P>() {
            eprintln!("PROFILED_KEY_LAYOUT {name} {carrier} size={size} align={alignment}");
        }
    }

    fn tuple_of<C>(
        db: &dyn Database,
        _ingredient: &IngredientImpl<C>,
        id: Id,
    ) -> (InlineText, u32, u32)
    where
        C: InternedQueryConfiguration
            + for<'a> crate::interned::Configuration<Fields<'a> = (InlineText, u32, u32)>,
    {
        C::id_to_input(db.zalsa(), id)
    }

    #[test]
    fn non_copy_three_input_keys_run_through_borrowed_and_callable_routes() {
        for callable in [false, true] {
            let (state, _reset) = fixture();
            let db = TestDb::new();
            let observed = Observed::default();
            let tuple = (InlineText("variable input"), 17, 29);
            let request = ProfiledRequest::new(&db, tuple.0, tuple.1, tuple.2);
            let argument = profiled_value::intern_ingredient_(db.zalsa());
            let ingredient = profiled_value::fn_ingredient_(&db, db.zalsa());
            let caller = profiled_caller::fn_ingredient_(&db, db.zalsa());
            assert_eq!(argument.entries(db.zalsa()).count(), 0);
            assert!(stored(&db, caller, request.as_id()).is_none());
            let result = try_with_attempt(&db, 100_000, || {
                if callable {
                    run_callable(&db, request, &observed)
                } else {
                    run_borrowed(&db, request, &observed)
                }
            })
            .unwrap();
            assert!(
                matches!(result, AttemptOutcome::Complete(Ok(length)) if length == tuple.0.0.len() + 2)
            );
            idle(&db);
            assert_eq!(state.ordinary.get(), 0);
            assert_eq!(observed.callers.get(), 1);
            assert_eq!(observed.leaves.get(), 1);
            let [first, second] = observed.ids.get().unwrap();
            assert_eq!(first, second);
            assert!(observed.same_value.get());
            assert_eq!(tuple_of(&db, ingredient, first), tuple);
            assert_eq!(argument.entries(db.zalsa()).count(), 1);
            let argument_key = argument.database_key_index(first);
            let function_key = ingredient.database_key_index(first);
            assert_eq!(argument_key.key_index(), function_key.key_index());
            assert_ne!(
                argument_key.ingredient_index(),
                function_key.ingredient_index()
            );
            let memo = stored(&db, ingredient, first).unwrap();
            assert!(!memo.header.may_be_provisional());
            assert!(!memo.header.has_incomplete_attempt());
            assert_eq!(memo.header.revisions.durability, Durability::NEVER_CHANGE);
            let caller_memo = stored(&db, caller, request.as_id()).unwrap();
            assert!(!caller_memo.header.may_be_provisional());
            assert!(!caller_memo.header.has_incomplete_attempt());
            assert_eq!(caller_memo.header.revisions.durability, Durability::LOW);
            let edges = caller_memo.header.origin().inputs().collect::<Vec<_>>();
            assert!(edges.contains(&argument_key));
            assert_eq!(edges.contains(&function_key), cfg!(feature = "persistence"));
            assert_eq!(
                state
                    .events
                    .borrow()
                    .iter()
                    .filter(|(event, key)| *event == KeyEvent::Intern && *key == argument_key)
                    .count(),
                1
            );

            let ordinary = TestDb::new();
            let ordinary_request = ProfiledRequest::new(&ordinary, tuple.0, tuple.1, tuple.2);
            assert_eq!(
                profiled_caller(&ordinary, ordinary_request),
                tuple.0.0.len() + 2
            );
            assert_eq!(
                memo.value(),
                Some(profiled_value(&ordinary, tuple.0, tuple.1, tuple.2))
            );
            assert_eq!(state.ordinary.get(), 1);
            eprintln!(
                "PROFILED_KEYS callable={callable} token_bytes={} key_future={:?} work={:?}",
                observed.token_bytes.get(),
                observed.future_layout.get().unwrap(),
                observed.work.borrow()
            );
            report_layout::<_, Profile>(ingredient, "profiled");
            report_layout::<_, CopyMemoProfile>(compare::fn_ingredient_(&db, db.zalsa()), "fixed");
        }
    }
}

mod finite_values {
    use std::collections::BTreeMap;

    use super::super::super::key_run::value_layout_for_tests;
    use super::super::super::registration::{
        FiniteInternedValues, InternedValues, PassiveMemoGroup, PassiveMemoProfile,
        PassiveMemoSchema,
    };
    use super::*;
    use crate::id::FromId;
    use crate::interned::FiniteInternedConfiguration;
    use crate::zalsa::IngredientIndex;

    #[derive(Default)]
    pub(super) struct ValueState {
        active: Cell<bool>,
        tag: Cell<usize>,
        attempted: Cell<usize>,
        work: RefCell<Vec<usize>>,
        clones: Cell<usize>,
        live: [Cell<usize>; 3],
        drops: [Cell<usize>; 3],
        drop_reason: [Cell<Option<Incomplete>>; 3],
        drop_order: [Cell<usize>; 3],
        detached_at_drop: [Cell<usize>; 3],
        sequence: Cell<usize>,
        child_order: Cell<usize>,
        caller_order: Cell<usize>,
        hook: RefCell<Option<Rc<ValueHook>>>,
        multiple_hook: RefCell<Option<Rc<MultipleHook>>>,
        composed: ComposedState,
    }

    impl ValueState {
        fn tick(&self) -> usize {
            let next = self.sequence.get() + 1;
            self.sequence.set(next);
            next
        }
    }

    #[derive(Debug, crate::SalsaValue)]
    struct OwnedMap {
        entries: BTreeMap<String, u32>,
        tag: usize,
    }

    impl OwnedMap {
        fn new(name_bytes: u32, tag: usize) -> Self {
            let entries = if name_bytes == 0 {
                BTreeMap::new()
            } else {
                BTreeMap::from([("x".repeat(name_bytes as usize), name_bytes)])
            };
            if tag != 0 {
                let state = state();
                state.values.live[tag].set(state.values.live[tag].get() + 1);
            }
            Self { entries, tag }
        }

        fn work(&self) -> Option<usize> {
            self.work_with(&mut || Ok(())).ok()
        }

        fn work_bounded(&self, fuel: &mut QuoteFuel) -> Result<usize, QuoteError> {
            let work = self.work_with(&mut || fuel.consume(1))?;
            if self.tag == 2
                && let Some(hook) = state().values.hook.borrow().as_ref()
            {
                hook.input_quoted.set(true);
            }
            Ok(work)
        }

        fn work_with(
            &self,
            admit: &mut impl FnMut() -> Result<(), QuoteError>,
        ) -> Result<usize, QuoteError> {
            admit()?;
            self.entries
                .keys()
                .try_fold(self.entries.len(), |total, name| {
                    admit()?;
                    total.checked_add(name.len()).ok_or(QuoteError::Overflow)
                })
        }
    }

    impl Clone for OwnedMap {
        fn clone(&self) -> Self {
            state().values.clones.set(state().values.clones.get() + 1);
            Self {
                entries: self.entries.clone(),
                tag: 0,
            }
        }
    }

    impl PartialEq for OwnedMap {
        fn eq(&self, other: &Self) -> bool {
            self.entries == other.entries
        }
    }

    impl Eq for OwnedMap {}

    impl Hash for OwnedMap {
        fn hash<H: Hasher>(&self, state: &mut H) {
            // Deliberate finite collisions exercise canonical equality and one reuse queue.
            state.write_u32(0);
        }
    }

    impl Drop for OwnedMap {
        fn drop(&mut self) {
            if self.tag == 0 {
                return;
            }
            // No allocation or database call is allowed in this observed field destructor.
            let _ = STATE.try_with(|slot| {
                let slot = slot.borrow();
                if let Some(state) = slot.as_ref() {
                    let values = &state.values;
                    values.live[self.tag].set(values.live[self.tag].get() - 1);
                    values.drops[self.tag].set(values.drops[self.tag].get() + 1);
                    values.drop_order[self.tag].set(values.tick());
                    values.drop_reason[self.tag]
                        .set(attempt_probe::current().and_then(|current| current.reason()));
                    if let Some(hook) = values.hook.borrow().as_ref() {
                        values.detached_at_drop[self.tag].set(hook.detached.live.get());
                    }
                }
            });
        }
    }

    #[crate::interned]
    struct Interface<'db> {
        #[returns(copy)]
        salt: Collision,
        #[returns(ref)]
        members: OwnedMap,
    }

    // Hash, equality, quote and destruction inspect finite scalar/string/map data only.
    impl FiniteInternedConfiguration for Interface<'static> {
        fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
            fields.1.work()
        }

        fn field_work_bounded(
            fields: &Self::Fields<'_>,
            fuel: &mut QuoteFuel,
        ) -> Result<usize, QuoteError> {
            fields.1.work_bounded(fuel)
        }
    }

    #[crate::tracked(returns(copy), attempt = CompleteOnly)]
    fn member_count(db: &dyn Database, value: Interface<'_>) -> usize {
        if value.salt(db).0 == u32::MAX {
            let _output = Produced::new(db, 0);
        }
        value.members(db).entries.len()
    }

    #[crate::tracked(returns(copy), attempt = ReturnOnly)]
    fn map_caller(db: &dyn Database, request: Request) -> Id {
        Interface::new(
            db,
            Collision(request.left(db)),
            OwnedMap::new(request.right(db), state().values.tag.get()),
        )
        .as_id()
    }

    struct ValueScope(bool);

    impl ValueScope {
        fn new() -> Self {
            state()
                .values
                .attempted
                .set(state().values.attempted.get() + 1);
            Self(state().values.active.replace(true))
        }
    }

    impl Drop for ValueScope {
        fn drop(&mut self) {
            state().values.active.set(self.0);
        }
    }

    struct Provider<'db, M: Configuration, D: Configuration> {
        values: FiniteInternedValues<'db, Interface<'static>, M>,
        caller: Route<'db, D>,
    }

    impl<'run, 'db: 'run, M, D> ExecutableRouteProvider<'run, 'db, D> for Provider<'db, M, D>
    where
        M: for<'a> Configuration<
                DbView = dyn Database,
                SalsaStruct<'a> = Interface<'a>,
                Output<'a> = usize,
            >,
        D: for<'a> Configuration<DbView = dyn Database, Input<'a> = Request, Output<'a> = Id>,
    {
        // Request conversion constructs a handle; Id equality compares its scalar identity.
        fixture_native_value!(executable, 'run, 'db, D, 1);

        async fn body(
            &'run self,
            context: ProviderContext<'run, 'db, Self>,
            db: &'db dyn Database,
            input: Request,
        ) -> RunResult<Id> {
            let _caller = Marker("caller");
            let endpoint = context.endpoint();
            if state()
                .values
                .hook
                .borrow()
                .as_ref()
                .is_some_and(|hook| hook.fault == ValueFault::ReadBytes)
            {
                let _ = input.next(db);
            }
            let fields = (
                Collision(input.left(db)),
                OwnedMap::new(input.right(db), state().values.tag.get()),
            );
            let value = {
                let _scope = ValueScope::new();
                endpoint.intern_value(&self.values, fields).await
            };
            state().delivered.borrow_mut().push(value.as_id());
            Ok(value.as_id())
        }

        async fn initial(
            &'run self,
            _context: ProviderContext<'run, 'db, Self>,
            _db: &'db dyn Database,
            _id: Id,
            _input: Request,
        ) -> RunResult<Id> {
            Err(RunError::RequiresFetch)
        }

        async fn recover<'call>(
            &'run self,
            _context: ProviderContext<'run, 'db, Self>,
            _db: &'db dyn Database,
            _cycle: &'call Cycle<'call>,
            _last: &'call Id,
            _value: Id,
            _input: Request,
        ) -> RunResult<Id>
        where
            'run: 'call,
        {
            Err(RunError::RequiresFetch)
        }
    }

    fn run_values(db: &TestDb, requests: &[Request]) -> RunResult<Vec<Id>> {
        let provider;
        let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
        let values = registry.finite_interned_values(
            db as &dyn Database,
            Interface::ingredient(db.zalsa()),
            member_count::fn_ingredient_(db, db.zalsa()),
        )?;
        let caller = registry.reserve(
            db as &dyn Database,
            map_caller::fn_ingredient_(db, db.zalsa()),
        )?;
        provider = Provider { values, caller };
        let route = provider.caller.clone();
        let binding = registry.provider(&provider)?;
        registry.bind_executable(&route, &binding)?;
        registry.seal()?.run(move |endpoint| async move {
            let _root = Marker("root");
            let context = endpoint.provider(binding)?;
            let mut ids = Vec::new();
            for request in requests {
                ids.push(*context.fetch_ref(&route, request.as_id())?.await?);
            }
            Ok(ids)
        })
    }

    fn run_fault(db: &'static TestDb, request: Request) -> RunResult<Vec<Id>> {
        let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
        let values = registry.finite_interned_values(
            db as &dyn Database,
            Interface::ingredient(db.zalsa()),
            member_count::fn_ingredient_(db, db.zalsa()),
        )?;
        let caller = registry.reserve(
            db as &dyn Database,
            map_caller::fn_ingredient_(db, db.zalsa()),
        )?;
        let provider: &'static _ = Box::leak(Box::new(Provider { values, caller }));
        let binding = registry.provider(provider)?;
        registry.bind_executable(&provider.caller, &binding)?;
        registry.seal()?.run(move |endpoint| async move {
            let _root = Marker("root");
            if let Some(hook) = state().values.hook.borrow().as_ref() {
                *hook.endpoint.borrow_mut() = Some(endpoint.clone());
            }
            Ok(vec![
                *endpoint
                    .provider(binding)?
                    .fetch_ref(&provider.caller, request.as_id())?
                    .await?,
            ])
        })
    }

    fn complete_values(db: &TestDb, requests: &[Request]) -> Vec<Id> {
        let outcome = try_with_attempt(db, 100_000, || run_values(db, requests)).unwrap();
        let AttemptOutcome::Complete(Ok(ids)) = outcome else {
            panic!("{outcome:?}")
        };
        assert!(!state().values.active.get());
        idle(db);
        ids
    }

    fn value_ids(db: &dyn Database) -> Vec<Id> {
        Interface::ingredient(db.zalsa())
            .entries(db.zalsa())
            .map(|entry| entry.key().key_index())
            .collect()
    }

    fn stale_values(profile: bool) -> (TestDb, Request, Id) {
        let mut db = TestDb::new();
        let request = Request::new(&db, if profile { u32::MAX } else { 0 }, 65, None);
        let count = <Interface<'static> as crate::interned::Configuration>::REVISIONS.get();
        let mut old = None;
        for index in 0..count {
            if index != 0 {
                request.set_left(&mut db).to(index as u32);
            }
            state().values.tag.set(usize::from(index == 0));
            let id = map_caller(&db, request);
            assert_eq!(member_count(&db, Interface::from_id(id)), 1);
            if index == 0 {
                old = Some(id);
            }
        }
        state().values.tag.set(0);
        request.set_left(&mut db).to(count as u32);
        let request = Request::new(&db, count as u32, 1, None);
        (db, request, old.unwrap())
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ValueFault {
        InputWork,
        Validate,
        OldWork,
        ReadWork,
        ReadBytes,
        Discard,
        QuietReuse,
    }

    struct ValueSnapshot {
        stage: &'static str,
        frame: Option<DatabaseKeyIndex>,
        claimed: bool,
        detached: usize,
        live: [usize; 3],
        reason: Option<Incomplete>,
        reads: Option<ReadState>,
    }

    struct ValueHook {
        db: &'static TestDb,
        caller: DatabaseKeyIndex,
        selected: Option<DatabaseKeyIndex>,
        fault: ValueFault,
        endpoint: RefCell<Option<TaskEndpoint<'static, 'static>>>,
        fired: Cell<bool>,
        input_quoted: Cell<bool>,
        detached: Rc<detached_observation::State>,
        notes: RefCell<Vec<ValueSnapshot>>,
    }

    impl ValueHook {
        fn note(&self, stage: &'static str) {
            let state = state();
            if stage == "child" {
                state.values.child_order.set(state.values.tick());
            }
            if stage == "caller" {
                state.values.caller_order.set(state.values.tick());
            }
            self.notes.borrow_mut().push(ValueSnapshot {
                stage,
                frame: self.db.zalsa_local().active_query().map(|(key, _)| key),
                claimed: matches!(
                    self.db
                        .zalsa()
                        .lookup_ingredient(self.caller.ingredient_index())
                        .as_function()
                        .unwrap()
                        .sync_table()
                        .peek_claim(self.db.zalsa(), self.caller.key_index(), Reentrancy::Deny,),
                    ClaimResult::Cycle { .. }
                ),
                detached: self.detached.live.get(),
                live: std::array::from_fn(|index| state.values.live[index].get()),
                reason: attempt_probe::current().and_then(|current| current.reason()),
                reads: self
                    .db
                    .zalsa_local()
                    .try_with_query_stack(|stack| stack.last().map(|query| query.read_state()))
                    .flatten(),
            });
        }

        fn fail(&self) {
            assert!(!self.fired.replace(true));
            self.note("boundary");
            let endpoint = self.endpoint.borrow_mut().take().unwrap();
            let marker = Marker("child");
            let _reply = endpoint
                .demand::<(), _>(move || async move {
                    let _marker = marker;
                    panic!("rejected finite-field request polled its child");
                })
                .unwrap();
            attempt_probe::report_incomplete(self.db, Incomplete::Interrupted);
        }
    }

    pub(super) fn note(stage: &'static str) {
        let hook = state().values.hook.borrow().clone();
        if let Some(hook) = hook {
            hook.note(stage);
        }
    }

    pub(super) fn admit(work: ExecutionWork) -> RunResult<()> {
        let state = state();
        if state.values.active.get()
            && let Some(hook) = state.values.hook.borrow().clone()
            && matches!(hook.fault, ValueFault::ReadWork | ValueFault::ReadBytes)
            && hook.notes.borrow().is_empty()
        {
            hook.note("request");
        }
        if state.values.active.get()
            && let Some(hook) = state.values.hook.borrow().clone()
            && !hook.fired.get()
            && hook.detached.live.get() == 1
            && match hook.fault {
                ValueFault::ReadWork => matches!(work, ExecutionWork::Work { .. }),
                ValueFault::ReadBytes => {
                    matches!(work, ExecutionWork::Resource { requested_bytes } if requested_bytes != 0)
                }
                _ => false,
            }
        {
            hook.fail();
            return Err(RunError::Refused(Incomplete::Interrupted));
        }
        if state.values.active.get()
            && let ExecutionWork::Work { units } = work
        {
            state.values.work.borrow_mut().push(units);
            let composed = state.values.composed.hook.borrow().clone();
            if let Some(hook) = composed {
                hook.admit(units)?;
            }
            let hook = state.values.hook.borrow().clone();
            if let Some(hook) = hook
                && !hook.fired.get()
                && ((hook.fault == ValueFault::InputWork && units == 2 && hook.input_quoted.get())
                    || (hook.fault == ValueFault::OldWork && units == 66))
            {
                hook.fail();
                return Err(RunError::Refused(Incomplete::Interrupted));
            }
        }
        Ok(())
    }

    pub(super) fn event(event: (KeyEvent, DatabaseKeyIndex)) {
        let composed = state().values.composed.hook.borrow().clone();
        if let Some(hook) = composed {
            hook.event(event);
        }
        let multiple = state().values.multiple_hook.borrow().clone();
        if let Some(hook) = multiple {
            hook.event(event);
        }
        let hook = state().values.hook.borrow().clone();
        let Some(hook) = hook else { return };
        if hook.fault == ValueFault::QuietReuse {
            if event.0 == KeyEvent::Discard && Some(event.1) == hook.selected {
                hook.fired.set(true);
                hook.note("discard");
            } else if event.0 == KeyEvent::Reuse
                && event.1.ingredient_index()
                    == Interface::ingredient(hook.db.zalsa()).ingredient_index()
            {
                hook.note("reuse");
            }
        } else if !hook.fired.get()
            && matches!(hook.fault, ValueFault::Validate | ValueFault::Discard)
            && Some(event.1) == hook.selected
            && event.0
                == if hook.fault == ValueFault::Validate {
                    KeyEvent::Validate
                } else {
                    KeyEvent::Discard
                }
        {
            hook.fail();
        }
    }

    fn fault_fixture(
        fault: ValueFault,
    ) -> (
        &'static TestDb,
        Request,
        Option<Id>,
        Rc<ValueHook>,
        impl Drop,
    ) {
        let (db, request, old) = if matches!(
            fault,
            ValueFault::OldWork
                | ValueFault::ReadWork
                | ValueFault::ReadBytes
                | ValueFault::Discard
                | ValueFault::QuietReuse
        ) {
            let (db, request, old) = stale_values(false);
            (db, request, Some(old))
        } else {
            let mut db = TestDb::new();
            let request = Request::new(&db, 0, 1, None);
            let old = if fault == ValueFault::Validate {
                state().values.tag.set(1);
                let id = map_caller(&db, request);
                state().values.tag.set(0);
                request.set_left(&mut db).to(1);
                Some(id)
            } else {
                None
            };
            let fresh = Request::new(&db, 0, 1, None);
            (db, fresh, old)
        };
        let db: &'static TestDb = Box::leak(Box::new(db));
        let selected = old.map(|id| {
            if fault == ValueFault::Validate {
                Interface::ingredient(db.zalsa()).database_key_index(id)
            } else {
                member_count::fn_ingredient_(db, db.zalsa()).database_key_index(id)
            }
        });
        let detached = Rc::new(detached_observation::State::default());
        let observer = detached_observation::install(detached.clone());
        let hook = Rc::new(ValueHook {
            db,
            caller: map_caller::fn_ingredient_(db, db.zalsa()).database_key_index(request.as_id()),
            selected,
            fault,
            endpoint: RefCell::new(None),
            fired: Cell::new(false),
            input_quoted: Cell::new(false),
            detached,
            notes: RefCell::new(Vec::new()),
        });
        let state = state();
        state.values.tag.set(2);
        *state.values.hook.borrow_mut() = Some(hook.clone());
        state.events.borrow_mut().clear();
        state.journal.borrow_mut().clear();
        (db, request, old, hook, observer)
    }

    fn rejected(db: &'static TestDb, request: Request, hook: &ValueHook) {
        let state = state();
        let result = try_with_attempt(db, 100_000, || {
            let result = run_fault(db, request);
            state.direct_error.set(result.as_ref().err().copied());
            result
        });
        assert_eq!(
            result,
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        );
        assert_eq!(
            state.direct_error.get(),
            Some(RunError::Refused(Incomplete::Interrupted))
        );
        assert!(hook.fired.get());
        assert!(state.delivered.borrow().is_empty());
        assert!(
            stored(
                db,
                map_caller::fn_ingredient_(db, db.zalsa()),
                request.as_id()
            )
            .is_none()
        );
        assert_eq!(&*state.journal.borrow(), &["child", "caller", "root"]);
        let notes = hook.notes.borrow();
        let child = notes.iter().find(|note| note.stage == "child").unwrap();
        assert_eq!(child.frame, Some(hook.caller));
        assert!(child.claimed);
        assert_eq!(child.reason, Some(Incomplete::Interrupted));
        let boundary = notes.iter().find(|note| note.stage == "boundary").unwrap();
        assert_eq!(boundary.frame, Some(hook.caller));
        assert!(boundary.claimed);
        drop(notes);
        idle(db);
        assert!(!state.values.active.get());
    }

    // A nonempty map needs two metadata visits. A cold insert also visits the vacant
    // slot, so its paid quotation quanta are one, two, and four; a hit stops at two.
    // Payload work is admitted after each complete input quote and again before commit.
    // The penultimate charge pays for reinspection, which commits before the final
    // dependency-read charge; refusal of that read therefore leaves the value interned.
    fn cold_map_work(fields: usize, read: usize) -> [usize; 18] {
        [1, 1, 1, 1, 1, 1, 1, 2, fields, 1, 1, 4, fields, 1, fields, 1, 4, read]
    }

    fn hit_map_work(fields: usize, read: usize) -> [usize; 14] {
        [1, 1, 1, 1, 1, 1, 1, 2, fields, 1, fields, 1, 2, read]
    }

    #[test]
    fn canonical_values_and_bulk_work() {
        let (state, _reset) = fixture();
        let db = TestDb::new();
        let requests = [0, 1, 65, 1].map(|width| Request::new(&db, 0, width, None));
        assert!(value_ids(&db).is_empty());
        let result = complete_values(&db, &requests);
        assert_eq!(result[1], result[3]);
        assert_eq!(state.values.attempted.get(), 4);
        assert_eq!(
            &*state.values.work.borrow(),
            &[
                &[1, 1, 1, 1, 1, 1, 1, 2, 1, 1, 2, 79][..],
                &cold_map_work(2, 79),
                &cold_map_work(66, 79),
                &hit_map_work(2, 79),
            ].concat()
        );
        assert_eq!(state.values.clones.get(), 0);
        for (request, id) in requests.into_iter().zip(result) {
            assert_eq!(map_caller(&db, request), id);
            assert_eq!(
                Interface::new(&db, Collision(0), OwnedMap::new(request.right(&db), 0)).as_id(),
                id
            );
            let value = Interface::from_id(id);
            assert_eq!(value.salt(&db), Collision(0));
            assert_eq!(
                value.members(&db).entries,
                OwnedMap::new(request.right(&db), 0).entries
            );
            let memo = stored(
                &db,
                map_caller::fn_ingredient_(&db, db.zalsa()),
                request.as_id(),
            )
            .unwrap();
            assert_eq!(memo.header.revisions.durability, Durability::LOW);
            assert!(
                memo.header
                    .origin()
                    .inputs()
                    .any(|key| key == Interface::ingredient(db.zalsa()).database_key_index(id))
            );
            assert_eq!(
                member_count(&db, value),
                usize::from(request.right(&db) != 0)
            );
            let pointer = std::ptr::from_ref(
                stored(&db, member_count::fn_ingredient_(&db, db.zalsa()), id).unwrap(),
            )
            .addr();
            let fresh = Request::new(&db, 0, request.right(&db), None);
            assert_eq!(complete_values(&db, &[fresh]), [id]);
            assert_eq!(
                std::ptr::from_ref(
                    stored(&db, member_count::fn_ingredient_(&db, db.zalsa()), id).unwrap()
                )
                .addr(),
                pointer
            );
        }
        assert_eq!(value_ids(&db).len(), 3);
    }

    #[test]
    fn value_sequences_share_allowance() {
        const READ_UNITS: usize = 79;
        for fresh in [false, true] {
            for budget in 0..=174 {
                let (state, _reset) = fixture();
                let db = TestDb::new();
                let requests = (0..5)
                    .map(|index| Request::new(&db, if fresh { index } else { 0 }, 1, None))
                    .collect::<Vec<_>>();
                let stamp = Stamp::current(&db);
                assert_eq!(
                    try_with_attempt(&db, budget, || run_values(&db, &requests)),
                    Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
                );
                // Input conversion costs three units; publication and the root's memo
                // read each cost one. A committed value survives refused dependency delivery.
                let mut remaining = budget;
                let mut attempted = 0;
                let mut delivered = 0;
                let mut committed = 0;
                let mut published = 0;
                let mut work = Vec::new();
                'requests: for index in 0..requests.len() {
                    if remaining < 3 {
                        break;
                    }
                    remaining -= 3;
                    attempted += 1;
                    let cold = cold_map_work(2, READ_UNITS);
                    let hit = hit_map_work(2, READ_UNITS);
                    let charges = if fresh || index == 0 { &cold[..] } else { &hit[..] };
                    for (step, &units) in charges.iter().enumerate() {
                        if remaining < units {
                            break 'requests;
                        }
                        remaining -= units;
                        work.push(units);
                        if step == charges.len() - 2 {
                            committed += 1;
                        }
                    }
                    delivered += 1;
                    if remaining == 0 {
                        break;
                    }
                    remaining -= 1;
                    published += 1;
                    if remaining == 0 {
                        break;
                    }
                    remaining -= 1;
                }
                assert_eq!(state.delivered.borrow().len(), delivered);
                assert_eq!(state.values.attempted.get(), attempted);
                assert_eq!(&*state.values.work.borrow(), &work);
                assert_eq!(value_ids(&db).len(), if fresh { committed } else { usize::from(committed != 0) });
                for request in &requests[..published] {
                    assert!(
                        stored(
                            &db,
                            map_caller::fn_ingredient_(&db, db.zalsa()),
                            request.as_id()
                        )
                        .is_some()
                    );
                }
                for request in &requests[published..] {
                    assert!(
                        stored(
                            &db,
                            map_caller::fn_ingredient_(&db, db.zalsa()),
                            request.as_id()
                        )
                        .is_none()
                    );
                }
                let result = complete_values(&db, &requests);
                for (request, id) in requests.iter().zip(result) {
                    assert_eq!(map_caller(&db, *request), id);
                }
                assert_eq!(Stamp::current(&db), stamp);
            }
        }
    }

    #[test]
    fn incoming_fields_survive_event_and_admission_refusal() {
        for fault in [ValueFault::InputWork, ValueFault::Validate] {
            let (state, _reset) = fixture();
            let (db, request, old, hook, _observer) = fault_fixture(fault);
            let stamp = Stamp::current(db);
            rejected(db, request, &hook);
            let notes = hook.notes.borrow();
            let child = notes.iter().find(|note| note.stage == "child").unwrap();
            assert_eq!(child.live[2], 1);
            assert_eq!(child.detached, 0);
            drop(notes);
            assert_eq!(state.values.live[2].get(), 0);
            assert_eq!(state.values.drops[2].get(), 1);
            assert_eq!(
                state.values.drop_reason[2].get(),
                Some(Incomplete::Interrupted)
            );
            assert!(state.values.child_order.get() < state.values.drop_order[2].get());
            assert!(state.values.drop_order[2].get() < state.values.caller_order.get());
            let hit_work = hit_map_work(2, 79);
            let expected = if fault == ValueFault::InputWork {
                &[1, 1, 1, 1, 1, 1, 1, 2, 2][..]
            } else {
                &hit_work[..]
            };
            assert_eq!(&*state.values.work.borrow(), expected);
            assert_eq!(value_ids(db).len(), usize::from(old.is_some()));
            *state.values.hook.borrow_mut() = None;
            state.values.tag.set(0);
            let id = complete_values(db, &[request])[0];
            if let Some(old) = old {
                assert_eq!(id, old);
            }
            assert_eq!(complete_values(db, &[request]), [id]);
            assert_eq!(Stamp::current(db), stamp);
        }
    }

    #[test]
    fn retired_fields_survive_reuse_boundaries() {
        for fault in [
            ValueFault::OldWork,
            ValueFault::Discard,
            ValueFault::QuietReuse,
        ] {
            let (state, _reset) = fixture();
            let (db, request, old, hook, _observer) = fault_fixture(fault);
            let stamp = Stamp::current(db);
            let old_memo = std::ptr::from_ref(stored(
                db,
                member_count::fn_ingredient_(db, db.zalsa()),
                old.unwrap(),
            ).unwrap()).addr();
            if fault == ValueFault::QuietReuse {
                assert!(matches!(
                    try_with_attempt(db, 100_000, || run_fault(db, request)),
                    Ok(AttemptOutcome::Complete(Ok(_)))
                ));
                assert!(hook.fired.get());
                let notes = hook.notes.borrow();
                for stage in ["discard", "reuse"] {
                    let note = notes.iter().find(|note| note.stage == stage).unwrap();
                    assert_eq!(note.detached, 1);
                    assert_eq!(note.live[1], 1);
                    assert_eq!(note.frame, Some(hook.caller));
                    assert!(note.claimed);
                }
            } else {
                rejected(db, request, &hook);
                if fault == ValueFault::OldWork {
                    let notes = hook.notes.borrow();
                    let child = notes.iter().find(|note| note.stage == "child").unwrap();
                    assert_eq!(child.detached, 0);
                    assert_eq!(child.live[1], 1);
                    assert_eq!(child.live[2], 1);
                    drop(notes);
                    assert_eq!(state.values.live[1].get(), 1);
                    assert_eq!(state.values.drops[1].get(), 0);
                    assert_eq!(state.values.drops[2].get(), 1);
                    assert!(state.values.child_order.get() < state.values.drop_order[2].get());
                    assert!(state.values.drop_order[2].get() < state.values.caller_order.get());
                    assert_eq!(hook.detached.acquired.get(), 0);
                    assert_eq!(hook.detached.retired.get(), 0);
                    assert!(value_ids(db).contains(&old.unwrap()));
                    assert_eq!(std::ptr::from_ref(stored(
                        db,
                        member_count::fn_ingredient_(db, db.zalsa()),
                        old.unwrap(),
                    ).unwrap()).addr(), old_memo);
                    assert!(state.events.borrow().is_empty());
                    assert_eq!(&*state.values.work.borrow(), &[
                        1, 1, 1, 1, 1, 1, 1, 2, 2, 1, 1, 4, 2, 1, 1, 8, 2,
                        1, 1, 16, 2, 1, 2, 66,
                    ]);
                    *state.values.hook.borrow_mut() = None;
                    state.values.tag.set(0);
                    let new = complete_values(db, &[request])[0];
                    assert_eq!(new.index(), old.unwrap().index());
                    assert_eq!(new.generation(), old.unwrap().generation() + 1);
                    assert_eq!(complete_values(db, &[request]), [new]);
                    assert_eq!(state.values.live[1].get(), 0);
                    assert_eq!(state.values.drops[1].get(), 1);
                    assert_eq!(member_count(db, Interface::from_id(new)), 1);
                    assert_eq!(Stamp::current(db), stamp);
                    continue;
                }
                let notes = hook.notes.borrow();
                let child = notes.iter().find(|note| note.stage == "child").unwrap();
                assert_eq!(child.live[1], 1);
                assert_eq!(child.detached, 1);
                assert!(state.values.child_order.get() < state.values.drop_order[1].get());
            }
            assert_eq!(&*state.values.work.borrow(), &[
                1, 1, 1, 1, 1, 1, 1, 2, 2, 1, 1, 4, 2, 1, 1, 8, 2,
                1, 1, 16, 2, 1, 2, 66, 1, 16, 79,
            ]);
            assert_eq!(state.values.live[1].get(), 0);
            assert_eq!(state.values.drops[1].get(), 1);
            assert_eq!(state.values.detached_at_drop[1].get(), 0);
            assert_eq!(
                (
                    hook.detached.live.get(),
                    hook.detached.acquired.get(),
                    hook.detached.retired.get()
                ),
                (0, 1, 1)
            );
            let old = old.unwrap();
            let new = value_ids(db)
                .into_iter()
                .find(|id| id.index() == old.index())
                .unwrap();
            assert_eq!(new.generation(), old.generation() + 1);
            assert!(stored(db, member_count::fn_ingredient_(db, db.zalsa()), new).is_none());
            let events = state.events.borrow();
            let discard = events
                .iter()
                .position(|event| event.0 == KeyEvent::Discard && Some(event.1) == hook.selected);
            let reuse = events
                .iter()
                .position(|event| event.0 == KeyEvent::Reuse && event.1.key_index() == new);
            match fault {
                ValueFault::Discard => {
                    assert!(discard.is_some());
                    assert!(reuse.is_none());
                }
                ValueFault::QuietReuse => assert!(discard.unwrap() < reuse.unwrap()),
                _ => unreachable!(),
            }
            drop(events);
            *state.values.hook.borrow_mut() = None;
            state.values.tag.set(0);
            assert_eq!(complete_values(db, &[request]), [new]);
            assert_eq!(complete_values(db, &[request]), [new]);
            assert_eq!(member_count(db, Interface::from_id(new)), 1);
            assert_eq!(Stamp::current(db), stamp);
        }
    }

    #[test]
    fn interned_value_read_refusal_retains_retired_generation_until_children_drain() {
        for fault in [ValueFault::ReadWork, ValueFault::ReadBytes] {
            let (state, _reset) = fixture();
            let (db, request, old, hook, _observer) = fault_fixture(fault);
            let stamp = Stamp::current(db);
            rejected(db, request, &hook);
            let notes = hook.notes.borrow();
            let request_state = notes.iter().find(|note| note.stage == "request").unwrap();
            let boundary = notes.iter().find(|note| note.stage == "boundary").unwrap();
            let child = notes.iter().find(|note| note.stage == "child").unwrap();
            assert!(boundary.reads.is_some());
            assert_eq!(request_state.reads, boundary.reads);
            assert_eq!(boundary.reads, child.reads);
            assert_eq!(child.live[1], 1);
            assert_eq!(child.detached, 1);
            drop(notes);
            assert!(state.values.child_order.get() < state.values.drop_order[1].get());
            assert!(state.values.drop_order[1].get() < state.values.caller_order.get());
            assert_eq!(state.values.live[1].get(), 0);
            assert_eq!(state.values.drops[1].get(), 1);
            assert_eq!(hook.detached.live.get(), 0);
            assert_eq!(hook.detached.acquired.get(), 1);
            assert_eq!(hook.detached.retired.get(), 1);
            assert!(state.events.borrow().is_empty());

            let old = old.unwrap();
            let new = value_ids(db)
                .into_iter()
                .find(|id| id.index() == old.index())
                .unwrap();
            assert_eq!(new.generation(), old.generation() + 1);
            *state.values.hook.borrow_mut() = None;
            state.values.tag.set(0);
            assert_eq!(complete_values(db, &[request]), [new]);
            assert_eq!(complete_values(db, &[request]), [new]);
            let memo = stored(
                db,
                map_caller::fn_ingredient_(db, db.zalsa()),
                request.as_id(),
            )
            .unwrap();
            let interned = Interface::ingredient(db.zalsa()).database_key_index(new);
            assert!(memo.header.origin().inputs().any(|input| input == interned));
            assert_eq!(Stamp::current(db), stamp);
        }
    }

    #[test]
    fn rejected_profile_restores_owned_input() {
        let (state, _reset) = fixture();
        let (db, request, old) = stale_values(true);
        let ingredient = member_count::fn_ingredient_(&db, db.zalsa());
        let old_memo = stored(&db, ingredient, old).unwrap();
        assert!(!old_memo.header.outputs_are_empty());
        assert_eq!(old_memo.header.revisions.tracked_struct_ids().len(), 1);
        let pointer = std::ptr::from_ref(old_memo).addr();
        let ids = value_ids(&db);
        let stamp = Stamp::current(&db);
        let detached = Rc::new(detached_observation::State::default());
        let _observer = detached_observation::install(detached.clone());
        state.values.tag.set(2);
        for count in 1..=2 {
            assert_eq!(
                try_with_attempt(&db, 100_000, || {
                    let result = run_values(&db, &[request]);
                    assert_eq!(
                        result,
                        Err(RunError::Contract(
                            "finite interned value memo has tracked outputs"
                        ))
                    );
                    result
                }),
                Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
            );
            assert_eq!(state.values.drops[2].get(), count);
            assert_eq!(state.values.live[2].get(), 0);
            assert_eq!(
                state.values.drop_reason[2].get(),
                Some(Incomplete::Interrupted)
            );
            assert_eq!(value_ids(&db), ids);
            assert_eq!(
                std::ptr::from_ref(stored(&db, ingredient, old).unwrap()).addr(),
                pointer
            );
            assert_eq!(
                stored(&db, ingredient, old)
                    .unwrap()
                    .header
                    .revisions
                    .tracked_struct_ids()
                    .len(),
                1
            );
            assert_eq!(detached.acquired.get(), 0);
            assert!(state.delivered.borrow().is_empty());
            assert!(
                stored(
                    &db,
                    map_caller::fn_ingredient_(&db, db.zalsa()),
                    request.as_id()
                )
                .is_none()
            );
            assert_eq!(Stamp::current(&db), stamp);
        }
        assert_eq!(state.values.clones.get(), 0);
    }

    #[crate::interned]
    struct MemoLess<'db> {
        #[returns(copy)]
        salt: Collision,
        #[returns(ref)]
        members: OwnedMap,
    }

    // These fields have the same finite operations as Interface, without attached function memos.
    impl FiniteInternedConfiguration for MemoLess<'static> {
        fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
            fields.1.work()
        }

        fn field_work_bounded(
            fields: &Self::Fields<'_>,
            fuel: &mut QuoteFuel,
        ) -> Result<usize, QuoteError> {
            fields.1.work_bounded(fuel)
        }
    }

    #[crate::tracked(returns(copy), attempt = ReturnOnly)]
    fn memo_less_caller(db: &dyn Database, request: Request) -> Id {
        MemoLess::new(
            db,
            Collision(request.left(db)),
            OwnedMap::new(request.right(db), state().values.tag.get()),
        )
        .as_id()
    }

    fn complete_memo_less(db: &TestDb, salt: u32, width: u32, tag: usize, count: usize) -> Vec<Id> {
        let outcome = try_with_attempt(db, 100_000, || {
            let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
            let values =
                registry.finite_interned_values_with_memos(MemoLess::ingredient(db.zalsa()), ())?;
            registry.seal()?.run(|endpoint| async move {
                let mut ids = Vec::new();
                for _ in 0..count {
                    let _scope = ValueScope::new();
                    let value = endpoint
                        .intern_value(&values, (Collision(salt), OwnedMap::new(width, tag)))
                        .await;
                    ids.push(value.as_id());
                }
                Ok(ids)
            })
        });
        let Ok(AttemptOutcome::Complete(Ok(ids))) = outcome else {
            panic!("zero-memo value request did not complete: {outcome:?}");
        };
        idle(db);
        assert!(!state().values.active.get());
        ids
    }

    #[test]
    fn empty_memo_schema_preserves_cold_hits_and_selected_field_retirement() {
        let (state, _reset) = fixture();
        {
            let db = TestDb::new();
            let owner = MemoLess::ingredient(db.zalsa());
            assert_eq!(owner.memo_table_types().len(), 0);
            assert_eq!(owner.entries(db.zalsa()).count(), 0);
            let ids = complete_memo_less(&db, 0, 3, 0, 2);
            assert_eq!(ids[0], ids[1]);
            assert_eq!(owner.entries(db.zalsa()).count(), 1);
            let value = MemoLess::from_id(ids[0]);
            assert_eq!(value.salt(&db), Collision(0));
            assert_eq!(value.members(&db).entries, OwnedMap::new(3, 0).entries);
            assert_eq!(&*state.values.work.borrow(), &[cold_map_work(4, 1).as_slice(), &hit_map_work(4, 1)].concat());
            assert_eq!(
                &*state.events.borrow(),
                &[(KeyEvent::Intern, owner.database_key_index(ids[0]))],
            );
            assert_eq!(
                MemoLess::new(&db, Collision(0), OwnedMap::new(3, 0)).as_id(),
                ids[0]
            );
        }

        let mut db = TestDb::new();
        let request = Request::new(&db, 0, 65, None);
        let revisions = <MemoLess<'static> as crate::interned::Configuration>::REVISIONS.get();
        let mut old = None;
        // Query-owned creation gives the canonical LRU a finite last-used revision.
        for index in 0..revisions {
            if index != 0 {
                request.set_left(&mut db).to(index as u32);
            }
            state.values.tag.set(usize::from(index == 0));
            let id = memo_less_caller(&db, request);
            if index == 0 {
                old = Some(id);
            }
        }
        state.values.tag.set(0);
        request.set_left(&mut db).to(revisions as u32);
        let old = old.expect("the canonical retention window has at least one revision");
        let owner = MemoLess::ingredient(db.zalsa());
        assert_eq!(owner.memo_table_types().len(), 0);
        let entry_count = owner.entries(db.zalsa()).count();
        assert_eq!(state.values.live[1].get(), 1);
        assert_eq!(state.values.drops[1].get(), 0);
        state.values.work.borrow_mut().clear();
        state.events.borrow_mut().clear();
        let stamp = Stamp::current(&db);
        let new = complete_memo_less(&db, revisions as u32, 1, 2, 1)[0];
        assert_eq!(new.index(), old.index());
        assert_eq!(new.generation(), old.generation() + 1);
        assert_eq!(owner.entries(db.zalsa()).count(), entry_count);
        assert_eq!(
            MemoLess::from_id(new).salt(&db),
            Collision(revisions as u32)
        );
        assert_eq!(
            MemoLess::from_id(new).members(&db).entries,
            OwnedMap::new(1, 0).entries
        );
        assert_eq!(&*state.values.work.borrow(), &[1, 1, 1, 1, 1, 1, 1, 2, 2, 1, 1, 4, 2, 1, 1, 8, 2, 1, 2, 66, 1, 8, 1]);
        assert_eq!(state.values.live[1].get(), 0);
        assert_eq!(state.values.drops[1].get(), 1);
        assert_eq!(state.values.live[2].get(), 1);
        assert_eq!(state.values.drops[2].get(), 0);
        assert_eq!(
            &*state.events.borrow(),
            &[(KeyEvent::Reuse, owner.database_key_index(new))],
        );
        assert_eq!(complete_memo_less(&db, revisions as u32, 1, 0, 1), [new]);
        assert_eq!(&*state.values.work.borrow(), &[
            &[1, 1, 1, 1, 1, 1, 1, 2, 2, 1, 1, 4, 2, 1, 1, 8, 2, 1, 2, 66, 1, 8, 1][..],
            &hit_map_work(2, 1),
        ].concat());
        assert_eq!(
            &*state.events.borrow(),
            &[(KeyEvent::Reuse, owner.database_key_index(new))],
        );
        assert_eq!(Stamp::current(&db), stamp);
        assert_eq!(state.values.clones.get(), 0);
    }

    #[test]
    fn empty_memo_schema_rejects_nonempty_and_foreign_owners() {
        let (state, _reset) = fixture();
        let db = TestDb::new();
        let foreign = TestDb::new();
        let nonempty = Interface::ingredient(db.zalsa());
        let _actual_memo = member_count::fn_ingredient_(&db, db.zalsa());
        assert_eq!(nonempty.memo_table_types().len(), 1);
        assert_eq!(nonempty.entries(db.zalsa()).count(), 0);
        let foreign_empty = MemoLess::ingredient(foreign.zalsa());
        assert_eq!(foreign_empty.memo_table_types().len(), 0);
        assert_eq!(foreign_empty.entries(foreign.zalsa()).count(), 0);
        assert!(state.events.borrow().is_empty());
        let outcome = try_with_attempt(&db, 100_000, || {
            let mut registry = RegistryBuilder::new(&db, &ADMISSION)?;
            assert!(matches!(
                registry.finite_interned_values_with_memos(nonempty, ()),
                Err(RunError::Contract(
                    "finite interned value memo mapping is unsupported"
                )),
            ));
            assert!(matches!(
                registry.finite_interned_values_with_memos(foreign_empty, ()),
                Err(RunError::Contract(
                    "finite interned value memo mapping is unsupported"
                )),
            ));
            Ok::<_, RunError>(())
        });
        assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
        assert_eq!(nonempty.entries(db.zalsa()).count(), 0);
        assert_eq!(foreign_empty.entries(foreign.zalsa()).count(), 0);
        assert!(state.events.borrow().is_empty());
        idle(&db);
    }

    mod passive_supertypes {
        use super::*;
        use crate::memo_ingredient_indices::MemoIngredientMap;

        #[crate::interned]
        #[derive(Debug)]
        struct AUnrelatedOwner<'db> {
            #[returns(copy)]
            value: u32,
        }

        impl FiniteInternedConfiguration for AUnrelatedOwner<'static> {
            fn field_work(_fields: &Self::Fields<'_>) -> Option<usize> {
                Some(1)
            }

            fn field_work_bounded(
                fields: &Self::Fields<'_>,
                fuel: &mut QuoteFuel,
            ) -> Result<usize, QuoteError> {
                fuel.consume(1)?;
                Self::field_work(fields).ok_or(QuoteError::Overflow)
            }
        }

        #[crate::interned]
        #[derive(Debug)]
        struct LeftOwner<'db> {
            #[returns(copy)]
            value: u32,
        }

        impl FiniteInternedConfiguration for LeftOwner<'static> {
            fn field_work(_fields: &Self::Fields<'_>) -> Option<usize> {
                Some(1)
            }

            fn field_work_bounded(
                fields: &Self::Fields<'_>,
                fuel: &mut QuoteFuel,
            ) -> Result<usize, QuoteError> {
                fuel.consume(1)?;
                Self::field_work(fields).ok_or(QuoteError::Overflow)
            }
        }

        #[crate::interned]
        #[derive(Debug)]
        struct RightOwner<'db> {
            #[returns(copy)]
            value: u32,
        }

        impl FiniteInternedConfiguration for RightOwner<'static> {
            fn field_work(_fields: &Self::Fields<'_>) -> Option<usize> {
                Some(1)
            }

            fn field_work_bounded(
                fields: &Self::Fields<'_>,
                fuel: &mut QuoteFuel,
            ) -> Result<usize, QuoteError> {
                fuel.consume(1)?;
                Self::field_work(fields).ok_or(QuoteError::Overflow)
            }
        }

        #[crate::interned]
        #[derive(Debug)]
        struct ZUnrelatedOwner<'db> {
            #[returns(copy)]
            value: u32,
        }

        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, crate::Supertype)]
        enum SharedOwner<'db> {
            Left(LeftOwner<'db>),
            Right(RightOwner<'db>),
        }

        #[crate::tracked(returns(copy), attempt = ReturnOnly)]
        fn direct_memo(db: &dyn Database, value: LeftOwner<'_>) -> u32 {
            state().ordinary.set(state().ordinary.get() + 1);
            value.value(db)
        }

        #[crate::tracked(returns(copy), attempt = ReturnOnly)]
        fn shared_memo(db: &dyn Database, value: SharedOwner<'_>) -> u32 {
            state().ordinary.set(state().ordinary.get() + 1);
            match value {
                SharedOwner::Left(value) => value.value(db),
                SharedOwner::Right(value) => value.value(db),
            }
        }

        #[crate::tracked(returns(copy), attempt = ReturnOnly)]
        fn unrelated_memo(db: &dyn Database, value: AUnrelatedOwner<'_>) -> u32 {
            state().ordinary.set(state().ordinary.get() + 1);
            value.value(db)
        }

        #[test]
        fn shared_query_registers_each_owner_with_its_actual_slots() {
            let (state, _reset) = fixture();
            let db = TestDb::new();
            let left = LeftOwner::ingredient(db.zalsa());
            let right = RightOwner::ingredient(db.zalsa());
            let direct = direct_memo::fn_ingredient_(&db, db.zalsa());
            let shared = shared_memo::fn_ingredient_(&db, db.zalsa());
            assert_eq!(left.memo_table_types().len(), 2);
            assert_eq!(right.memo_table_types().len(), 1);
            assert_ne!(
                shared.memo_ingredient_indices.get(left.ingredient_index()),
                shared.memo_ingredient_indices.get(right.ingredient_index()),
            );

            let outcome = try_with_attempt(&db, 100_000, || {
                let mut registry = RegistryBuilder::new(&db, &ADMISSION)?;
                let direct = registry.passive_memo::<_, _, CopyMemoProfile>(left, direct)?;
                let shared_left = registry.passive_memo::<_, _, CopyMemoProfile>(left, shared)?;
                let shared_right = registry.passive_memo::<_, _, CopyMemoProfile>(right, shared)?;
                let left =
                    registry.finite_interned_values_with_memos(left, (shared_left, direct))?;
                let right = registry.finite_interned_values_with_memos(right, (shared_right,))?;
                assert_eq!(state.ordinary.get(), 0);
                registry.seal()?.run(|endpoint| async move {
                    let first = endpoint.intern_value(&left, (7,)).await;
                    let second = endpoint.intern_value(&left, (7,)).await;
                    assert_eq!(first, second);
                    Ok((first, endpoint.intern_value(&right, (11,)).await))
                })
            });
            let Ok(AttemptOutcome::Complete(Ok((left, right)))) = outcome else {
                panic!("shared-supertype registration did not complete: {outcome:?}");
            };
            assert_eq!(state.ordinary.get(), 0);
            assert_eq!(left, LeftOwner::new(&db, 7));
            assert_eq!(right, RightOwner::new(&db, 11));
            assert_eq!(direct_memo(&db, left), 7);
            assert_eq!(shared_memo(&db, SharedOwner::Left(left)), 7);
            assert_eq!(shared_memo(&db, SharedOwner::Right(right)), 11);
            idle(&db);
        }

        #[test]
        fn unrelated_owners_in_and_beyond_the_supertype_map_are_rejected() {
            let (state, _reset) = fixture();
            let db = TestDb::new();
            let before = AUnrelatedOwner::ingredient(db.zalsa());
            let after = ZUnrelatedOwner::ingredient(db.zalsa());
            let left = LeftOwner::ingredient(db.zalsa());
            let right = RightOwner::ingredient(db.zalsa());
            let shared = shared_memo::fn_ingredient_(&db, db.zalsa());
            let direct = direct_memo::fn_ingredient_(&db, db.zalsa());
            assert!(before.ingredient_index() < left.ingredient_index());
            assert!(after.ingredient_index() > right.ingredient_index());
            assert_eq!(
                shared
                    .memo_ingredient_indices
                    .get_checked(before.ingredient_index()),
                None
            );
            assert_eq!(
                shared
                    .memo_ingredient_indices
                    .get_checked(after.ingredient_index()),
                None
            );
            // Equal output types and slot numbers do not identify the same Memo<C>.
            assert_eq!(before.memo_table_types().len(), 1);

            let outcome = try_with_attempt(&db, 100_000, || {
                let mut registry = RegistryBuilder::new(&db, &ADMISSION)?;
                assert!(matches!(
                    registry.passive_memo::<_, _, CopyMemoProfile>(before, shared),
                    Err(RunError::Contract(
                        "finite interned value memo mapping is unsupported"
                    )),
                ));
                assert!(matches!(
                    registry.passive_memo::<_, _, CopyMemoProfile>(after, shared),
                    Err(RunError::Contract(
                        "finite interned value memo mapping is unsupported"
                    )),
                ));
                assert!(matches!(
                    registry.passive_memo::<_, _, CopyMemoProfile>(before, direct),
                    Err(RunError::Contract(
                        "finite interned value memo mapping is unsupported"
                    )),
                ));
                Ok::<_, RunError>(())
            });
            assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
            assert_eq!(state.ordinary.get(), 0);
            assert!(state.events.borrow().is_empty());
            idle(&db);
        }

        #[test]
        fn exact_owner_key_still_registers_and_interns_by_equality() {
            let (state, _reset) = fixture();
            let db = TestDb::new();
            let owner = AUnrelatedOwner::ingredient(db.zalsa());
            let memo = unrelated_memo::fn_ingredient_(&db, db.zalsa());
            let outcome = try_with_attempt(&db, 100_000, || {
                let mut registry = RegistryBuilder::new(&db, &ADMISSION)?;
                let memo = registry.passive_memo::<_, _, CopyMemoProfile>(owner, memo)?;
                let values = registry.finite_interned_values_with_memos(owner, (memo,))?;
                registry.seal()?.run(|endpoint| async move {
                    let first = endpoint.intern_value(&values, (7,)).await;
                    assert_eq!(first, endpoint.intern_value(&values, (7,)).await);
                    assert_ne!(first, endpoint.intern_value(&values, (8,)).await);
                    Ok(first)
                })
            });
            let Ok(AttemptOutcome::Complete(Ok(value))) = outcome else {
                panic!("exact-owner registration did not complete: {outcome:?}");
            };
            assert_eq!(state.ordinary.get(), 0);
            assert_eq!(value, AUnrelatedOwner::new(&db, 7));
            assert_eq!(unrelated_memo(&db, value), 7);
            assert_eq!(owner.entries(db.zalsa()).count(), 2);
            idle(&db);
        }
    }

    #[crate::interned]
    #[derive(Debug)]
    struct MultipleMemos<'db> {
        #[returns(copy)]
        salt: Collision,
        #[returns(copy)]
        metadata: u32,
        #[returns(ref)]
        members: OwnedMap,
    }

    impl FiniteInternedConfiguration for MultipleMemos<'static> {
        fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
            fields.2.work()
        }

        fn field_work_bounded(
            fields: &Self::Fields<'_>,
            fuel: &mut QuoteFuel,
        ) -> Result<usize, QuoteError> {
            fields.2.work_bounded(fuel)
        }
    }

    fn multiple_metadata(db: &dyn Database, salt: u32, metadata: u32, function: u32) {
        // The low bits select a real function; bit 2 selects direct accumulation.
        if metadata & 3 != function {
            return;
        }
        if metadata & 4 == 0 {
            let _output = Produced::new(db, salt);
        }
        #[cfg(feature = "accumulator")]
        if metadata & 4 != 0 {
            Warning { _value: salt }.accumulate(db);
        }
    }

    #[crate::tracked(returns(copy), attempt = ReturnOnly)]
    fn first_memo(db: &dyn Database, value: MultipleMemos<'_>) -> usize {
        value.salt(db).0 as usize
            + value
                .members(db)
                .entries
                .values()
                .map(|value| *value as usize)
                .sum::<usize>()
    }

    #[crate::tracked(returns(copy), attempt = ReturnOnly)]
    fn second_memo(db: &dyn Database, value: MultipleMemos<'_>) -> bool {
        value.salt(db).0 % 2 == 0
    }

    fn multiple_fields(db: &dyn Database, request: Request) -> (Collision, u32, OwnedMap) {
        let salt = request.left(db);
        (
            Collision(salt),
            request.right(db),
            OwnedMap::new(salt + 1, state().values.tag.get()),
        )
    }

    #[crate::tracked(returns(copy), attempt = ReturnOnly)]
    fn multiple_caller(db: &dyn Database, request: Request) -> Id {
        let (salt, metadata, members) = multiple_fields(db, request);
        MultipleMemos::new(db, salt, metadata, members).as_id()
    }

    #[test]
    fn complete_two_memo_schema_interns_cold_and_reuses_hits() {
        let (_state, _reset) = fixture();
        let db = TestDb::new();
        let owner = MultipleMemos::ingredient(db.zalsa());
        let first = first_memo::fn_ingredient_(&db, db.zalsa());
        let second = second_memo::fn_ingredient_(&db, db.zalsa());
        assert_eq!(owner.entries(db.zalsa()).count(), 0);

        let outcome = try_with_attempt(&db, 100_000, || {
            let mut registry = RegistryBuilder::new(&db, &ADMISSION)?;
            let first = registry.passive_memo::<_, _, CopyMemoProfile>(owner, first)?;
            let second = registry.passive_memo::<_, _, CopyMemoProfile>(owner, second)?;
            // Descriptor order is independent of the ingredient's actual memo-slot order.
            let values = registry.finite_interned_values_with_memos(owner, (second, first))?;
            registry.seal()?.run(|endpoint| async move {
                let first = endpoint
                    .intern_value(&values, (Collision(0), 0, OwnedMap::new(3, 0)))
                    .await;
                let second = endpoint
                    .intern_value(&values, (Collision(0), 0, OwnedMap::new(3, 0)))
                    .await;
                assert_eq!(first, second);
                Ok(first)
            })
        });
        let Ok(AttemptOutcome::Complete(Ok(value))) = outcome else {
            panic!("two-memo value request did not complete: {outcome:?}");
        };
        assert_eq!(first_memo(&db, value), 3);
        assert!(second_memo(&db, value));
        assert_eq!(owner.entries(db.zalsa()).count(), 1);
    }

    #[crate::input]
    struct ComposedRequest {
        #[returns(copy)]
        left: u32,
        #[returns(copy)]
        right: u32,
    }

    fn composed_fields(db: &dyn Database, request: ComposedRequest) -> (Collision, u32, OwnedMap) {
        let salt = request.left(db);
        (
            Collision(salt),
            request.right(db),
            OwnedMap::new(salt + 1, state().values.tag.get()),
        )
    }

    struct ComposedProvider<'db, I: FiniteInternedConfiguration, S, D: Configuration> {
        values: InternedValues<'db, I, S>,
        caller: Route<'db, D>,
    }

    impl<'run, 'db: 'run, I, S, D> ExecutableRouteProvider<'run, 'db, D>
        for ComposedProvider<'db, I, S, D>
    where
        I: FiniteInternedConfiguration
            + for<'a> crate::interned::Configuration<Fields<'a> = (Collision, u32, OwnedMap)>,
        S: PassiveMemoSchema<'db, I> + 'run,
        D: for<'a> Configuration<
                DbView = dyn Database,
                Input<'a> = ComposedRequest,
                Output<'a> = Id,
            >,
    {
        // ComposedRequest conversion constructs a handle; Id equality compares its scalar identity.
        fixture_native_value!(executable, 'run, 'db, D, 1);

        async fn body(
            &'run self,
            context: ProviderContext<'run, 'db, Self>,
            db: &'db dyn Database,
            input: ComposedRequest,
        ) -> RunResult<Id> {
            let _caller = Marker("caller");
            let fields = composed_fields(db, input);
            let value = {
                let _scope = ValueScope::new();
                context.endpoint().intern_value(&self.values, fields).await
            };
            state().delivered.borrow_mut().push(value.as_id());
            Ok(value.as_id())
        }

        async fn initial(
            &'run self,
            _context: ProviderContext<'run, 'db, Self>,
            _db: &'db dyn Database,
            _id: Id,
            _input: ComposedRequest,
        ) -> RunResult<Id> {
            Err(RunError::RequiresFetch)
        }

        async fn recover<'call>(
            &'run self,
            _context: ProviderContext<'run, 'db, Self>,
            _db: &'db dyn Database,
            _cycle: &'call Cycle<'call>,
            _last: &'call Id,
            _value: Id,
            _input: ComposedRequest,
        ) -> RunResult<Id>
        where
            'run: 'call,
        {
            Err(RunError::RequiresFetch)
        }
    }

    #[crate::interned]
    #[derive(Debug)]
    struct ThreeMemos<'db> {
        #[returns(copy)]
        salt: Collision,
        #[returns(copy)]
        metadata: u32,
        #[returns(ref)]
        members: OwnedMap,
    }

    impl FiniteInternedConfiguration for ThreeMemos<'static> {
        fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
            fields.2.work()
        }

        fn field_work_bounded(
            fields: &Self::Fields<'_>,
            fuel: &mut QuoteFuel,
        ) -> Result<usize, QuoteError> {
            fields.2.work_bounded(fuel)
        }
    }

    #[crate::tracked(returns(copy), attempt = ReturnOnly)]
    fn composed_count(db: &dyn Database, value: ThreeMemos<'_>) -> usize {
        value.members(db).entries.len()
    }

    #[crate::tracked(returns(copy), attempt = ReturnOnly)]
    fn composed_even(db: &dyn Database, value: ThreeMemos<'_>) -> bool {
        value.salt(db).0 % 2 == 0
    }

    #[crate::tracked(returns(ref), attempt = ReturnOnly)]
    fn composed_payload(db: &dyn Database, value: ThreeMemos<'_>) -> Box<[u32]> {
        vec![value.salt(db).0, value.metadata(db)].into_boxed_slice()
    }

    struct BoxedWordsProfile;

    // The boxed slice owns scalar words and cannot run semantic work during destruction.
    impl<C> PassiveMemoProfile<C> for BoxedWordsProfile
    where
        C: for<'db> Configuration<SalsaStruct<'db> = ThreeMemos<'db>, Output<'db> = Box<[u32]>>,
    {
        fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
            Some(output.len())
        }

        fn retired_output_work_bounded<'db>(
            output: &C::Output<'db>,
            fuel: &mut QuoteFuel,
        ) -> Result<usize, QuoteError> {
            fuel.consume(1)?;
            <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
        }
    }

    #[crate::tracked(returns(copy), attempt = ReturnOnly)]
    fn composed_caller(db: &dyn Database, request: ComposedRequest) -> Id {
        let (salt, metadata, members) = composed_fields(db, request);
        ThreeMemos::new(db, salt, metadata, members).as_id()
    }

    #[test]
    fn composed_three_memo_schema_accepts_profiled_outputs() {
        let (state, _reset) = fixture();
        let db = TestDb::new();
        let first_request = ComposedRequest::new(&db, 2, 7);
        let second_request = ComposedRequest::new(&db, 2, 7);
        let owner = ThreeMemos::ingredient(db.zalsa());
        let count = composed_count::fn_ingredient_(&db, db.zalsa());
        let even = composed_even::fn_ingredient_(&db, db.zalsa());
        let payload = composed_payload::fn_ingredient_(&db, db.zalsa());
        assert_eq!(owner.entries(db.zalsa()).count(), 0);

        let outcome = try_with_attempt(&db, 100_000, || {
            let provider;
            let mut registry = RegistryBuilder::new(&db, &ADMISSION)?;
            let count = registry.passive_memo::<_, _, CopyMemoProfile>(owner, count)?;
            let even = registry.passive_memo::<_, _, CopyMemoProfile>(owner, even)?;
            let payload = registry.passive_memo::<_, _, BoxedWordsProfile>(owner, payload)?;
            let memos = PassiveMemoGroup::new((count, even), (payload,));
            let values = registry.finite_interned_values_with_memos(owner, memos)?;
            let caller = registry.reserve(
                &db as &dyn Database,
                composed_caller::fn_ingredient_(&db, db.zalsa()),
            )?;
            provider = ComposedProvider { values, caller };
            let route = provider.caller.clone();
            let binding = registry.provider(&provider)?;
            registry.bind_executable(&route, &binding)?;
            registry.seal()?.run(move |endpoint| async move {
                let context = endpoint.provider(binding)?;
                let first = *context.fetch_ref(&route, first_request.as_id())?.await?;
                let second = *context.fetch_ref(&route, second_request.as_id())?.await?;
                assert_eq!(first, second);
                Ok(first)
            })
        });
        let Ok(AttemptOutcome::Complete(Ok(id))) = outcome else {
            panic!("three-memo value request did not complete: {outcome:?}");
        };
        idle(&db);
        assert!(!state.values.active.get());
        assert_eq!(state.values.attempted.get(), 2);
        assert_eq!(&*state.values.work.borrow(), &[cold_map_work(4, 79).as_slice(), &hit_map_work(4, 79)].concat());
        assert_eq!(owner.entries(db.zalsa()).count(), 1);
        let value = ThreeMemos::from_id(id);
        for request in [first_request, second_request] {
            assert_eq!(composed_caller(&db, request), id);
            let memo = stored(
                &db,
                composed_caller::fn_ingredient_(&db, db.zalsa()),
                request.as_id(),
            )
            .unwrap();
            assert!(
                memo.header
                    .origin()
                    .inputs()
                    .any(|key| key == owner.database_key_index(id))
            );
        }

        let ordinary = TestDb::new();
        let request = ComposedRequest::new(&ordinary, 2, 7);
        let expected = ThreeMemos::from_id(composed_caller(&ordinary, request));
        assert_eq!(
            composed_count(&db, value),
            composed_count(&ordinary, expected)
        );
        assert_eq!(
            composed_even(&db, value),
            composed_even(&ordinary, expected)
        );
        assert_eq!(
            composed_payload(&db, value),
            composed_payload(&ordinary, expected)
        );
    }

    #[derive(Default)]
    struct ComposedState {
        payload_tag: Cell<usize>,
        live: [Cell<usize>; 7],
        drops: [Cell<usize>; 7],
        drop_order: [Cell<usize>; 7],
        hook: RefCell<Option<Rc<ComposedHook>>>,
    }

    #[test]
    fn composed_three_memo_schema_accepts_reversed_and_empty_fragments() {
        let (_state, _reset) = fixture();
        let db = TestDb::new();
        let owner = ThreeMemos::ingredient(db.zalsa());
        let outcome = try_with_attempt(&db, 100_000, || {
            let mut registry = RegistryBuilder::new(&db, &ADMISSION)?;
            let count = registry.passive_memo::<_, _, CopyMemoProfile>(
                owner,
                composed_count::fn_ingredient_(&db, db.zalsa()),
            )?;
            let even = registry.passive_memo::<_, _, CopyMemoProfile>(
                owner,
                composed_even::fn_ingredient_(&db, db.zalsa()),
            )?;
            let payload = registry.passive_memo::<_, _, BoxedWordsProfile>(
                owner,
                composed_payload::fn_ingredient_(&db, db.zalsa()),
            )?;
            let memos = PassiveMemoGroup::new((payload,), PassiveMemoGroup::new((), (even, count)));
            let values = registry.finite_interned_values_with_memos(owner, memos)?;
            registry.seal()?.run(move |endpoint| async move {
                let first = endpoint
                    .intern_value(&values, (Collision(2), 7, OwnedMap::new(3, 0)))
                    .await;
                let second = endpoint
                    .intern_value(&values, (Collision(2), 7, OwnedMap::new(3, 0)))
                    .await;
                assert_eq!(first, second);
                Ok(first)
            })
        });
        let Ok(AttemptOutcome::Complete(Ok(value))) = outcome else {
            panic!("{outcome:?}")
        };
        assert_eq!(owner.entries(db.zalsa()).count(), 1);
        assert_eq!(composed_count(&db, value), 1);
        assert!(composed_even(&db, value));
        assert_eq!(composed_payload(&db, value).as_ref(), &[2, 7]);
        idle(&db);
    }

    #[derive(Debug, crate::SalsaValue)]
    struct PassiveWords {
        words: Box<[u32]>,
        kind: usize,
        tag: usize,
    }

    impl PassiveWords {
        fn new(salt: u32, kind: usize) -> Self {
            let state = state();
            let base = state.values.composed.payload_tag.get();
            let tag = if base == 0 { 0 } else { base + kind };
            if tag != 0 {
                state.values.composed.live[tag].set(state.values.composed.live[tag].get() + 1);
            }
            Self {
                words: vec![salt; salt as usize + 3 + kind * 2].into_boxed_slice(),
                kind,
                tag,
            }
        }
    }

    impl Clone for PassiveWords {
        fn clone(&self) -> Self {
            Self {
                words: self.words.clone(),
                kind: self.kind,
                tag: 0,
            }
        }
    }

    impl PartialEq for PassiveWords {
        fn eq(&self, other: &Self) -> bool {
            self.words == other.words && self.kind == other.kind
        }
    }

    impl Eq for PassiveWords {}

    impl Drop for PassiveWords {
        fn drop(&mut self) {
            if self.tag == 0 {
                return;
            }
            // The observer records only scalar destruction state; it cannot enter Salsa.
            let _ = STATE.try_with(|slot| {
                let slot = slot.borrow();
                if let Some(state) = slot.as_ref() {
                    let observed = &state.values.composed;
                    observed.live[self.tag].set(observed.live[self.tag].get() - 1);
                    observed.drops[self.tag].set(observed.drops[self.tag].get() + 1);
                    observed.drop_order[self.tag].set(state.values.tick());
                }
            });
        }
    }

    #[crate::interned]
    #[derive(Debug)]
    struct SixMemos<'db> {
        #[returns(copy)]
        salt: Collision,
        #[returns(copy)]
        metadata: u32,
        #[returns(ref)]
        members: OwnedMap,
    }

    impl FiniteInternedConfiguration for SixMemos<'static> {
        fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
            fields.2.work()
        }

        fn field_work_bounded(
            fields: &Self::Fields<'_>,
            fuel: &mut QuoteFuel,
        ) -> Result<usize, QuoteError> {
            fields.2.work_bounded(fuel)
        }
    }

    #[crate::tracked(returns(copy), attempt = ReturnOnly)]
    fn six_count(db: &dyn Database, value: SixMemos<'_>) -> usize {
        value.members(db).entries.len()
    }

    #[crate::tracked(returns(copy), attempt = ReturnOnly)]
    fn six_even(db: &dyn Database, value: SixMemos<'_>) -> bool {
        value.salt(db).0 % 2 == 0
    }

    #[crate::tracked(returns(ref), attempt = ReturnOnly)]
    fn six_left(db: &dyn Database, value: SixMemos<'_>) -> PassiveWords {
        PassiveWords::new(value.salt(db).0, 0)
    }

    #[crate::tracked(returns(ref), attempt = ReturnOnly, lru = 1)]
    fn six_right(db: &dyn Database, value: SixMemos<'_>) -> PassiveWords {
        PassiveWords::new(value.salt(db).0, 1)
    }

    #[crate::tracked(returns(ref), attempt = CompleteOnly)]
    fn six_historical(db: &dyn Database, value: SixMemos<'_>) -> PassiveWords {
        let salt = value.salt(db).0;
        if value.metadata(db) & 3 == 1 {
            let _output = Produced::new(db, salt);
        }
        #[cfg(feature = "accumulator")]
        if value.metadata(db) & 3 == 2 {
            Warning { _value: salt }.accumulate(db);
        }
        PassiveWords::new(salt, 2)
    }

    #[crate::tracked(returns(copy), attempt = CompleteOnly)]
    fn six_indirect_source(db: &dyn Database, salt: u32, accumulated: bool) -> u32 {
        db.report_untracked_read();
        #[cfg(feature = "accumulator")]
        if accumulated {
            Warning { _value: salt }.accumulate(db);
        }
        let _ = accumulated;
        salt
    }

    #[crate::tracked(returns(copy), attempt = CompleteOnly)]
    fn six_indirect(db: &dyn Database, value: SixMemos<'_>) -> u32 {
        six_indirect_source(db, value.salt(db).0, value.metadata(db) & 4 != 0)
    }

    struct WordsProfile<const MODE: u8>;

    // Every output owns a finite boxed scalar slice. No payload destructor calls semantic code.
    impl<C, const MODE: u8> PassiveMemoProfile<C> for WordsProfile<MODE>
    where
        C: for<'db> Configuration<SalsaStruct<'db> = SixMemos<'db>, Output<'db> = PassiveWords>,
    {
        fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
            match MODE {
                1 => None,
                2 => Some(usize::MAX),
                _ => Some(output.words.len()),
            }
        }

        fn retired_output_work_bounded<'db>(
            output: &C::Output<'db>,
            fuel: &mut QuoteFuel,
        ) -> Result<usize, QuoteError> {
            fuel.consume(1)?;
            <Self as PassiveMemoProfile<C>>::retired_output_work(output)
                .ok_or(QuoteError::Unsupported)
        }
    }

    #[crate::tracked(returns(copy), attempt = ReturnOnly)]
    fn six_caller(db: &dyn Database, request: ComposedRequest) -> Id {
        let (salt, metadata, members) = composed_fields(db, request);
        SixMemos::new(db, salt, metadata, members).as_id()
    }

    fn finish_six<S>(
        mut registry: RegistryBuilder<'static, 'static>,
        values: InternedValues<'static, SixMemos<'static>, S>,
        db: &'static TestDb,
        request: ComposedRequest,
    ) -> RunResult<Id>
    where
        S: PassiveMemoSchema<'static, SixMemos<'static>> + 'static,
    {
        let caller = registry.reserve(
            db as &dyn Database,
            six_caller::fn_ingredient_(db, db.zalsa()),
        )?;
        let provider: &'static _ = Box::leak(Box::new(ComposedProvider { values, caller }));
        let binding = registry.provider(provider)?;
        registry.bind_executable(&provider.caller, &binding)?;
        registry.seal()?.run(move |endpoint| async move {
            let _root = Marker("root");
            if let Some(hook) = state().values.composed.hook.borrow().as_ref() {
                *hook.owner.endpoint.borrow_mut() = Some(endpoint.clone());
            }
            Ok(*endpoint
                .provider(binding)?
                .fetch_ref(&provider.caller, request.as_id())?
                .await?)
        })
    }

    fn run_six<const LEFT: u8, const RIGHT: u8, const HISTORICAL: u8>(
        db: &'static TestDb,
        request: ComposedRequest,
        reverse: bool,
    ) -> RunResult<Id> {
        let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
        let owner = SixMemos::ingredient(db.zalsa());
        let a = registry.passive_memo::<_, _, CopyMemoProfile>(
            owner,
            six_count::fn_ingredient_(db, db.zalsa()),
        )?;
        let b = registry.passive_memo::<_, _, CopyMemoProfile>(
            owner,
            six_even::fn_ingredient_(db, db.zalsa()),
        )?;
        let c = registry.passive_memo::<_, _, WordsProfile<LEFT>>(
            owner,
            six_left::fn_ingredient_(db, db.zalsa()),
        )?;
        let d = registry.passive_memo::<_, _, WordsProfile<RIGHT>>(
            owner,
            six_right::fn_ingredient_(db, db.zalsa()),
        )?;
        let e = registry.passive_memo::<_, _, WordsProfile<HISTORICAL>>(
            owner,
            six_historical::fn_ingredient_(db, db.zalsa()),
        )?;
        let f = registry.passive_memo::<_, _, CopyMemoProfile>(
            owner,
            six_indirect::fn_ingredient_(db, db.zalsa()),
        )?;
        if reverse {
            let memos = PassiveMemoGroup::new(
                (f, e),
                PassiveMemoGroup::new((d,), PassiveMemoGroup::new((b, c), (a,))),
            );
            let values = registry.finite_interned_values_with_memos(owner, memos)?;
            finish_six(registry, values, db, request)
        } else {
            let memos = PassiveMemoGroup::new(
                PassiveMemoGroup::new((a, c), (b,)),
                PassiveMemoGroup::new((d, e), (f,)),
            );
            let values = registry.finite_interned_values_with_memos(owner, memos)?;
            finish_six(registry, values, db, request)
        }
    }

    fn complete_six(db: &'static TestDb, request: ComposedRequest, reverse: bool) -> Id {
        let outcome = try_with_attempt(db, 100_000, || run_six::<0, 0, 0>(db, request, reverse));
        let Ok(AttemptOutcome::Complete(Ok(id))) = outcome else {
            panic!("{outcome:?}")
        };
        idle(db);
        assert!(!state().values.active.get());
        id
    }

    async fn report_value_layout<'call, 'run: 'call, 'db: 'run, I, S>(
        endpoint: &'call TaskEndpoint<'run, 'db>,
        values: &'call InternedValues<'db, I, S>,
        fields: I::Fields<'db>,
        case: &'static str,
    ) -> I::Struct<'db>
    where
        I: FiniteInternedConfiguration,
        S: PassiveMemoSchema<'db, I> + 'call,
    {
        for (carrier, size, alignment) in value_layout_for_tests::<I, S>() {
            eprintln!("COMPOSED_VALUE_LAYOUT {case} {carrier} size={size} align={alignment}");
        }
        let _scope = ValueScope::new();
        let future = endpoint.intern_value(values, fields);
        eprintln!(
            "COMPOSED_VALUE_LAYOUT {case} InternValueFuture size={} align={}",
            size_of_val(&future),
            align_of_val(&future)
        );
        future.await
    }

    fn complete_value_layout<'run, 'db: 'run, I, S>(
        registry: RegistryBuilder<'run, 'db>,
        values: InternedValues<'db, I, S>,
        fields: [I::Fields<'db>; 2],
        case: &'static str,
    ) -> RunResult<Id>
    where
        I: FiniteInternedConfiguration,
        I::Fields<'db>: 'run,
        S: PassiveMemoSchema<'db, I> + 'run,
    {
        let db = values.db();
        let owner = values.ingredient;
        assert_eq!(owner.entries(db.zalsa()).count(), 0);
        let id = registry.seal()?.run(move |endpoint| async move {
            let [cold_fields, hit_fields] = fields;
            let first = report_value_layout(&endpoint, &values, cold_fields, case)
                .await
                .as_id();
            let second = {
                let _scope = ValueScope::new();
                endpoint.intern_value(&values, hit_fields).await.as_id()
            };
            assert_eq!(first, second);
            Ok(first)
        })?;
        assert_eq!(owner.entries(db.zalsa()).count(), 1);
        Ok(id)
    }

    #[test]
    fn finite_value_schema_layouts() {
        enum Case {
            Zero,
            One,
            Two,
            Three { reverse: bool },
            Six { reverse: bool },
        }

        for (case, name) in [
            (Case::Zero, "zero"),
            (Case::One, "one"),
            (Case::Two, "two"),
            (Case::Three { reverse: false }, "three"),
            (Case::Three { reverse: true }, "three_empty_reverse"),
            (Case::Six { reverse: false }, "six_balanced"),
            (Case::Six { reverse: true }, "six_reverse"),
        ] {
            let (state, _reset) = fixture();
            let db = TestDb::new();
            let outcome = try_with_attempt(&db, 100_000, || {
                let mut registry = RegistryBuilder::new(&db, &ADMISSION)?;
                match case {
                    Case::Zero => {
                        let values = registry.finite_interned_values_with_memos(
                            MemoLess::ingredient(db.zalsa()),
                            (),
                        )?;
                        let fields = std::array::from_fn(|_| (Collision(0), OwnedMap::new(3, 0)));
                        complete_value_layout(registry, values, fields, name)
                    }
                    Case::One => {
                        let values = registry.finite_interned_values(
                            &db as &dyn Database,
                            Interface::ingredient(db.zalsa()),
                            member_count::fn_ingredient_(&db, db.zalsa()),
                        )?;
                        let fields = std::array::from_fn(|_| (Collision(0), OwnedMap::new(3, 0)));
                        complete_value_layout(registry, values, fields, name)
                    }
                    Case::Two => {
                        let owner = MultipleMemos::ingredient(db.zalsa());
                        let first = first_memo::fn_ingredient_(&db, db.zalsa());
                        let second = second_memo::fn_ingredient_(&db, db.zalsa());
                        let first = registry.passive_memo::<_, _, CopyMemoProfile>(owner, first)?;
                        let second =
                            registry.passive_memo::<_, _, CopyMemoProfile>(owner, second)?;
                        let values =
                            registry.finite_interned_values_with_memos(owner, (second, first))?;
                        let fields =
                            std::array::from_fn(|_| (Collision(0), 0, OwnedMap::new(3, 0)));
                        complete_value_layout(registry, values, fields, name)
                    }
                    Case::Three { reverse } => {
                        let owner = ThreeMemos::ingredient(db.zalsa());
                        let count = composed_count::fn_ingredient_(&db, db.zalsa());
                        let even = composed_even::fn_ingredient_(&db, db.zalsa());
                        let payload = composed_payload::fn_ingredient_(&db, db.zalsa());
                        let count = registry.passive_memo::<_, _, CopyMemoProfile>(owner, count)?;
                        let even = registry.passive_memo::<_, _, CopyMemoProfile>(owner, even)?;
                        let payload =
                            registry.passive_memo::<_, _, BoxedWordsProfile>(owner, payload)?;
                        let fields =
                            std::array::from_fn(|_| (Collision(0), 0, OwnedMap::new(3, 0)));
                        if reverse {
                            let memos = PassiveMemoGroup::new(
                                (payload,),
                                PassiveMemoGroup::new((), (even, count)),
                            );
                            let values =
                                registry.finite_interned_values_with_memos(owner, memos)?;
                            complete_value_layout(registry, values, fields, name)
                        } else {
                            let memos = PassiveMemoGroup::new((count, even), (payload,));
                            let values =
                                registry.finite_interned_values_with_memos(owner, memos)?;
                            complete_value_layout(registry, values, fields, name)
                        }
                    }
                    Case::Six { reverse } => {
                        let owner = SixMemos::ingredient(db.zalsa());
                        let count = six_count::fn_ingredient_(&db, db.zalsa());
                        let even = six_even::fn_ingredient_(&db, db.zalsa());
                        let left = six_left::fn_ingredient_(&db, db.zalsa());
                        let right = six_right::fn_ingredient_(&db, db.zalsa());
                        let historical = six_historical::fn_ingredient_(&db, db.zalsa());
                        let indirect = six_indirect::fn_ingredient_(&db, db.zalsa());
                        let a = registry.passive_memo::<_, _, CopyMemoProfile>(owner, count)?;
                        let b = registry.passive_memo::<_, _, CopyMemoProfile>(owner, even)?;
                        let c = registry.passive_memo::<_, _, WordsProfile<0>>(owner, left)?;
                        let d = registry.passive_memo::<_, _, WordsProfile<0>>(owner, right)?;
                        let e =
                            registry.passive_memo::<_, _, WordsProfile<0>>(owner, historical)?;
                        let f = registry.passive_memo::<_, _, CopyMemoProfile>(owner, indirect)?;
                        let fields =
                            std::array::from_fn(|_| (Collision(0), 0, OwnedMap::new(3, 0)));
                        if reverse {
                            let memos = PassiveMemoGroup::new(
                                (f, e),
                                PassiveMemoGroup::new((d,), PassiveMemoGroup::new((b, c), (a,))),
                            );
                            let values =
                                registry.finite_interned_values_with_memos(owner, memos)?;
                            complete_value_layout(registry, values, fields, name)
                        } else {
                            let memos = PassiveMemoGroup::new(
                                PassiveMemoGroup::new((a, c), (b,)),
                                PassiveMemoGroup::new((d, e), (f,)),
                            );
                            let values =
                                registry.finite_interned_values_with_memos(owner, memos)?;
                            complete_value_layout(registry, values, fields, name)
                        }
                    }
                }
            });
            assert!(
                matches!(outcome, Ok(AttemptOutcome::Complete(Ok(_)))),
                "{name}: {outcome:?}"
            );
            idle(&db);
            assert!(!state.values.active.get());
            assert_eq!(state.values.attempted.get(), 2);
            assert_eq!(&*state.values.work.borrow(), &[cold_map_work(4, 1).as_slice(), &hit_map_work(4, 1)].concat());
        }
    }

    fn six_ids(db: &dyn Database) -> Vec<Id> {
        SixMemos::ingredient(db.zalsa())
            .entries(db.zalsa())
            .map(|entry| entry.key().key_index())
            .collect()
    }

    fn six_slots(db: &dyn Database) -> [IngredientIndex; 6] {
        fn index<C>(db: &dyn Database, memo: &IngredientImpl<C>) -> (usize, IngredientIndex)
        where
            C: for<'a> Configuration<SalsaStruct<'a> = SixMemos<'a>>,
        {
            let slot = memo
                .passive_memo_index(db.zalsa(), SixMemos::ingredient(db.zalsa()))
                .unwrap_or_else(|error| panic!("{}", error.value_message()));
            (slot.as_usize(), memo.index)
        }
        let mut slots = [
            index(db, six_count::fn_ingredient_(db, db.zalsa())),
            index(db, six_even::fn_ingredient_(db, db.zalsa())),
            index(db, six_left::fn_ingredient_(db, db.zalsa())),
            index(db, six_right::fn_ingredient_(db, db.zalsa())),
            index(db, six_historical::fn_ingredient_(db, db.zalsa())),
            index(db, six_indirect::fn_ingredient_(db, db.zalsa())),
        ];
        slots.sort_by_key(|entry| entry.0);
        assert_eq!(slots.map(|entry| entry.0), [0, 1, 2, 3, 4, 5]);
        slots.map(|entry| entry.1)
    }

    fn populate_six(db: &dyn Database, id: Id, occupancy: u8) {
        let value = SixMemos::from_id(id);
        if occupancy & 1 != 0 {
            six_count(db, value);
        }
        if occupancy & 2 != 0 {
            six_even(db, value);
        }
        if occupancy & 4 != 0 {
            six_left(db, value);
        }
        if occupancy & 8 != 0 {
            six_right(db, value);
        }
        if occupancy & 16 != 0 {
            six_historical(db, value);
        }
        if occupancy & 32 != 0 {
            six_indirect(db, value);
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    struct SixSnapshot {
        count: Option<(usize, Option<usize>)>,
        even: Option<(usize, Option<bool>)>,
        left: Option<(usize, Option<Vec<u32>>)>,
        right: Option<(usize, Option<Vec<u32>>)>,
        historical: Option<(usize, Option<Vec<u32>>)>,
        indirect: Option<(usize, Option<u32>)>,
    }

    impl SixSnapshot {
        fn present(&self) -> [bool; 6] {
            [
                self.count.is_some(),
                self.even.is_some(),
                self.left.is_some(),
                self.right.is_some(),
                self.historical.is_some(),
                self.indirect.is_some(),
            ]
        }

        fn output_work(&self) -> usize {
            [&self.left, &self.right, &self.historical]
                .into_iter()
                .filter_map(|memo| memo.as_ref()?.1.as_ref())
                .map(Vec::len)
                .sum()
        }
    }

    fn six_snapshot(db: &dyn Database, id: Id) -> SixSnapshot {
        fn scalar<'db, C: Configuration>(
            db: &'db dyn Database,
            ingredient: &'db IngredientImpl<C>,
            id: Id,
        ) -> Option<(usize, Option<C::Output<'db>>)>
        where
            for<'a> C::Output<'a>: Copy,
        {
            stored(db, ingredient, id)
                .map(|memo| (std::ptr::from_ref(memo).addr(), memo.value().copied()))
        }
        fn words<C>(
            db: &dyn Database,
            ingredient: &IngredientImpl<C>,
            id: Id,
        ) -> Option<(usize, Option<Vec<u32>>)>
        where
            C: for<'a> Configuration<Output<'a> = PassiveWords>,
        {
            stored(db, ingredient, id).map(|memo| {
                (
                    std::ptr::from_ref(memo).addr(),
                    memo.value().map(|value| value.words.to_vec()),
                )
            })
        }
        SixSnapshot {
            count: scalar(db, six_count::fn_ingredient_(db, db.zalsa()), id),
            even: scalar(db, six_even::fn_ingredient_(db, db.zalsa()), id),
            left: words(db, six_left::fn_ingredient_(db, db.zalsa()), id),
            right: words(db, six_right::fn_ingredient_(db, db.zalsa()), id),
            historical: words(db, six_historical::fn_ingredient_(db, db.zalsa()), id),
            indirect: scalar(db, six_indirect::fn_ingredient_(db, db.zalsa()), id),
        }
    }

    fn stale_six(
        occupancy: u8,
        metadata: u32,
        evict_right: bool,
    ) -> (&'static TestDb, ComposedRequest, Id) {
        let mut db = TestDb::new();
        let request = ComposedRequest::new(&db, 0, metadata);
        let count = <SixMemos<'static> as crate::interned::Configuration>::REVISIONS.get();
        let mut old = None;
        for index in 0..count {
            if index != 0 {
                request.set_left(&mut db).to(index as u32);
            }
            state().values.tag.set(usize::from(index == 0));
            state()
                .values
                .composed
                .payload_tag
                .set(usize::from(index == 0));
            let id = six_caller(&db, request);
            if index == 0 {
                old = Some(id);
                populate_six(&db, id, occupancy);
                if evict_right {
                    state().values.tag.set(0);
                    state().values.composed.payload_tag.set(0);
                    let recent = ComposedRequest::new(&db, 100, metadata);
                    let recent = SixMemos::from_id(six_caller(&db, recent));
                    six_right(&db, recent);
                }
            } else if evict_right {
                six_right(&db, SixMemos::from_id(id));
            }
        }
        state().values.tag.set(0);
        state().values.composed.payload_tag.set(0);
        request.set_left(&mut db).to(count as u32);
        let request = ComposedRequest::new(&db, count as u32, metadata);
        state().values.work.borrow_mut().clear();
        state().events.borrow_mut().clear();
        state().journal.borrow_mut().clear();
        (Box::leak(Box::new(db)), request, old.unwrap())
    }

    fn assert_six_caller(db: &dyn Database, request: ComposedRequest, id: Id) {
        assert_eq!(six_caller(db, request), id);
        let memo = stored(
            db,
            six_caller::fn_ingredient_(db, db.zalsa()),
            request.as_id(),
        )
        .unwrap();
        assert_eq!(memo.header.revisions.durability, Durability::LOW);
        assert!(
            memo.header
                .origin()
                .inputs()
                .any(|key| key == SixMemos::ingredient(db.zalsa()).database_key_index(id))
        );
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ComposedFault {
        Work,
        Discard,
        Panic,
        Reenter,
        ReenterRefuse,
    }

    struct ComposedHook {
        owner: Rc<ValueHook>,
        request: ComposedRequest,
        old: Id,
        slots: [IngredientIndex; 6],
        retired_units: usize,
        fault: ComposedFault,
        fired: Cell<bool>,
        new_id: Cell<Option<Id>>,
        replacement: RefCell<Option<SixSnapshot>>,
        payload: Arc<()>,
    }

    impl ComposedHook {
        fn reject(&self) {
            let endpoint = self.owner.endpoint.borrow_mut().take().unwrap();
            let marker = Marker("child");
            let _reply = endpoint
                .demand::<(), _>(move || async move {
                    let _marker = marker;
                    panic!("rejected composed retirement polled its queued child");
                })
                .unwrap();
            if self.fault == ComposedFault::Panic {
                panic_any(Payload(self.payload.clone()));
            }
            attempt_probe::report_incomplete(self.owner.db, Incomplete::Interrupted);
        }

        fn admit(&self, units: usize) -> RunResult<()> {
            if self.fault == ComposedFault::Work
                && units == self.retired_units
                && self.owner.detached.live.get() == 0
                && !self.fired.replace(true)
            {
                self.owner.note("combined work");
                self.reject();
                return Err(RunError::Refused(Incomplete::Interrupted));
            }
            Ok(())
        }

        fn event(&self, event: (KeyEvent, DatabaseKeyIndex)) {
            if event.0 != KeyEvent::Discard
                || event.1.key_index() != self.old
                || !self.slots.contains(&event.1.ingredient_index())
            {
                return;
            }
            let db = self.owner.db;
            if event.1.ingredient_index() == self.slots[0] {
                self.owner.note("first discard");
                if matches!(
                    self.fault,
                    ComposedFault::Reenter | ComposedFault::ReenterRefuse
                ) {
                    state().values.composed.payload_tag.set(4);
                    let value = SixMemos::new(
                        db,
                        Collision(self.request.left(db)),
                        self.request.right(db),
                        OwnedMap::new(self.request.left(db) + 1, 0),
                    );
                    self.new_id.set(Some(value.as_id()));
                    populate_six(db, value.as_id(), 1 | 4 | 8);
                    *self.replacement.borrow_mut() = Some(six_snapshot(db, value.as_id()));
                    state().values.composed.payload_tag.set(0);
                }
            }
            if event.1.ingredient_index() != self.slots[5] || self.fired.replace(true) {
                return;
            }
            self.owner.note("last discard");
            assert_eq!(self.owner.detached.live.get(), 1);
            if self.fault != ComposedFault::Reenter {
                self.reject();
            }
        }
    }

    fn six_discard_events(
        db: &dyn Database,
        old: Id,
        snapshot: &SixSnapshot,
    ) -> Vec<(KeyEvent, DatabaseKeyIndex)> {
        let logical = [
            six_count::fn_ingredient_(db, db.zalsa()).index,
            six_even::fn_ingredient_(db, db.zalsa()).index,
            six_left::fn_ingredient_(db, db.zalsa()).index,
            six_right::fn_ingredient_(db, db.zalsa()).index,
            six_historical::fn_ingredient_(db, db.zalsa()).index,
            six_indirect::fn_ingredient_(db, db.zalsa()).index,
        ];
        six_slots(db)
            .into_iter()
            .filter(|function| {
                logical
                    .into_iter()
                    .zip(snapshot.present())
                    .any(|(actual, present)| actual == *function && present)
            })
            .map(|function| (KeyEvent::Discard, DatabaseKeyIndex::new(function, old)))
            .collect()
    }

    // Both schema arrangements in run_six need nineteen visits for an empty old table:
    // two input visits, one LRU visit, two old-field visits, eight schema nodes, and six
    // slots. Present outputs add one profile visit each; their packed input edges need
    // no output scan. The final quotation quantum is therefore thirty-two in these fixtures.
    fn six_retirement_work(fields: usize, retired: usize) -> [usize; 31] {
        [
            1, 1, 1, 1, 1, 1, 1, 2, fields, 1, 1, 4, fields, 1, 1, 8, fields,
            1, 1, 16, fields, 1, 1, 32, fields, 1, fields, retired, 1, 32, 79,
        ]
    }

    #[test]
    fn composed_six_memo_shapes_retire_actual_payloads_in_slot_order() {
        for reverse in [false, true] {
            for occupancy in [0, 1 | 4 | 8, 63] {
                let (state, _reset) = fixture();
                let (db, request, old) = stale_six(occupancy, 0, false);
                let before = six_snapshot(db, old);
                if occupancy & 32 != 0 {
                    let memo =
                        stored(db, six_indirect::fn_ingredient_(db, db.zalsa()), old).unwrap();
                    assert!(memo.header.origin().inputs().next().is_some());
                    assert!(memo.header.outputs_are_empty());
                }
                let detached = Rc::new(detached_observation::State::default());
                let _observer = detached_observation::install(detached.clone());
                let new = complete_six(db, request, reverse);
                assert_eq!(new.index(), old.index());
                assert_eq!(new.generation(), old.generation() + 1);
                assert_eq!(
                    &*state.values.work.borrow(),
&six_retirement_work(request.left(db) as usize + 2, 2 + before.output_work())
                );
                assert_eq!(
                    (
                        detached.live.get(),
                        detached.acquired.get(),
                        detached.retired.get()
                    ),
                    (0, 1, 1)
                );
                let mut expected = six_discard_events(db, old, &before);
                expected.push((
                    KeyEvent::Reuse,
                    SixMemos::ingredient(db.zalsa()).database_key_index(new),
                ));
                assert_eq!(&*state.events.borrow(), &expected);
                for (kind, present) in [
                    before.left.is_some(),
                    before.right.is_some(),
                    before.historical.is_some(),
                ]
                .into_iter()
                .enumerate()
                {
                    assert_eq!(
                        state.values.composed.drops[kind + 1].get(),
                        usize::from(present)
                    );
                    assert_eq!(state.values.composed.live[kind + 1].get(), 0);
                }
                assert_six_caller(db, request, new);
                assert_eq!(six_count(db, SixMemos::from_id(new)), 1);
                assert_eq!(
                    six_even(db, SixMemos::from_id(new)),
                    request.left(db) % 2 == 0
                );
                let fresh = ComposedRequest::new(db, request.left(db), request.right(db));
                assert_eq!(complete_six(db, fresh, !reverse), new);
                assert_six_caller(db, fresh, new);
            }
        }
    }

    #[test]
    fn composed_schema_distinguishes_absent_and_evicted_values() {
        for empty in [false, true] {
            let (state, _reset) = fixture();
            let (db, request, old) = stale_six(if empty { 4 } else { 4 | 8 }, 0, !empty);
            let before = six_snapshot(db, old);
            if empty {
                assert!(before.right.is_none());
            } else {
                assert!(
                    before.right.as_ref().unwrap().1.is_none(),
                    "ordinary LRU must remove the old payload"
                );
                assert_eq!(state.values.composed.drops[2].get(), 1);
            }
            // A refusing profile must not run for either an absent or a value-less allocation.
            let outcome = try_with_attempt(db, 100_000, || run_six::<0, 1, 0>(db, request, true));
            let Ok(AttemptOutcome::Complete(Ok(new))) = outcome else {
                panic!("{outcome:?}")
            };
            idle(db);
            assert_eq!(new.index(), old.index());
            assert_eq!(
                &*state.values.work.borrow(),
                &six_retirement_work(request.left(db) as usize + 2, 5)
            );
            let discards = state
                .events
                .borrow()
                .iter()
                .copied()
                .filter(|event| event.0 == KeyEvent::Discard)
                .collect::<Vec<_>>();
            assert_eq!(discards, six_discard_events(db, old, &before));
            assert_eq!(state.values.composed.drops[1].get(), 1);
            assert_eq!(state.values.composed.drops[2].get(), usize::from(!empty));
        }
    }

    #[test]
    fn composed_output_refusal_and_sum_overflow_preserve_the_old_generation() {
        for overflow in [false, true] {
            let (state, _reset) = fixture();
            let (db, request, old) = stale_six(63, 0, false);
            let before = six_snapshot(db, old);
            let ids = six_ids(db);
            let stamp = Stamp::current(db);
            let detached = Rc::new(detached_observation::State::default());
            let _observer = detached_observation::install(detached.clone());
            state.values.tag.set(2);
            for attempt in 1..=2 {
                let outcome = try_with_attempt(db, 100_000, || {
                    let result = if overflow {
                        run_six::<2, 0, 0>(db, request, false)
                    } else {
                        run_six::<0, 1, 0>(db, request, false)
                    };
                    assert_eq!(
                        result,
                        Err(RunError::Contract(if overflow {
                            "finite interned value memo retirement work overflow"
                        } else {
                            "retirement quotation is unsupported"
                        }))
                    );
                    result
                });
                assert_eq!(
                    outcome,
                    Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
                );
                assert_eq!(six_ids(db), ids);
                assert_eq!(six_snapshot(db, old), before);
                assert_eq!(detached.acquired.get(), 0);
                assert_eq!(state.values.drops[2].get(), attempt);
                assert_eq!(state.values.live[2].get(), 0);
                assert_eq!(
                    state.values.drop_reason[2].get(),
                    Some(Incomplete::Interrupted)
                );
                for tag in 1..=3 {
                    assert_eq!(state.values.composed.drops[tag].get(), 0);
                }
                assert!(state.delivered.borrow().is_empty());
                assert!(state.events.borrow().is_empty());
                assert!(
                    stored(
                        db,
                        six_caller::fn_ingredient_(db, db.zalsa()),
                        request.as_id()
                    )
                    .is_none()
                );
                assert_eq!(Stamp::current(db), stamp);
                idle(db);
            }
        }
    }

    #[test]
    fn composed_historical_metadata_is_rejected_before_its_profile() {
        for metadata in [
            1,
            #[cfg(feature = "accumulator")]
            2,
        ] {
            let (state, _reset) = fixture();
            let (db, request, old) = stale_six(63, metadata, false);
            let historical =
                stored(db, six_historical::fn_ingredient_(db, db.zalsa()), old).unwrap();
            check_multiple_metadata(historical, metadata == 2);
            let before = six_snapshot(db, old);
            let ids = six_ids(db);
            let stamp = Stamp::current(db);
            let detached = Rc::new(detached_observation::State::default());
            let _observer = detached_observation::install(detached.clone());
            state.values.tag.set(2);
            for attempt in 1..=2 {
                let outcome = try_with_attempt(db, 100_000, || {
                    // Its profile would refuse too; metadata must be checked first.
                    let result = run_six::<0, 0, 1>(db, request, false);
                    assert_eq!(
                        result,
                        Err(RunError::Contract(if metadata == 2 {
                            "finite interned value memo has direct accumulators"
                        } else {
                            "finite interned value memo has tracked outputs"
                        }))
                    );
                    result
                });
                assert_eq!(
                    outcome,
                    Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
                );
                assert_eq!(six_snapshot(db, old), before);
                assert_eq!(six_ids(db), ids);
                assert_eq!(detached.acquired.get(), 0);
                assert_eq!(state.values.drops[2].get(), attempt);
                assert!(state.delivered.borrow().is_empty());
                assert!(
                    stored(
                        db,
                        six_caller::fn_ingredient_(db, db.zalsa()),
                        request.as_id()
                    )
                    .is_none()
                );
                assert_eq!(Stamp::current(db), stamp);
                idle(db);
            }
        }
    }

    #[test]
    fn composed_old_field_quote_overflow_preserves_the_old_generation() {
        // Only six_left is present. Its profile quotes usize::MAX, so adding the old
        // fields' work overflows even though summing the memo outputs alone does not.
        let (state, _reset) = fixture();
        let (db, request, old) = stale_six(4, 0, false);
        let stamp = Stamp::current(db);
        let before = six_snapshot(db, old);
        let ids = six_ids(db);
        let detached = Rc::new(detached_observation::State::default());
        let _observer = detached_observation::install(detached.clone());
        let outcome = try_with_attempt(db, 100_000, || {
            let result = run_six::<2, 0, 0>(db, request, false);
            assert_eq!(
                result,
                Err(RunError::Contract(
                    "retirement quotation work overflow"
                ))
            );
            result
        });
        assert_eq!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        );
        assert_eq!(six_ids(db), ids);
        assert_eq!(six_snapshot(db, old), before);
        assert_eq!(
            (
                detached.live.get(),
                detached.acquired.get(),
                detached.retired.get()
            ),
            (0, 0, 0)
        );
        assert_eq!(state.values.composed.drops[1].get(), 0);
        assert_eq!(state.values.live[1].get(), 1);
        assert!(state.delivered.borrow().is_empty());
        assert!(state.events.borrow().is_empty());
        assert!(
            stored(
                db,
                six_caller::fn_ingredient_(db, db.zalsa()),
                request.as_id()
            )
            .is_none()
        );
        idle(db);
        let new = complete_six(db, request, false);
        assert_eq!(new.index(), old.index());
        assert_eq!(new.generation(), old.generation() + 1);
        assert_eq!(state.values.composed.drops[1].get(), 1);
        assert_eq!(state.values.drops[1].get(), 1);
        for reverse in [false, true] {
            assert_eq!(complete_six(db, request, reverse), new);
            assert_six_caller(db, request, new);
            assert_eq!(Stamp::current(db), stamp);
        }
    }

    #[cfg(feature = "accumulator")]
    #[test]
    fn composed_schema_allows_indirect_accumulated_inputs() {
        let (state, _reset) = fixture();
        let (db, request, old) = stale_six(63, 4, false);
        let memo = stored(db, six_indirect::fn_ingredient_(db, db.zalsa()), old).unwrap();
        assert!(memo.header.revisions.accumulated().is_none());
        assert!(
            memo.header
                .origin()
                .inputs()
                .any(|key| key.ingredient_index()
                    == six_indirect_source::fn_ingredient_(db, db.zalsa()).index)
        );
        let source = six_indirect_source::intern_ingredient_(db.zalsa())
            .entries(db.zalsa())
            .next()
            .unwrap()
            .key();
        assert!(
            stored(
                db,
                six_indirect_source::fn_ingredient_(db, db.zalsa()),
                source.key_index()
            )
            .unwrap()
            .header
            .revisions
            .accumulated()
            .is_some()
        );
        let new = complete_six(db, request, true);
        assert_eq!(new.index(), old.index());
        assert_eq!(
            &*state.values.work.borrow(),
            &six_retirement_work(request.left(db) as usize + 2, 17)
        );
        assert_eq!(state.values.composed.drops[3].get(), 1);
    }

    #[test]
    fn composed_retirement_orders_cleanup_and_preserves_reentrant_memos() {
        // Field tags one and two identify the old and incoming fields. Output tags one
        // through three identify old memo payloads; four and five identify replacements
        // created by the reentrant callback.
        for fault in [
            ComposedFault::Work,
            ComposedFault::Discard,
            ComposedFault::Panic,
            ComposedFault::Reenter,
            ComposedFault::ReenterRefuse,
        ] {
            let (state, _reset) = fixture();
            let (db, request, old) = stale_six(63, 0, false);
            let before = six_snapshot(db, old);
            let stamp = Stamp::current(db);
            let detached = Rc::new(detached_observation::State::default());
            let _observer = detached_observation::install(detached.clone());
            let owner = Rc::new(ValueHook {
                db,
                caller: six_caller::fn_ingredient_(db, db.zalsa())
                    .database_key_index(request.as_id()),
                selected: None,
                fault: ValueFault::QuietReuse,
                endpoint: RefCell::new(None),
                fired: Cell::new(false),
                input_quoted: Cell::new(false),
                detached: detached.clone(),
                notes: RefCell::new(Vec::new()),
            });
            let hook = Rc::new(ComposedHook {
                owner: owner.clone(),
                request,
                old,
                slots: six_slots(db),
                retired_units: 2 + before.output_work(),
                fault,
                fired: Cell::new(false),
                new_id: Cell::new(None),
                replacement: RefCell::new(None),
                payload: Arc::new(()),
            });
            *state.values.hook.borrow_mut() = Some(owner.clone());
            *state.values.composed.hook.borrow_mut() = Some(hook.clone());
            state.values.tag.set(2);
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                try_with_attempt(db, 100_000, || {
                    let result = run_six::<0, 0, 0>(db, request, false);
                    state.direct_error.set(result.as_ref().err().copied());
                    result
                })
            }));
            let success = fault == ComposedFault::Reenter;
            if fault == ComposedFault::Panic {
                let payload = outcome.unwrap_err().downcast::<Payload>().unwrap();
                assert!(Arc::ptr_eq(&payload.0, &hook.payload));
            } else if success {
                let Ok(Ok(AttemptOutcome::Complete(Ok(id)))) = outcome else {
                    panic!("{outcome:?}")
                };
                assert_eq!(hook.new_id.get(), Some(id));
            } else {
                assert_eq!(
                    outcome.unwrap(),
                    Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
                );
                assert_eq!(
                    state.direct_error.get(),
                    Some(RunError::Refused(Incomplete::Interrupted))
                );
            }
            assert!(hook.fired.get(), "{fault:?}");
            if fault == ComposedFault::Work {
                assert_eq!(six_snapshot(db, old), before);
                assert!(six_ids(db).contains(&old));
                assert_eq!(detached.acquired.get(), 0);
                assert_eq!(detached.retired.get(), 0);
                assert_eq!(state.values.live[1].get(), 1);
                assert_eq!(state.values.drops[1].get(), 0);
                assert_eq!(state.values.drops[2].get(), 1);
                for tag in 1..=3 {
                    assert_eq!(state.values.composed.live[tag].get(), 1);
                    assert_eq!(state.values.composed.drops[tag].get(), 0);
                }
                assert!(state.events.borrow().is_empty());
                assert!(state.delivered.borrow().is_empty());
                assert!(stored(db, six_caller::fn_ingredient_(db, db.zalsa()), request.as_id()).is_none());
                assert_eq!(&*state.journal.borrow(), &["child", "caller", "root"]);
                let notes = owner.notes.borrow();
                let child = notes.iter().find(|note| note.stage == "child").unwrap();
                assert_eq!(child.frame, Some(owner.caller));
                assert!(child.claimed);
                assert_eq!(child.detached, 0);
                assert_eq!(child.live[1], 1);
                assert_eq!(child.live[2], 1);
                drop(notes);
                assert!(state.values.child_order.get() < state.values.drop_order[2].get());
                assert!(state.values.drop_order[2].get() < state.values.caller_order.get());
                let charges = six_retirement_work(request.left(db) as usize + 2, hook.retired_units);
                assert_eq!(&*state.values.work.borrow(), &charges[..charges.len() - 3]);
                *state.values.composed.hook.borrow_mut() = None;
                *state.values.hook.borrow_mut() = None;
                state.values.tag.set(0);
                idle(db);
                let new = complete_six(db, request, false);
                assert_eq!(new.index(), old.index());
                assert_eq!(new.generation(), old.generation() + 1);
                assert_eq!(complete_six(db, request, true), new);
                assert_six_caller(db, request, new);
                assert_eq!(state.values.drops[1].get(), 1);
                for tag in 1..=3 {
                    assert_eq!(state.values.composed.live[tag].get(), 0);
                    assert_eq!(state.values.composed.drops[tag].get(), 1);
                }
                assert_eq!(Stamp::current(db), stamp);
                continue;
            }
            let new = six_ids(db)
                .into_iter()
                .find(|id| id.index() == old.index())
                .unwrap();
            assert_eq!(new.generation(), old.generation() + 1);
            assert_eq!(
                (
                    detached.live.get(),
                    detached.acquired.get(),
                    detached.retired.get()
                ),
                (0, 1, 1)
            );
            assert_eq!(state.values.drops[1].get(), 1);
            for tag in 1..=3 {
                assert_eq!(state.values.composed.drops[tag].get(), 1);
                assert_eq!(state.values.composed.live[tag].get(), 0);
            }
            if !success {
                assert!(state.delivered.borrow().is_empty());
                assert!(
                    stored(
                        db,
                        six_caller::fn_ingredient_(db, db.zalsa()),
                        request.as_id()
                    )
                    .is_none()
                );
                assert_eq!(&*state.journal.borrow(), &["child", "caller", "root"]);
                let notes = owner.notes.borrow();
                let child = notes.iter().find(|note| note.stage == "child").unwrap();
                assert_eq!(child.frame, Some(owner.caller));
                assert!(child.claimed);
                assert_eq!(child.detached, 1);
                assert_eq!(child.live[1], 1);
                for tag in 1..=3 {
                    assert!(
                        state.values.child_order.get()
                            < state.values.composed.drop_order[tag].get()
                    );
                    assert!(
                        state.values.composed.drop_order[tag].get()
                            < state.values.caller_order.get()
                    );
                }
                assert!(state.values.child_order.get() < state.values.drop_order[1].get());
                assert!(state.values.drop_order[1].get() < state.values.caller_order.get());
            }
            let reentered = matches!(fault, ComposedFault::Reenter | ComposedFault::ReenterRefuse);
            if reentered {
                assert_eq!(
                    six_snapshot(db, new),
                    *hook.replacement.borrow().as_ref().unwrap()
                );
                assert_eq!(state.values.composed.drops[4].get(), 0);
                assert_eq!(state.values.composed.drops[5].get(), 0);
                assert_eq!(state.values.composed.live[4].get(), 1);
                assert_eq!(state.values.composed.live[5].get(), 1);
            } else {
                assert_eq!(six_snapshot(db, new).present(), [false; 6]);
            }
            let discards = state
                .events
                .borrow()
                .iter()
                .copied()
                .filter(|event| event.0 == KeyEvent::Discard && event.1.key_index() == old)
                .collect::<Vec<_>>();
            assert_eq!(discards, six_discard_events(db, old, &before));
            *state.values.composed.hook.borrow_mut() = None;
            *state.values.hook.borrow_mut() = None;
            state.values.tag.set(0);
            idle(db);
            for reverse in [false, true] {
                assert_eq!(complete_six(db, request, reverse), new);
                assert_six_caller(db, request, new);
                if reentered {
                    assert_eq!(
                        six_snapshot(db, new),
                        *hook.replacement.borrow().as_ref().unwrap()
                    );
                }
                assert_eq!(Stamp::current(db), stamp);
            }
        }
    }

    #[test]
    fn composed_schema_rejects_incomplete_duplicate_and_foreign_mappings() {
        let (state, _reset) = fixture();
        let db = TestDb::new();
        let foreign = TestDb::new();
        let owner = SixMemos::ingredient(db.zalsa());
        let _slots = six_slots(&db);
        for case in 0..3 {
            let outcome = try_with_attempt(&db, 100_000, || {
                let mut registry = RegistryBuilder::new(&db, &ADMISSION)?;
                let a = registry.passive_memo::<_, _, CopyMemoProfile>(
                    owner,
                    six_count::fn_ingredient_(&db, db.zalsa()),
                )?;
                let b = registry.passive_memo::<_, _, CopyMemoProfile>(
                    owner,
                    six_even::fn_ingredient_(&db, db.zalsa()),
                )?;
                let c = registry.passive_memo::<_, _, WordsProfile<0>>(
                    owner,
                    six_left::fn_ingredient_(&db, db.zalsa()),
                )?;
                let d = registry.passive_memo::<_, _, WordsProfile<0>>(
                    owner,
                    six_right::fn_ingredient_(&db, db.zalsa()),
                )?;
                let e = registry.passive_memo::<_, _, WordsProfile<0>>(
                    owner,
                    six_historical::fn_ingredient_(&db, db.zalsa()),
                )?;
                macro_rules! rejected {
                    ($schema:expr) => {
                        assert!(matches!(
                            registry.finite_interned_values_with_memos(owner, $schema),
                            Err(RunError::Contract(
                                "finite interned value memo mapping is unsupported"
                            ))
                        ));
                    };
                }
                if case == 0 {
                    rejected!(PassiveMemoGroup::new(
                        (a, b),
                        PassiveMemoGroup::new((c, d), (e,))
                    ));
                } else {
                    let duplicate = registry.passive_memo::<_, _, CopyMemoProfile>(
                        owner,
                        six_count::fn_ingredient_(&db, db.zalsa()),
                    )?;
                    if case == 1 {
                        rejected!(PassiveMemoGroup::new(
                            (a, duplicate),
                            PassiveMemoGroup::new((b, c), (d, e))
                        ));
                    } else {
                        rejected!(PassiveMemoGroup::new(
                            (a, b),
                            PassiveMemoGroup::new((c, d), (e, duplicate))
                        ));
                    }
                }
                Ok::<_, RunError>(())
            });
            assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
            assert_eq!(owner.entries(db.zalsa()).count(), 0);
        }
        let outcome = try_with_attempt(&db, 100_000, || {
            let mut registry = RegistryBuilder::new(&db, &ADMISSION)?;
            for (actual_owner, function) in [
                (
                    SixMemos::ingredient(foreign.zalsa()),
                    six_count::fn_ingredient_(&db, db.zalsa()),
                ),
                (owner, six_count::fn_ingredient_(&foreign, foreign.zalsa())),
            ] {
                assert!(matches!(
                    registry.passive_memo::<_, _, CopyMemoProfile>(actual_owner, function),
                    Err(RunError::Contract(
                        "finite interned value memo mapping is unsupported"
                    ))
                ));
            }
            Ok::<_, RunError>(())
        });
        assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
        assert_eq!(owner.entries(db.zalsa()).count(), 0);
        assert!(six_ids(&foreign).is_empty());
        assert!(state.events.borrow().is_empty());
        idle(&db);
    }

    #[test]
    fn composed_values_cannot_cross_registry_ownership() {
        let (state, _reset) = fixture();
        let db = TestDb::new();
        let owner = ThreeMemos::ingredient(db.zalsa());
        let outcome = try_with_attempt(&db, 100_000, || {
            let mut registry = RegistryBuilder::new(&db, &ADMISSION)?;
            let a = registry.passive_memo::<_, _, CopyMemoProfile>(
                owner,
                composed_count::fn_ingredient_(&db, db.zalsa()),
            )?;
            let b = registry.passive_memo::<_, _, CopyMemoProfile>(
                owner,
                composed_even::fn_ingredient_(&db, db.zalsa()),
            )?;
            let c = registry.passive_memo::<_, _, BoxedWordsProfile>(
                owner,
                composed_payload::fn_ingredient_(&db, db.zalsa()),
            )?;
            let values = registry
                .finite_interned_values_with_memos(owner, PassiveMemoGroup::new((a, b), (c,)))?;
            let other = RegistryBuilder::new(&db, &ADMISSION)?;
            let result = other.seal()?.run(move |endpoint| async move {
                let value = endpoint
                    .intern_value(&values, (Collision(0), 0, OwnedMap::new(1, 2)))
                    .await;
                super::state().delivered.borrow_mut().push(value.as_id());
                Ok(value.as_id())
            });
            assert_eq!(
                result,
                Err(RunError::Contract(
                    "finite interned value capability is foreign"
                ))
            );
            result
        });
        assert_eq!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        );
        assert_eq!(owner.entries(db.zalsa()).count(), 0);
        assert!(state.delivered.borrow().is_empty());
        assert!(state.values.work.borrow().is_empty());
        assert_eq!(state.values.drops[2].get(), 1);
        idle(&db);
    }

    struct MultipleProvider<'db, I: FiniteInternedConfiguration, S, D: Configuration> {
        values: InternedValues<'db, I, S>,
        caller: Route<'db, D>,
    }

    impl<'run, 'db: 'run, I, S, D> ExecutableRouteProvider<'run, 'db, D>
        for MultipleProvider<'db, I, S, D>
    where
        I: FiniteInternedConfiguration
            + for<'a> crate::interned::Configuration<Fields<'a> = (Collision, u32, OwnedMap)>,
        S: PassiveMemoSchema<'db, I> + 'run,
        D: for<'a> Configuration<DbView = dyn Database, Input<'a> = Request, Output<'a> = Id>,
    {
        // Request conversion constructs a handle; Id equality compares its scalar identity.
        fixture_native_value!(executable, 'run, 'db, D, 1);

        async fn body(
            &'run self,
            context: ProviderContext<'run, 'db, Self>,
            db: &'db dyn Database,
            input: Request,
        ) -> RunResult<Id> {
            let _caller = Marker("caller");
            let fields = multiple_fields(db, input);
            let value = {
                let _scope = ValueScope::new();
                context.endpoint().intern_value(&self.values, fields).await
            };
            state().delivered.borrow_mut().push(value.as_id());
            Ok(value.as_id())
        }

        async fn initial(
            &'run self,
            _context: ProviderContext<'run, 'db, Self>,
            _db: &'db dyn Database,
            _id: Id,
            _input: Request,
        ) -> RunResult<Id> {
            Err(RunError::RequiresFetch)
        }

        async fn recover<'call>(
            &'run self,
            _context: ProviderContext<'run, 'db, Self>,
            _db: &'db dyn Database,
            _cycle: &'call Cycle<'call>,
            _last: &'call Id,
            _value: Id,
            _input: Request,
        ) -> RunResult<Id>
        where
            'run: 'call,
        {
            Err(RunError::RequiresFetch)
        }
    }

    fn finish_multiple<I, S, D>(
        mut registry: RegistryBuilder<'static, 'static>,
        values: InternedValues<'static, I, S>,
        db: &'static TestDb,
        request: Request,
        caller: &'static IngredientImpl<D>,
    ) -> RunResult<Id>
    where
        I: FiniteInternedConfiguration
            + for<'a> crate::interned::Configuration<Fields<'a> = (Collision, u32, OwnedMap)>,
        S: PassiveMemoSchema<'static, I> + 'static,
        D: for<'a> Configuration<DbView = dyn Database, Input<'a> = Request, Output<'a> = Id>,
    {
        let caller = registry.reserve(db as &dyn Database, caller)?;
        let provider: &'static _ = Box::leak(Box::new(MultipleProvider { values, caller }));
        let binding = registry.provider(provider)?;
        registry.bind_executable(&provider.caller, &binding)?;
        registry.seal()?.run(move |endpoint| async move {
            let _root = Marker("root");
            if let Some(hook) = state().values.hook.borrow().as_ref() {
                *hook.endpoint.borrow_mut() = Some(endpoint.clone());
            }
            Ok(*endpoint
                .provider(binding)?
                .fetch_ref(&provider.caller, request.as_id())?
                .await?)
        })
    }

    fn run_multiple(db: &'static TestDb, request: Request, reverse: bool) -> RunResult<Id> {
        let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
        let owner = MultipleMemos::ingredient(db.zalsa());
        let first = registry.passive_memo::<_, _, CopyMemoProfile>(
            owner,
            first_memo::fn_ingredient_(db, db.zalsa()),
        )?;
        let second = registry.passive_memo::<_, _, CopyMemoProfile>(
            owner,
            second_memo::fn_ingredient_(db, db.zalsa()),
        )?;
        if reverse {
            let values = registry.finite_interned_values_with_memos(owner, (second, first))?;
            finish_multiple(
                registry,
                values,
                db,
                request,
                multiple_caller::fn_ingredient_(db, db.zalsa()),
            )
        } else {
            let values = registry.finite_interned_values_with_memos(owner, (first, second))?;
            finish_multiple(
                registry,
                values,
                db,
                request,
                multiple_caller::fn_ingredient_(db, db.zalsa()),
            )
        }
    }

    fn complete_multiple(db: &'static TestDb, request: Request, reverse: bool) -> Id {
        let result = try_with_attempt(db, 100_000, || run_multiple(db, request, reverse));
        let Ok(AttemptOutcome::Complete(Ok(id))) = result else {
            panic!("{result:?}");
        };
        idle(db);
        assert!(!state().values.active.get());
        id
    }

    fn multiple_ids(db: &dyn Database) -> Vec<Id> {
        MultipleMemos::ingredient(db.zalsa())
            .entries(db.zalsa())
            .map(|entry| entry.key().key_index())
            .collect()
    }

    fn multiple_slots(db: &dyn Database) -> [IngredientIndex; 2] {
        let owner = MultipleMemos::ingredient(db.zalsa());
        let first = first_memo::fn_ingredient_(db, db.zalsa());
        let second = second_memo::fn_ingredient_(db, db.zalsa());
        let index = first
            .passive_memo_index(db.zalsa(), owner)
            .unwrap_or_else(|error| panic!("{}", error.value_message()))
            .as_usize();
        let other = second
            .passive_memo_index(db.zalsa(), owner)
            .unwrap_or_else(|error| panic!("{}", error.value_message()))
            .as_usize();
        assert_eq!(index + other, 1);
        if index == 0 {
            [first.index, second.index]
        } else {
            [second.index, first.index]
        }
    }

    fn populate_multiple(db: &dyn Database, id: Id, occupancy: u8) {
        let slots = multiple_slots(db);
        let value = MultipleMemos::from_id(id);
        for (slot, function) in slots.into_iter().enumerate() {
            if occupancy & (1 << slot) == 0 {
                continue;
            }
            if function == first_memo::fn_ingredient_(db, db.zalsa()).index {
                first_memo(db, value);
            } else {
                second_memo(db, value);
            }
        }
    }

    fn multiple_memos(db: &dyn Database, id: Id) -> (usize, usize, usize, bool) {
        let first = stored(db, first_memo::fn_ingredient_(db, db.zalsa()), id).unwrap();
        let second = stored(db, second_memo::fn_ingredient_(db, db.zalsa()), id).unwrap();
        (
            std::ptr::from_ref(first).addr(),
            *first.value().unwrap(),
            std::ptr::from_ref(second).addr(),
            *second.value().unwrap(),
        )
    }

    fn stale_multiple(occupancy: u8) -> (&'static TestDb, Request, Id) {
        let mut db = TestDb::new();
        let request = Request::new(&db, 0, 0, None);
        let count = <MultipleMemos<'static> as crate::interned::Configuration>::REVISIONS.get();
        let mut old = None;
        for index in 0..count {
            if index != 0 {
                request.set_left(&mut db).to(index as u32);
            }
            state().values.tag.set(usize::from(index == 0));
            let id = multiple_caller(&db, request);
            if index == 0 {
                old = Some(id);
                populate_multiple(&db, id, occupancy);
            }
        }
        state().values.tag.set(0);
        request.set_left(&mut db).to(count as u32);
        let request = Request::new(&db, count as u32, 0, None);
        (Box::leak(Box::new(db)), request, old.unwrap())
    }

    fn assert_multiple_result(db: &dyn Database, request: Request, id: Id) {
        assert_eq!(multiple_caller(db, request), id);
        let value = MultipleMemos::from_id(id);
        assert_eq!(first_memo(db, value), 2 * request.left(db) as usize + 1);
        assert_eq!(second_memo(db, value), request.left(db) % 2 == 0);
        let memo = stored(
            db,
            multiple_caller::fn_ingredient_(db, db.zalsa()),
            request.as_id(),
        )
        .unwrap();
        assert_eq!(memo.header.revisions.durability, Durability::LOW);
        assert!(
            memo.header
                .origin()
                .inputs()
                .any(|key| key == MultipleMemos::ingredient(db.zalsa()).database_key_index(id))
        );
    }

    #[test]
    fn two_memo_reuse_reports_populated_slots_in_storage_order() {
        for occupancy in 0..4 {
            for reverse in [false, true] {
                let (state, _reset) = fixture();
                let (db, request, old) = stale_multiple(occupancy);
                let slots = multiple_slots(db);
                state.events.borrow_mut().clear();
                let new = complete_multiple(db, request, reverse);
                assert_eq!(new.index(), old.index());
                assert_eq!(new.generation(), old.generation() + 1);
                let expected = slots
                    .into_iter()
                    .enumerate()
                    .filter(|(slot, _)| occupancy & (1 << slot) != 0)
                    .map(|(_, function)| (KeyEvent::Discard, DatabaseKeyIndex::new(function, old)))
                    .chain(std::iter::once((
                        KeyEvent::Reuse,
                        MultipleMemos::ingredient(db.zalsa()).database_key_index(new),
                    )))
                    .collect::<Vec<_>>();
                let events = state
                    .events
                    .borrow()
                    .iter()
                    .copied()
                    .filter(|event| {
                        (event.0 == KeyEvent::Discard
                            && event.1.key_index() == old
                            && slots.contains(&event.1.ingredient_index()))
                            || (event.0 == KeyEvent::Reuse
                                && event.1.ingredient_index()
                                    == MultipleMemos::ingredient(db.zalsa()).ingredient_index())
                    })
                    .collect::<Vec<_>>();
                assert_eq!(events, expected);
                assert_multiple_result(db, request, new);
            }
        }
    }

    mod metadata_memos {
        use super::*;

        // Metadata-producing queries cannot share the ReturnOnly configurations used by reentry.
        #[crate::interned]
        pub(super) struct Values<'db> {
            #[returns(copy)]
            salt: Collision,
            #[returns(copy)]
            metadata: u32,
            #[returns(ref)]
            members: OwnedMap,
        }

        impl FiniteInternedConfiguration for Values<'static> {
            fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
                fields.2.work()
            }

            fn field_work_bounded(
                fields: &Self::Fields<'_>,
                fuel: &mut QuoteFuel,
            ) -> Result<usize, QuoteError> {
                fields.2.work_bounded(fuel)
            }
        }

        #[crate::tracked(returns(copy), attempt = CompleteOnly)]
        pub(super) fn first(db: &dyn Database, value: Values<'_>) -> usize {
            multiple_metadata(db, value.salt(db).0, value.metadata(db), 1);
            value.salt(db).0 as usize
        }

        #[crate::tracked(returns(copy), attempt = CompleteOnly)]
        pub(super) fn second(db: &dyn Database, value: Values<'_>) -> bool {
            multiple_metadata(db, value.salt(db).0, value.metadata(db), 2);
            value.salt(db).0 % 2 == 0
        }

        #[crate::tracked(returns(copy), attempt = ReturnOnly)]
        pub(super) fn caller(db: &dyn Database, request: Request) -> Id {
            let (salt, metadata, members) = multiple_fields(db, request);
            Values::new(db, salt, metadata, members).as_id()
        }

        pub(super) fn slots(db: &dyn Database) -> [IngredientIndex; 2] {
            let owner = Values::ingredient(db.zalsa());
            let first = first::fn_ingredient_(db, db.zalsa());
            let second = second::fn_ingredient_(db, db.zalsa());
            let index = first
                .passive_memo_index(db.zalsa(), owner)
                .unwrap_or_else(|error| panic!("{}", error.value_message()))
                .as_usize();
            let other = second
                .passive_memo_index(db.zalsa(), owner)
                .unwrap_or_else(|error| panic!("{}", error.value_message()))
                .as_usize();
            assert_eq!(index + other, 1);
            if index == 0 {
                [first.index, second.index]
            } else {
                [second.index, first.index]
            }
        }

        pub(super) fn ids(db: &dyn Database) -> Vec<Id> {
            Values::ingredient(db.zalsa())
                .entries(db.zalsa())
                .map(|entry| entry.key().key_index())
                .collect()
        }

        pub(super) fn memos(db: &dyn Database, id: Id) -> (usize, usize, usize, bool) {
            let first = stored(db, first::fn_ingredient_(db, db.zalsa()), id).unwrap();
            let second = stored(db, second::fn_ingredient_(db, db.zalsa()), id).unwrap();
            (
                std::ptr::from_ref(first).addr(),
                *first.value().unwrap(),
                std::ptr::from_ref(second).addr(),
                *second.value().unwrap(),
            )
        }

        pub(super) fn assert_second_slot_metadata(db: &dyn Database, id: Id, accumulated: bool) {
            let first = first::fn_ingredient_(db, db.zalsa());
            let second = second::fn_ingredient_(db, db.zalsa());
            if slots(db)[1] == first.index {
                check_multiple_metadata(stored(db, first, id).unwrap(), accumulated);
                assert!(stored(db, second, id).unwrap().header.outputs_are_empty());
            } else {
                check_multiple_metadata(stored(db, second, id).unwrap(), accumulated);
                assert!(stored(db, first, id).unwrap().header.outputs_are_empty());
            }
        }

        pub(super) fn assert_no_caller_memo(db: &dyn Database, request: Request) {
            assert!(stored(db, caller::fn_ingredient_(db, db.zalsa()), request.as_id()).is_none());
        }

        pub(super) fn stale(accumulated: bool) -> (&'static TestDb, Request, Id) {
            let mut db = TestDb::new();
            let slot_one = slots(&db)[1];
            let selected = if slot_one == first::fn_ingredient_(&db, db.zalsa()).index {
                1
            } else {
                2
            };
            let kind = selected | if accumulated { 4 } else { 0 };
            let request = Request::new(&db, 0, kind, None);
            let count = <Values<'static> as crate::interned::Configuration>::REVISIONS.get();
            let mut old = None;
            for index in 0..count {
                if index != 0 {
                    request.set_left(&mut db).to(index as u32);
                }
                state().values.tag.set(usize::from(index == 0));
                let id = caller(&db, request);
                if index == 0 {
                    old = Some(id);
                    first(&db, Values::from_id(id));
                    second(&db, Values::from_id(id));
                }
            }
            state().values.tag.set(0);
            request.set_left(&mut db).to(count as u32);
            let request = Request::new(&db, count as u32, kind, None);
            (Box::leak(Box::new(db)), request, old.unwrap())
        }

        pub(super) fn run(db: &'static TestDb, request: Request) -> RunResult<Id> {
            let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
            let owner = Values::ingredient(db.zalsa());
            let first = registry.passive_memo::<_, _, CopyMemoProfile>(
                owner,
                first::fn_ingredient_(db, db.zalsa()),
            )?;
            let second = registry.passive_memo::<_, _, CopyMemoProfile>(
                owner,
                second::fn_ingredient_(db, db.zalsa()),
            )?;
            let values = registry.finite_interned_values_with_memos(owner, (first, second))?;
            finish_multiple(
                registry,
                values,
                db,
                request,
                caller::fn_ingredient_(db, db.zalsa()),
            )
        }
    }

    fn check_multiple_metadata<C: Configuration>(memo: &Memo<C>, accumulated: bool) {
        if !accumulated {
            assert_eq!(memo.header.revisions.tracked_struct_ids().len(), 1);
            assert!(!memo.header.outputs_are_empty());
            assert!(memo.header.origin().outputs().next().is_none());
        }
        #[cfg(feature = "accumulator")]
        if accumulated {
            assert!(memo.header.outputs_are_empty());
            assert!(memo.header.revisions.accumulated().is_some());
        }
    }

    #[test]
    fn two_memo_second_slot_metadata_refuses_before_replacement() {
        let kinds = [
            false,
            #[cfg(feature = "accumulator")]
            true,
        ];
        for accumulated in kinds {
            let (state, _reset) = fixture();
            let (db, request, old) = metadata_memos::stale(accumulated);
            metadata_memos::assert_second_slot_metadata(db, old, accumulated);
            let before = metadata_memos::memos(db, old);
            let ids = metadata_memos::ids(db);
            let stamp = Stamp::current(db);
            let detached = Rc::new(detached_observation::State::default());
            let _observer = detached_observation::install(detached.clone());
            state.values.tag.set(2);
            for attempt in 1..=2 {
                let result = try_with_attempt(db, 100_000, || {
                    let result = metadata_memos::run(db, request);
                    assert_eq!(
                        result,
                        Err(RunError::Contract(if accumulated {
                            "finite interned value memo has direct accumulators"
                        } else {
                            "finite interned value memo has tracked outputs"
                        }))
                    );
                    result
                });
                assert_eq!(
                    result,
                    Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
                );
                assert_eq!(metadata_memos::ids(db), ids);
                assert_eq!(metadata_memos::memos(db, old), before);
                assert_eq!(detached.acquired.get(), 0);
                assert_eq!(state.values.drops[2].get(), attempt);
                assert_eq!(state.values.live[2].get(), 0);
                assert_eq!(
                    state.values.drop_reason[2].get(),
                    Some(Incomplete::Interrupted)
                );
                assert!(state.delivered.borrow().is_empty());
                metadata_memos::assert_no_caller_memo(db, request);
                assert_eq!(Stamp::current(db), stamp);
                idle(db);
            }
        }
    }

    #[derive(Clone, Copy, Eq, PartialEq)]
    enum MultipleFault {
        Refuse,
        Panic,
        Reenter,
        ReenterRefuse,
    }

    struct MultipleHook {
        owner: Rc<ValueHook>,
        request: Request,
        first: DatabaseKeyIndex,
        second: DatabaseKeyIndex,
        first_seen: Cell<bool>,
        fired: Cell<bool>,
        fault: MultipleFault,
        new_id: Cell<Option<Id>>,
        memos: Cell<Option<(usize, usize, usize, bool)>>,
        payload: Arc<()>,
    }

    impl MultipleHook {
        fn event(&self, event: (KeyEvent, DatabaseKeyIndex)) {
            if event == (KeyEvent::Discard, self.first) {
                assert!(!self.first_seen.replace(true));
                self.owner.note("first discard");
            }
            if event != (KeyEvent::Discard, self.second) || self.fired.replace(true) {
                return;
            }
            assert!(self.first_seen.get());
            self.owner.note("second discard");
            assert_eq!(self.owner.detached.live.get(), 1);
            let db = self.owner.db;
            let new = multiple_ids(db)
                .into_iter()
                .find(|id| id.index() == self.second.key_index().index())
                .unwrap();
            assert_eq!(new.generation(), self.second.key_index().generation() + 1);
            self.new_id.set(Some(new));
            if matches!(
                self.fault,
                MultipleFault::Reenter | MultipleFault::ReenterRefuse
            ) {
                let value = MultipleMemos::new(
                    db,
                    Collision(self.request.left(db)),
                    self.request.right(db),
                    OwnedMap::new(self.request.left(db) + 1, 0),
                );
                assert_eq!(value.as_id(), new);
                populate_multiple(db, new, 3);
                self.memos.set(Some(multiple_memos(db, new)));
            }
            if self.fault == MultipleFault::Reenter {
                return;
            }
            let endpoint = self.owner.endpoint.borrow_mut().take().unwrap();
            let marker = Marker("child");
            let _reply = endpoint
                .demand::<(), _>(move || async move {
                    let _marker = marker;
                    panic!("rejected second discard polled its queued child");
                })
                .unwrap();
            if self.fault == MultipleFault::Panic {
                panic_any(Payload(self.payload.clone()));
            }
            attempt_probe::report_incomplete(db, Incomplete::Interrupted);
        }
    }

    fn multiple_fault_fixture(
        fault: MultipleFault,
    ) -> (&'static TestDb, Request, Id, Rc<MultipleHook>, impl Drop) {
        let (db, request, old) = stale_multiple(3);
        let slots = multiple_slots(db);
        // Both allocations exist before detachment; Copy values need no synthetic drop guards.
        let _both = multiple_memos(db, old);
        let detached = Rc::new(detached_observation::State::default());
        let observer = detached_observation::install(detached.clone());
        let owner = Rc::new(ValueHook {
            db,
            caller: multiple_caller::fn_ingredient_(db, db.zalsa())
                .database_key_index(request.as_id()),
            selected: None,
            fault: ValueFault::QuietReuse,
            endpoint: RefCell::new(None),
            fired: Cell::new(false),
            input_quoted: Cell::new(false),
            detached,
            notes: RefCell::new(Vec::new()),
        });
        let hook = Rc::new(MultipleHook {
            owner: owner.clone(),
            request,
            first: DatabaseKeyIndex::new(slots[0], old),
            second: DatabaseKeyIndex::new(slots[1], old),
            first_seen: Cell::new(false),
            fired: Cell::new(false),
            fault,
            new_id: Cell::new(None),
            memos: Cell::new(None),
            payload: Arc::new(()),
        });
        let state = state();
        *state.values.hook.borrow_mut() = Some(owner);
        *state.values.multiple_hook.borrow_mut() = Some(hook.clone());
        state.values.tag.set(2);
        state.events.borrow_mut().clear();
        state.journal.borrow_mut().clear();
        (db, request, old, hook, observer)
    }

    fn reject_multiple(db: &'static TestDb, request: Request, hook: &MultipleHook) {
        let result = catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(db, 100_000, || {
                let result = run_multiple(db, request, true);
                state().direct_error.set(result.as_ref().err().copied());
                result
            })
        }));
        if hook.fault == MultipleFault::Panic {
            let payload = result.unwrap_err().downcast::<Payload>().unwrap();
            assert!(Arc::ptr_eq(&payload.0, &hook.payload));
        } else {
            assert_eq!(
                result.unwrap(),
                Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
            );
            assert_eq!(
                state().direct_error.get(),
                Some(RunError::Refused(Incomplete::Interrupted))
            );
        }
        assert!(hook.fired.get());
        let state = state();
        assert!(state.delivered.borrow().is_empty());
        assert!(
            stored(
                db,
                multiple_caller::fn_ingredient_(db, db.zalsa()),
                request.as_id()
            )
            .is_none()
        );
        assert_eq!(&*state.journal.borrow(), &["child", "caller", "root"]);
        let notes = hook.owner.notes.borrow();
        for stage in ["first discard", "second discard", "child"] {
            let note = notes.iter().find(|note| note.stage == stage).unwrap();
            assert_eq!(note.frame, Some(hook.owner.caller));
            assert!(note.claimed);
            assert_eq!(note.detached, 1);
            assert_eq!(note.live[1], 1);
            if stage == "child" && hook.fault != MultipleFault::Panic {
                assert_eq!(note.reason, Some(Incomplete::Interrupted));
            }
        }
        assert!(state.values.child_order.get() < state.values.drop_order[1].get());
        assert!(state.values.drop_order[1].get() < state.values.caller_order.get());
        assert_eq!(state.values.drops[1].get(), 1);
        assert_eq!(state.values.detached_at_drop[1].get(), 0);
        assert_eq!(
            (
                hook.owner.detached.live.get(),
                hook.owner.detached.acquired.get(),
                hook.owner.detached.retired.get()
            ),
            (0, 1, 1)
        );
        assert!(
            !state
                .events
                .borrow()
                .iter()
                .any(|event| event.0 == KeyEvent::Reuse
                    && Some(event.1.key_index()) == hook.new_id.get()
                    && event.1.ingredient_index()
                        == MultipleMemos::ingredient(db.zalsa()).ingredient_index())
        );
        idle(db);
    }

    fn stop_multiple_hook() {
        *state().values.multiple_hook.borrow_mut() = None;
        *state().values.hook.borrow_mut() = None;
        state().values.tag.set(0);
    }

    #[test]
    fn two_memo_second_discard_retains_detached_owner() {
        for fault in [MultipleFault::Refuse, MultipleFault::Panic] {
            let (_state, _reset) = fixture();
            let (db, request, old, hook, _observer) = multiple_fault_fixture(fault);
            let stamp = Stamp::current(db);
            reject_multiple(db, request, &hook);
            let new = hook.new_id.get().unwrap();
            assert_eq!(new.index(), old.index());
            assert_eq!(new.generation(), old.generation() + 1);
            assert!(stored(db, first_memo::fn_ingredient_(db, db.zalsa()), new).is_none());
            assert!(stored(db, second_memo::fn_ingredient_(db, db.zalsa()), new).is_none());
            stop_multiple_hook();
            for reverse in [false, true] {
                assert_eq!(complete_multiple(db, request, reverse), new);
                assert_multiple_result(db, request, new);
                assert_eq!(Stamp::current(db), stamp);
            }
        }
    }

    #[test]
    fn two_memo_discard_reentry_preserves_both_replacement_memos() {
        for fault in [MultipleFault::Reenter, MultipleFault::ReenterRefuse] {
            let (_state, _reset) = fixture();
            let (db, request, _old, hook, _observer) = multiple_fault_fixture(fault);
            let stamp = Stamp::current(db);
            if fault == MultipleFault::Reenter {
                let new = complete_multiple(db, request, true);
                assert_eq!(hook.new_id.get(), Some(new));
                assert!(hook.fired.get());
                assert_eq!(
                    (
                        hook.owner.detached.live.get(),
                        hook.owner.detached.acquired.get(),
                        hook.owner.detached.retired.get()
                    ),
                    (0, 1, 1)
                );
            } else {
                reject_multiple(db, request, &hook);
            }
            let new = hook.new_id.get().unwrap();
            let memos = hook.memos.get().unwrap();
            assert_eq!(multiple_memos(db, new), memos);
            stop_multiple_hook();
            for reverse in [false, true] {
                assert_eq!(complete_multiple(db, request, reverse), new);
                assert_multiple_result(db, request, new);
                assert_eq!(multiple_memos(db, new), memos);
                assert_eq!(Stamp::current(db), stamp);
            }
        }
    }

    #[test]
    fn schema_and_foreign_capabilities_are_rejected() {
        let (state, _reset) = fixture();
        let db = TestDb::new();
        let foreign = TestDb::new();
        assert!(matches!(
            try_with_attempt(&db, 100_000, || {
                let mut registry = RegistryBuilder::new(&db, &ADMISSION)?;
                let _actual_second_type = second_memo::fn_ingredient_(&db, db.zalsa());
                let result = registry.finite_interned_values(
                    &db as &dyn Database,
                    MultipleMemos::ingredient(db.zalsa()),
                    first_memo::fn_ingredient_(&db, db.zalsa()),
                );
                assert!(matches!(
                    result,
                    Err(RunError::Contract(
                        "finite interned value memo mapping is unsupported"
                    ))
                ));
                assert_eq!(
                    MultipleMemos::ingredient(db.zalsa())
                        .entries(db.zalsa())
                        .count(),
                    0
                );
                Ok::<_, RunError>(())
            }),
            Ok(AttemptOutcome::Complete(Ok(())))
        ));
        assert!(matches!(
            try_with_attempt(&db, 100_000, || {
                let mut registry = RegistryBuilder::new(&db, &ADMISSION)?;
                let result = registry.finite_interned_values(
                    &db as &dyn Database,
                    Interface::ingredient(foreign.zalsa()),
                    member_count::fn_ingredient_(&foreign, foreign.zalsa()),
                );
                assert!(matches!(
                    result,
                    Err(RunError::Contract(
                        "finite interned value ingredient is foreign"
                    ))
                ));
                Ok::<_, RunError>(())
            }),
            Ok(AttemptOutcome::Complete(Ok(())))
        ));
        assert_eq!(
            try_with_attempt(&db, 100_000, || {
                let mut owner = RegistryBuilder::new(&db, &ADMISSION)?;
                let values = owner.finite_interned_values(
                    &db as &dyn Database,
                    Interface::ingredient(db.zalsa()),
                    member_count::fn_ingredient_(&db, db.zalsa()),
                )?;
                let other = RegistryBuilder::new(&db, &ADMISSION)?;
                let result = other.seal()?.run(move |endpoint| async move {
                    let value = endpoint
                        .intern_value(&values, (Collision(0), OwnedMap::new(1, 2)))
                        .await;
                    super::state().delivered.borrow_mut().push(value.as_id());
                    Ok(value.as_id())
                });
                assert_eq!(
                    result,
                    Err(RunError::Contract(
                        "finite interned value capability is foreign"
                    ))
                );
                result
            }),
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        );
        assert!(value_ids(&db).is_empty());
        assert!(value_ids(&foreign).is_empty());
        assert!(state.delivered.borrow().is_empty());
        assert!(state.values.work.borrow().is_empty());
        assert_eq!(state.values.drops[2].get(), 1);
        assert_eq!(
            state.values.drop_reason[2].get(),
            Some(Incomplete::Interrupted)
        );
        idle(&db);
    }
}
